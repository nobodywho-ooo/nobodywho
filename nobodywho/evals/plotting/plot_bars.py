#!/usr/bin/env python3
"""Plot benchmark results as grouped bar charts for model comparison."""

import re
from pathlib import Path
from typing import Annotated, Optional

import matplotlib.pyplot as plt
import numpy as np
import pandas as pd
import typer

app = typer.Typer(
    help="Plot benchmark results as grouped bar charts",
    pretty_exceptions_show_locals=False,
)

BENCHMARK_CONFIGS = {
    "ifeval_prompt_level_strict_acc": {"label": "IFEval\n(Prompt)", "color": "#1f77b4"},
    "ifeval_inst_level_strict_acc": {"label": "IFEval\n(Inst)", "color": "#ff7f0e"},
    "gsm8k_exact_match__flexible-extract": {"label": "GSM8K", "color": "#2ca02c"},
    "truthfulqa_gen_bleu_acc": {"label": "TruthfulQA", "color": "#d62728"},
    "humaneval_pass_at_1__create_test": {"label": "HumanEval", "color": "#9467bd"},
    "mbpp_pass_at_1": {"label": "MBPP", "color": "#8c564b"},
    "drop_f1": {"label": "DROP\n(F1)", "color": "#e377c2"},
    "drop_em": {"label": "DROP\n(EM)", "color": "#7f7f7f"},
    "mmmu_val_science_acc": {"label": "MMMU\n(Science)", "color": "#17becf"},
    "mmmu_val_humanities_and_social_science_acc": {"label": "MMMU\n(Humanities)", "color": "#bcbd22"},
}


def extract_model_label(model_name: str, model_size_gb: float) -> str:
    """Extract a short label from model name with size."""
    name = model_name
    for prefix in ["google_", "Qwen_", "results_"]:
        if name.startswith(prefix):
            name = name[len(prefix) :]

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
    benchmarks: Annotated[
        Optional[str],
        typer.Option(
            "-b",
            "--benchmarks",
            help=f"Comma-separated benchmarks to plot (default: all present in data). Options: {', '.join(BENCHMARK_CONFIGS.keys())}",
        ),
    ] = None,
    labels: Annotated[
        Optional[str],
        typer.Option(
            "-l",
            "--labels",
            help="Comma-separated labels for each model (must match number of CSV files). Defaults to model_name from CSV.",
        ),
    ] = None,
    sort_by: Annotated[
        Optional[str],
        typer.Option(
            "--sort-by",
            help="Sort models by: 'size' (file size), 'name', or a benchmark column name",
        ),
    ] = "size",
    figsize: Annotated[
        str, typer.Option("--figsize", help="Figure size as WxH (e.g., 14x6)")
    ] = "14x6",
):
    """Generate grouped bar chart comparing models across benchmarks (uses latest row from each file)."""
    # Expand glob patterns
    csv_files = []
    for path in models:
        if "*" in str(path):
            csv_files.extend(Path(".").glob(str(path)))
        else:
            csv_files.append(path)

    if not csv_files:
        raise typer.BadParameter("No CSV files specified")

    df = load_results(csv_files)

    # Select benchmarks
    if benchmarks:
        selected = benchmarks.split(",")
        plot_benchmarks = {k: v for k, v in BENCHMARK_CONFIGS.items() if k in selected}
    else:
        plot_benchmarks = {k: v for k, v in BENCHMARK_CONFIGS.items() if k in df.columns}

    if not plot_benchmarks:
        typer.echo("Error: No valid benchmarks found in data")
        raise typer.Exit(1)

    # Sort models
    if sort_by == "size":
        df = df.sort_values("model_size_gb").reset_index(drop=True)
    elif sort_by == "name":
        df = df.sort_values("model_name").reset_index(drop=True)
    elif sort_by in df.columns:
        df = df.sort_values(sort_by).reset_index(drop=True)

    # Create model labels
    if labels:
        label_list = [l.strip() for l in labels.split(",")]
        if len(label_list) != len(df):
            raise typer.BadParameter(
                f"Number of labels ({len(label_list)}) must match number of models ({len(df)})"
            )
        df["label"] = label_list
    else:
        df["label"] = df["model_name"]

    # Parse figure size
    try:
        w, h = map(float, figsize.split("x"))
    except ValueError:
        w, h = 14, 6

    fig, ax = plt.subplots(figsize=(w, h))

    bench_keys = list(plot_benchmarks.keys())
    bench_labels = [plot_benchmarks[k]["label"] for k in bench_keys]
    n_benchmarks = len(bench_keys)
    n_models = len(df)

    x = np.arange(n_benchmarks)
    bar_width = 0.8 / n_models

    colors = plt.cm.tab10(np.linspace(0, 1, max(n_models, 1)))

    for i, (_, row) in enumerate(df.iterrows()):
        values = [row.get(k, 0) if pd.notna(row.get(k, np.nan)) else 0 for k in bench_keys]
        offset = (i - n_models / 2 + 0.5) * bar_width
        bars = ax.bar(
            x + offset,
            values,
            bar_width,
            label=row["label"],
            color=colors[i],
            edgecolor="white",
            linewidth=0.5,
        )
        # Add value labels on bars
        for bar, val in zip(bars, values):
            if val > 0:
                ax.text(
                    bar.get_x() + bar.get_width() / 2,
                    bar.get_height() + 0.01,
                    f"{val:.2f}",
                    ha="center",
                    va="bottom",
                    fontsize=6,
                    rotation=90,
                )

    ax.set_ylabel("Score")
    ax.set_ylim(0, 1.15)
    ax.set_xticks(x)
    ax.set_xticklabels(bench_labels, fontsize=9)
    ax.legend(loc="upper left", fontsize=8, ncol=min(n_models, 4))
    ax.grid(True, alpha=0.3, axis="y", zorder=0)

    from datetime import datetime

    date_str = datetime.now().strftime("%Y-%m-%d")
    if title:
        ax.set_title(f"{title} ({date_str})")
    else:
        ax.set_title(f"Model Benchmark Comparison ({date_str})")

    plt.tight_layout()

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
        typer.echo(f"  {col}: {config['label'].replace(chr(10), ' ')}")


if __name__ == "__main__":
    app()
