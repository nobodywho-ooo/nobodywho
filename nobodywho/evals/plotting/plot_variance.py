#!/usr/bin/env python3
"""Plot benchmark results with variance from multiple runs."""

import re
from pathlib import Path
from typing import Annotated, Optional

import matplotlib.pyplot as plt
import numpy as np
import pandas as pd
import typer

app = typer.Typer(
    help="Plot benchmark results with variance from multiple runs",
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


def load_results(csv_paths: list[Path], last_n: int) -> pd.DataFrame:
    """Load results from CSV files, taking last N runs per file."""
    dfs = []
    for path in csv_paths:
        if not path.exists():
            typer.echo(f"Warning: {path} not found, skipping")
            continue
        df = pd.read_csv(path)
        if len(df) > last_n:
            df = df.tail(last_n)
        if len(df) > 0:
            dfs.append(df)

    if not dfs:
        raise typer.Exit("No valid CSV files found")

    return pd.concat(dfs, ignore_index=True)


def aggregate_by_model(df: pd.DataFrame, benchmarks: list[str]) -> pd.DataFrame:
    """Aggregate runs by model, computing mean and std for each benchmark."""
    grouped = df.groupby("model_name")

    rows = []
    for model_name, group in grouped:
        row = {
            "model_name": model_name,
            "model_size_gb": group["model_size_gb"].iloc[0],
            "n_runs": len(group),
        }

        if "tokens_per_second" in group.columns:
            row["tokens_per_second_mean"] = group["tokens_per_second"].mean()
            row["tokens_per_second_std"] = group["tokens_per_second"].std()

        for bench in benchmarks:
            if bench in group.columns:
                values = group[bench].dropna()
                if len(values) > 0:
                    row[f"{bench}_mean"] = values.mean()
                    row[f"{bench}_std"] = values.std() if len(values) > 1 else 0.0
                else:
                    row[f"{bench}_mean"] = np.nan
                    row[f"{bench}_std"] = 0.0

        rows.append(row)

    return pd.DataFrame(rows)


@app.command()
def plot(
    models: Annotated[
        list[Path],
        typer.Argument(help="Path(s) to CSV result files"),
    ],
    last_n: Annotated[
        int,
        typer.Option("-n", "--last", help="Number of recent runs to use per model for variance calculation"),
    ] = 1,
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
            help="Sort models by: 'size', 'name', or a benchmark name",
        ),
    ] = "size",
    benchmarks: Annotated[
        Optional[str],
        typer.Option(
            "-b",
            "--benchmarks",
            help="Comma-separated benchmarks to plot (default: all)",
        ),
    ] = None,
    show_throughput: Annotated[
        bool, typer.Option("--throughput/--no-throughput", help="Show throughput bars")
    ] = True,
    figsize: Annotated[
        str, typer.Option("--figsize", help="Figure size as WxH (e.g., 14x6)")
    ] = "14x6",
    error_style: Annotated[
        str,
        typer.Option(
            "--error-style",
            help="Error visualization: 'band' (shaded area), 'bar' (error bars), or 'both'",
        ),
    ] = "band",
    band_alpha: Annotated[
        float,
        typer.Option("--band-alpha", help="Opacity of variance bands (0.0-1.0)"),
    ] = 0.15,
    line_width: Annotated[
        float,
        typer.Option("--line-width", help="Width of benchmark lines"),
    ] = 2.0,
    marker_size: Annotated[
        float,
        typer.Option("--marker-size", help="Size of data point markers"),
    ] = 5,
    no_grid: Annotated[
        bool,
        typer.Option("--no-grid", help="Hide grid lines"),
    ] = False,
):
    """Generate benchmark plot with variance from multiple runs.

    Use -n/--last to specify how many recent runs to include for variance calculation.
    With -n 1 (default), no variance is shown. With -n 5, uses last 5 runs per model.
    """
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
    df = load_results(csv_files, last_n)

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

    # Aggregate by model
    agg_df = aggregate_by_model(df, list(plot_benchmarks.keys()))

    # Sort models
    if sort_by == "size":
        agg_df = agg_df.sort_values("model_size_gb")
    elif sort_by == "name":
        agg_df = agg_df.sort_values("model_name")
    elif f"{sort_by}_mean" in agg_df.columns:
        agg_df = agg_df.sort_values(f"{sort_by}_mean")

    # Create model labels
    agg_df["label"] = agg_df.apply(
        lambda row: extract_model_label(
            row["model_name"], row.get("model_size_gb", 0)
        ),
        axis=1,
    )

    # Parse figure size
    try:
        w, h = map(float, figsize.split("x"))
    except ValueError:
        w, h = 14, 6

    # Create plot
    fig, ax1 = plt.subplots(figsize=(w, h))

    x = np.arange(len(agg_df))
    x_labels = agg_df["label"].tolist()

    # Plot throughput bars if available
    if show_throughput and "tokens_per_second_mean" in agg_df.columns:
        ax2 = ax1.twinx()
        throughput_mean = agg_df["tokens_per_second_mean"].fillna(0)
        throughput_std = agg_df["tokens_per_second_std"].fillna(0)

        ax2.bar(
            x,
            throughput_mean,
            alpha=0.3,
            color="gray",
            label="Throughput",
            zorder=1,
            yerr=throughput_std if error_style in ("bar", "both") and last_n > 1 else None,
            capsize=2,
        )
        ax2.set_ylabel("Throughput (tokens/sec)", color="gray")
        ax2.tick_params(axis="y", labelcolor="gray")
        max_throughput = throughput_mean.max()
        ax2.set_ylim(0, max_throughput * 1.2 if max_throughput > 0 else 300)

    # Plot benchmark lines with variance
    for col, config in plot_benchmarks.items():
        mean_col = f"{col}_mean"
        std_col = f"{col}_std"

        if mean_col not in agg_df.columns:
            continue

        means = agg_df[mean_col].fillna(0)
        stds = agg_df[std_col].fillna(0)

        # Plot mean line
        ax1.plot(
            x,
            means,
            marker="o",
            label=config["label"],
            color=config["color"],
            linewidth=line_width,
            markersize=marker_size,
            zorder=3,
        )

        # Plot variance visualization (only if using multiple runs)
        if last_n > 1:
            if error_style in ("band", "both"):
                ax1.fill_between(
                    x,
                    means - stds,
                    means + stds,
                    alpha=band_alpha,
                    color=config["color"],
                    zorder=2,
                )

            if error_style in ("bar", "both"):
                ax1.errorbar(
                    x,
                    means,
                    yerr=stds,
                    fmt="none",
                    color=config["color"],
                    capsize=3,
                    capthick=1,
                    zorder=4,
                )

    # Configure axes
    ax1.set_xlabel("Model (Size)")
    ax1.set_ylabel("Score")
    ax1.set_ylim(0, 1.0)
    ax1.set_xticks(x)
    ax1.set_xticklabels(x_labels, rotation=45, ha="right", fontsize=8)
    ax1.legend(loc="center right", fontsize=9)
    if not no_grid:
        ax1.grid(True, alpha=0.3, zorder=0)

    # Add run count annotation if using multiple runs
    if last_n > 1:
        run_counts = agg_df["n_runs"].tolist()
        ax1.set_xlabel(f"Model (Size) - using last {last_n} runs")
        for i, n in enumerate(run_counts):
            if n > 1:
                ax1.annotate(
                    f"n={n}",
                    (i, 0.02),
                    fontsize=6,
                    ha="center",
                    alpha=0.7,
                )

    # Set title
    if title:
        plt.title(title)
    else:
        model_names = agg_df["model_name"].tolist()
        if model_names:
            first = model_names[0]
            suffix = " (with variance)" if last_n > 1 else ""
            if "gemma" in first.lower():
                sizes = set()
                for name in model_names:
                    match = re.search(r"(\d+)[bB]", name)
                    if match:
                        sizes.add(match.group(1) + "b")
                plt.title(f"Gemma {' & '.join(sorted(sizes))} benchmarks{suffix}")
            elif "qwen" in first.lower():
                plt.title(f"Qwen benchmarks{suffix}")
            else:
                plt.title(f"Model benchmarks{suffix}")

    plt.tight_layout()

    # Save or show
    if output:
        plt.savefig(output, dpi=150, bbox_inches="tight")
        typer.echo(f"Saved plot to {output}")
    else:
        plt.show()


@app.command()
def stats(
    models: Annotated[
        list[Path],
        typer.Argument(help="Path(s) to CSV result files"),
    ],
    last_n: Annotated[
        int,
        typer.Option("-n", "--last", help="Number of recent runs to use per model"),
    ] = 5,
    benchmarks: Annotated[
        Optional[str],
        typer.Option(
            "-b",
            "--benchmarks",
            help="Comma-separated benchmarks (default: all)",
        ),
    ] = None,
):
    """Print variance statistics without plotting."""
    csv_files = []
    for path in models:
        if "*" in str(path):
            csv_files.extend(Path(".").glob(str(path)))
        else:
            csv_files.append(path)

    if not csv_files:
        raise typer.BadParameter("No CSV files specified")

    df = load_results(csv_files, last_n)

    if benchmarks:
        selected = benchmarks.split(",")
        bench_list = [k for k in BENCHMARK_CONFIGS.keys() if k in selected]
    else:
        bench_list = [k for k in BENCHMARK_CONFIGS.keys() if k in df.columns]

    agg_df = aggregate_by_model(df, bench_list)

    typer.echo(f"\nVariance Statistics (last {last_n} runs per model)")
    typer.echo("=" * 80)

    for _, row in agg_df.iterrows():
        typer.echo(f"\n{row['model_name']} ({row['n_runs']} runs)")
        typer.echo("-" * 40)

        for bench in bench_list:
            mean_col = f"{bench}_mean"
            std_col = f"{bench}_std"
            if mean_col in row and not pd.isna(row[mean_col]):
                label = BENCHMARK_CONFIGS[bench]["label"]
                mean = row[mean_col]
                std = row[std_col]
                cv = (std / mean * 100) if mean > 0 else 0
                typer.echo(f"  {label:20s}: {mean:.4f} +/- {std:.4f} (CV: {cv:.1f}%)")


if __name__ == "__main__":
    app()
