#!/usr/bin/env python3
"""Plot benchmark results from CSV files."""

import re
from pathlib import Path
from typing import Annotated, Optional

import matplotlib.pyplot as plt
import pandas as pd
import typer

app = typer.Typer(
    help="Plot benchmark results from eval CSV files",
    pretty_exceptions_show_locals=False,
)

# Benchmark columns and display configuration
BENCHMARK_CONFIGS = {
    "ifeval_prompt_level_strict_acc": {"label": "IFEval (Prompt)", "color": "#1f77b4"},
    "ifeval_inst_level_strict_acc": {"label": "IFEval (Inst)", "color": "#ff7f0e"},
    "gsm8k_exact_match__flexible-extract": {"label": "GSM8K", "color": "#2ca02c"},
    "truthfulqa_gen_bleu_acc": {"label": "TruthfulQA", "color": "#d62728"},
    "humaneval_pass_at_1__create_test": {"label": "HumanEval", "color": "#9467bd"},
    "mbpp_pass_at_1": {"label": "MBPP", "color": "#8c564b"},
    "drop_f1": {"label": "DROP (F1)", "color": "#e377c2"},
    "drop_em": {"label": "DROP (EM)", "color": "#7f7f7f"},
    "mmmu_val_science_acc": {"label": "MMMU (Science)", "color": "#17becf"},
    "mmmu_val_humanities_and_social_science_acc": {"label": "MMMU (Humanities)", "color": "#bcbd22"},
}


def extract_model_label(model_name: str, model_size_gb: float) -> str:
    """Extract a short label from model name with size."""
    name = model_name
    for prefix in ["google_", "Qwen_", "results_"]:
        if name.startswith(prefix):
            name = name[len(prefix):]

    quant_match = re.search(r"((?:IQ|Q|UD-IQ|UD-Q)\d+[_\w]*)", name)
    quant = quant_match.group(1) if quant_match else ""

    size_match = re.search(r"(\d+)[bB]", name)
    model_size = size_match.group(1) + "b" if size_match else ""

    size_str = f"{model_size_gb:.1f}GB" if model_size_gb else ""

    parts = [p for p in [model_size, quant, size_str] if p]
    return "-".join(parts) if parts else name


def load_results(csv_paths: list[Path]) -> pd.DataFrame:
    """Load results from CSV files, taking only the last row from each."""
    dfs = []
    for path in csv_paths:
        if not path.exists():
            typer.echo(f"Warning: {path} not found, skipping")
            continue
        df = pd.read_csv(path)
        if len(df) > 0:
            dfs.append(df.tail(1))

    if not dfs:
        raise typer.Exit("No valid CSV files found")

    return pd.concat(dfs, ignore_index=True)


@app.command()
def plot(
    models: Annotated[
        list[Path],
        typer.Argument(help="Path(s) to CSV result files or glob patterns"),
    ],
    title: Annotated[
        Optional[str], typer.Option("-t", "--title", help="Plot title")
    ] = None,
    output: Annotated[
        Optional[Path],
        typer.Option("-o", "--output", help="Output file (PNG, PDF, SVG)"),
    ] = None,
    sort_by: Annotated[
        Optional[str],
        typer.Option(
            "--sort-by",
            help="Sort models by: 'size' (file size), 'name', or a benchmark name",
        ),
    ] = "size",
    benchmarks: Annotated[
        Optional[str],
        typer.Option(
            "-b",
            "--benchmarks",
            help=f"Comma-separated benchmarks to plot (default: all). Options: {', '.join(BENCHMARK_CONFIGS.keys())}",
        ),
    ] = None,
    show_throughput: Annotated[
        bool, typer.Option("--throughput/--no-throughput", help="Show throughput bars")
    ] = True,
    figsize: Annotated[
        str, typer.Option("--figsize", help="Figure size as WxH (e.g., 14x6)")
    ] = "14x6",
):
    """Generate benchmark comparison plot from CSV results (uses latest row from each file)."""
    # Expand glob patterns
    csv_files = []
    for path in models:
        if "*" in str(path):
            csv_files.extend(Path(".").glob(str(path)))
        else:
            csv_files.append(path)

    if not csv_files:
        raise typer.BadParameter("No CSV files specified")

    # Load data
    df = load_results(csv_files)

    # Select benchmarks to plot
    if benchmarks:
        selected = benchmarks.split(",")
        plot_benchmarks = {k: v for k, v in BENCHMARK_CONFIGS.items() if k in selected}
    else:
        plot_benchmarks = {
            k: v for k, v in BENCHMARK_CONFIGS.items() if k in df.columns
        }

    if not plot_benchmarks:
        typer.echo("Error: No valid benchmarks found in data")
        raise typer.Exit(1)

    # Sort models
    if sort_by == "size":
        df = df.sort_values("model_size_gb")
    elif sort_by == "name":
        df = df.sort_values("model_name")
    elif sort_by in df.columns:
        df = df.sort_values(sort_by)

    # Create model labels
    df["label"] = df.apply(
        lambda row: extract_model_label(row["model_name"], row.get("model_size_gb", 0)),
        axis=1,
    )

    # Parse figure size
    try:
        w, h = map(float, figsize.split("x"))
    except ValueError:
        w, h = 14, 6

    # Create plot
    fig, ax1 = plt.subplots(figsize=(w, h))

    x = range(len(df))
    x_labels = df["label"].tolist()

    # Plot throughput bars if available
    if show_throughput and "tokens_per_second" in df.columns:
        ax2 = ax1.twinx()
        ax2.bar(
            x,
            df["tokens_per_second"],
            alpha=0.3,
            color="gray",
            label="Throughput",
            zorder=1,
        )
        ax2.set_ylabel("Throughput (tokens/sec)", color="gray")
        ax2.tick_params(axis="y", labelcolor="gray")
        max_throughput = df["tokens_per_second"].max()
        ax2.set_ylim(0, max_throughput * 1.1 if max_throughput > 0 else 300)

    # Plot benchmark lines
    for col, config in plot_benchmarks.items():
        if col in df.columns:
            values = df[col].fillna(0)
            ax1.plot(
                x,
                values,
                marker="o",
                label=config["label"],
                color=config["color"],
                linewidth=1.5,
                markersize=4,
                zorder=2,
            )

    # Configure axes
    ax1.set_xlabel("Model (Size)")
    ax1.set_ylabel("Score")
    ax1.set_ylim(0, 1.0)
    ax1.set_xticks(x)
    ax1.set_xticklabels(x_labels, rotation=45, ha="right", fontsize=8)
    ax1.legend(loc="center right", fontsize=8)
    ax1.grid(True, alpha=0.3, zorder=0)

    # Set title
    if title:
        plt.title(title)
    else:
        model_names = df["model_name"].tolist()
        if model_names:
            first = model_names[0]
            if "gemma" in first.lower():
                sizes = set()
                for name in model_names:
                    match = re.search(r"(\d+)[bB]", name)
                    if match:
                        sizes.add(match.group(1) + "b")
                plt.title(f"Gemma {' & '.join(sorted(sizes))} benchmarks")
            elif "qwen" in first.lower():
                plt.title("Qwen benchmarks")
            else:
                plt.title("Model benchmarks")

    plt.tight_layout()

    # Save or show
    if output:
        plt.savefig(output, dpi=150, bbox_inches="tight")
        typer.echo(f"Saved plot to {output}")
    else:
        plt.show()


@app.command()
def list_benchmarks():
    """List available benchmark columns."""
    typer.echo("Available benchmarks:")
    for col, config in BENCHMARK_CONFIGS.items():
        typer.echo(f"  {col}: {config['label']}")


if __name__ == "__main__":
    app()
