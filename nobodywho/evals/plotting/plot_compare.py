#!/usr/bin/env python3
"""Compare benchmark results across model families."""

import re
from pathlib import Path
from typing import Annotated, Optional

import matplotlib.pyplot as plt
import numpy as np
import pandas as pd
import typer

app = typer.Typer(
    help="Compare benchmark results across model families",
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

# Model family patterns and their short names
FAMILY_PATTERNS = [
    (r"gemma-3-(\d+)b", "gemma3-{0}b"),
    (r"gemma-2-(\d+)b", "gemma2-{0}b"),
    (r"gemma-(\d+)b", "gemma-{0}b"),
    (r"Qwen3-(\d+)B", "qwen3-{0}b"),
    (r"Qwen2-(\d+)B", "qwen2-{0}b"),
    (r"Qwen-(\d+)B", "qwen-{0}b"),
    (r"llama-3-(\d+)b", "llama3-{0}b"),
    (r"llama-2-(\d+)b", "llama2-{0}b"),
    (r"mistral-(\d+)b", "mistral-{0}b"),
    (r"phi-(\d+)", "phi-{0}"),
]

# Colors for different model families
FAMILY_COLORS = {
    "gemma3": "#4285F4",  # Google blue
    "gemma2": "#34A853",  # Google green
    "gemma": "#4285F4",
    "qwen3": "#FF6B6B",   # Red
    "qwen2": "#FF8E8E",
    "qwen": "#FF6B6B",
    "llama3": "#9B59B6",  # Purple
    "llama2": "#8E44AD",
    "llama": "#9B59B6",
    "mistral": "#F39C12", # Orange
    "phi": "#1ABC9C",     # Teal
}


def detect_family(model_name: str) -> tuple[str, str]:
    """Detect model family and size from model name.

    Returns (family_with_size, base_family) e.g., ("gemma3-4b", "gemma3")
    """
    name_lower = model_name.lower()

    for pattern, template in FAMILY_PATTERNS:
        match = re.search(pattern, model_name, re.IGNORECASE)
        if match:
            family_size = template.format(*match.groups())
            base_family = family_size.split("-")[0]
            return family_size, base_family

    if "gemma" in name_lower:
        return "gemma", "gemma"
    if "qwen" in name_lower:
        return "qwen", "qwen"
    if "llama" in name_lower:
        return "llama", "llama"

    return "unknown", "unknown"


def extract_quantization(model_name: str) -> str:
    """Extract quantization type from model name."""
    match = re.search(r"((?:UD-)?(?:IQ|Q)\d+[_\w]*|bf16|F16|f16)", model_name, re.IGNORECASE)
    if match:
        return match.group(1).upper()
    return "unknown"


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


def add_family_info(df: pd.DataFrame) -> pd.DataFrame:
    """Add family and quantization columns to dataframe."""
    families = []
    base_families = []
    quants = []

    for _, row in df.iterrows():
        family, base = detect_family(row["model_name"])
        families.append(family)
        base_families.append(base)
        quants.append(extract_quantization(row["model_name"]))

    df = df.copy()
    df["family"] = families
    df["base_family"] = base_families
    df["quantization"] = quants
    return df


@app.command()
def plot(
    models: Annotated[
        list[Path],
        typer.Argument(help="Path(s) to CSV result files"),
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
        typer.Option("-b", "--benchmarks", help="Comma-separated benchmarks to plot"),
    ] = None,
    group_by: Annotated[
        str,
        typer.Option(
            "--group-by",
            help="How to group models: 'family' (group by model family), 'quantization' (group by quant type), 'none'",
        ),
    ] = "family",
    sort_by: Annotated[
        str,
        typer.Option("--sort-by", help="Sort within groups: 'size', 'name', 'quantization'"),
    ] = "size",
    show_throughput: Annotated[
        bool, typer.Option("--throughput/--no-throughput", help="Show throughput bars")
    ] = True,
    figsize: Annotated[
        str, typer.Option("--figsize", help="Figure size as WxH")
    ] = "16x7",
):
    """Compare models across families with grouped visualization (uses latest row from each file)."""
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
    df = add_family_info(df)

    # Select benchmarks
    if benchmarks:
        selected = benchmarks.split(",")
        plot_benchmarks = {k: v for k, v in BENCHMARK_CONFIGS.items() if k in selected}
    else:
        plot_benchmarks = {k: v for k, v in BENCHMARK_CONFIGS.items() if k in df.columns}

    if not plot_benchmarks:
        typer.echo("Error: No valid benchmarks found")
        raise typer.Exit(1)

    # Sort within groups
    if sort_by == "size":
        df = df.sort_values(["model_size_gb"])
    elif sort_by == "quantization":
        df = df.sort_values(["quantization"])
    else:
        df = df.sort_values(["model_name"])

    # Group data
    if group_by == "family":
        df = df.sort_values(["family", "model_size_gb"])
        groups = df.groupby("family", sort=False)
    elif group_by == "quantization":
        df = df.sort_values(["quantization", "family"])
        groups = df.groupby("quantization", sort=False)
    else:
        groups = [("all", df)]

    # Create labels
    if group_by == "family":
        df["label"] = df["quantization"] + "\n" + df["model_size_gb"].apply(lambda x: f"{x:.1f}GB")
    elif group_by == "quantization":
        df["label"] = df["family"] + "\n" + df["model_size_gb"].apply(lambda x: f"{x:.1f}GB")
    else:
        df["label"] = df["family"] + "-" + df["quantization"] + "\n" + df["model_size_gb"].apply(lambda x: f"{x:.1f}GB")

    # Parse figure size
    try:
        w, h = map(float, figsize.split("x"))
    except ValueError:
        w, h = 16, 7

    # Create plot
    fig, ax1 = plt.subplots(figsize=(w, h))

    x_positions = []
    x_labels = []
    group_boundaries = []
    group_centers = []
    group_names = []

    current_x = 0
    group_spacing = 1.5

    for group_name, group_df in groups:
        group_x = []

        for idx, row in group_df.iterrows():
            x_positions.append(current_x)
            x_labels.append(row["label"])
            group_x.append(current_x)
            current_x += 1

        if group_x:
            group_centers.append(np.mean(group_x))
            group_names.append(group_name)
            group_boundaries.append(current_x - 0.5)

        current_x += group_spacing

    x_positions = np.array(x_positions)

    # Plot throughput bars
    if show_throughput and "tokens_per_second" in df.columns:
        ax2 = ax1.twinx()
        throughput = df["tokens_per_second"].fillna(0).values

        bar_colors = []
        for _, row in df.iterrows():
            color = FAMILY_COLORS.get(row["base_family"], "#CCCCCC")
            bar_colors.append(color)

        ax2.bar(x_positions, throughput, alpha=0.25, color=bar_colors, zorder=1)
        ax2.set_ylabel("Throughput (tokens/sec)", color="gray")
        ax2.tick_params(axis="y", labelcolor="gray")
        max_tp = max(throughput) if len(throughput) > 0 else 300
        ax2.set_ylim(0, max_tp * 1.2)

    # Plot benchmark lines
    for col, config in plot_benchmarks.items():
        if col not in df.columns:
            continue

        values = df[col].fillna(0).values

        ax1.plot(
            x_positions,
            values,
            marker="o",
            label=config["label"],
            color=config["color"],
            linewidth=2,
            markersize=5,
            zorder=3,
        )

    # Draw group separators
    for boundary in group_boundaries[:-1]:
        ax1.axvline(x=boundary + group_spacing/2, color="gray", linestyle="--", alpha=0.3, zorder=0)

    # Add group labels at top
    for center, name in zip(group_centers, group_names):
        ax1.annotate(
            name,
            xy=(center, 1.02),
            xycoords=("data", "axes fraction"),
            ha="center",
            va="bottom",
            fontsize=10,
            fontweight="bold",
            color=FAMILY_COLORS.get(name.split("-")[0], "#333333"),
        )

    # Configure axes
    ax1.set_xlabel("Model (Quantization / Size)")
    ax1.set_ylabel("Score")
    ax1.set_ylim(0, 1.0)
    ax1.set_xticks(x_positions)
    ax1.set_xticklabels(x_labels, rotation=45, ha="right", fontsize=8)
    ax1.legend(loc="lower right", fontsize=8)
    ax1.grid(True, alpha=0.3, axis="y", zorder=0)

    # Set title
    if title:
        plt.title(title, pad=20)
    else:
        families = df["family"].unique()
        plt.title(f"Model Comparison: {', '.join(families)}", pad=20)

    plt.tight_layout()

    if output:
        plt.savefig(output, dpi=150, bbox_inches="tight")
        typer.echo(f"Saved plot to {output}")
    else:
        plt.show()


@app.command()
def families(
    models: Annotated[
        list[Path],
        typer.Argument(help="Path(s) to CSV result files"),
    ],
):
    """List detected model families in the CSV files."""
    csv_files = []
    for path in models:
        if "*" in str(path):
            csv_files.extend(Path(".").glob(str(path)))
        else:
            csv_files.append(path)

    df = load_results(csv_files)

    typer.echo("\nDetected model families:\n")

    for model_name in df["model_name"].unique():
        family, base = detect_family(model_name)
        quant = extract_quantization(model_name)
        typer.echo(f"  {model_name}")
        typer.echo(f"    -> family: {family}, quant: {quant}")
    typer.echo()


@app.command()
def bar(
    models: Annotated[
        list[Path],
        typer.Argument(help="Path(s) to CSV result files"),
    ],
    benchmark: Annotated[
        str,
        typer.Option("-b", "--benchmark", help="Single benchmark to compare"),
    ],
    title: Annotated[
        Optional[str], typer.Option("-t", "--title", help="Plot title")
    ] = None,
    output: Annotated[
        Optional[Path],
        typer.Option("-o", "--output", help="Output file"),
    ] = None,
    figsize: Annotated[
        str, typer.Option("--figsize", help="Figure size as WxH")
    ] = "12x6",
):
    """Create grouped bar chart for a single benchmark across families (uses latest row from each file)."""
    csv_files = []
    for path in models:
        if "*" in str(path):
            csv_files.extend(Path(".").glob(str(path)))
        else:
            csv_files.append(path)

    if not csv_files:
        raise typer.BadParameter("No CSV files specified")

    df = load_results(csv_files)
    df = add_family_info(df)

    if benchmark not in df.columns:
        typer.echo(f"Error: Benchmark '{benchmark}' not found in data")
        typer.echo(f"Available: {[c for c in df.columns if '_acc' in c or '_match' in c or 'pass_at' in c]}")
        raise typer.Exit(1)

    # Get unique families and quantizations
    families = sorted(df["family"].unique())
    quants = sorted(df["quantization"].unique())

    try:
        w, h = map(float, figsize.split("x"))
    except ValueError:
        w, h = 12, 6

    fig, ax = plt.subplots(figsize=(w, h))

    x = np.arange(len(quants))
    width = 0.8 / len(families)

    for i, family in enumerate(families):
        family_data = df[df["family"] == family]

        values = []
        for quant in quants:
            row = family_data[family_data["quantization"] == quant]
            if len(row) > 0:
                values.append(row[benchmark].iloc[0])
            else:
                values.append(0)

        offset = (i - len(families)/2 + 0.5) * width
        base_family = family.split("-")[0]
        color = FAMILY_COLORS.get(base_family, f"C{i}")

        ax.bar(x + offset, values, width, label=family, color=color)

    ax.set_xlabel("Quantization")
    ax.set_ylabel("Score")
    ax.set_xticks(x)
    ax.set_xticklabels(quants, rotation=45, ha="right")
    ax.legend()
    ax.set_ylim(0, 1.0)
    ax.grid(True, alpha=0.3, axis="y")

    bench_label = BENCHMARK_CONFIGS.get(benchmark, {}).get("label", benchmark)
    plt.title(title or f"{bench_label} by Model Family and Quantization")
    plt.tight_layout()

    if output:
        plt.savefig(output, dpi=150, bbox_inches="tight")
        typer.echo(f"Saved plot to {output}")
    else:
        plt.show()


if __name__ == "__main__":
    app()
