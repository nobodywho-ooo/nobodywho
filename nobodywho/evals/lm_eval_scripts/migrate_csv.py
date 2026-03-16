#!/usr/bin/env python3
"""Add missing CSV columns to existing result files.

Run this after adding new benchmark tasks to the eval suite to backfill
the new columns (with empty values) into existing CSV files.

Usage:
    python migrate_csv.py ../eval_results/*.csv
    python migrate_csv.py ../eval_results/  # directory: processes all CSVs in it
"""

import csv
import sys
from pathlib import Path

# Import the canonical metric column list from eval.py
sys.path.insert(0, str(Path(__file__).parent))


def migrate_csv(path: Path, dry_run: bool = False) -> bool:
    """Add any missing columns to a CSV file. Returns True if changes were made."""
    with open(path, newline="") as f:
        reader = csv.DictReader(f)
        if reader.fieldnames is None:
            print(f"  SKIP (empty): {path.name}")
            return False
        existing_cols = list(reader.fieldnames)
        rows = list(reader)

    # Import here so the venv check happens at runtime
    from eval import get_all_metric_columns, get_csv_fieldnames

    # Canonical base columns (kept in sync with get_csv_fieldnames in eval.py)
    CANONICAL_BASE = [
        "timestamp", "model_path", "model_name", "model_size_gb",
        "limit", "seed", "duration_seconds",
        "total_samples", "failed_samples", "failure_rate",
        "total_tokens_generated", "generation_time_seconds", "tokens_per_second",
        "sampler_config", "allow_thinking",
    ]

    new_base_cols = [c for c in CANONICAL_BASE if c not in existing_cols]
    new_metric_cols = [c for c in get_all_metric_columns() if c not in existing_cols]
    all_new_cols = new_base_cols + new_metric_cols

    if not all_new_cols:
        print(f"  OK (up to date): {path.name}")
        return False

    print(f"  MIGRATE: {path.name} — adding {all_new_cols}")

    if dry_run:
        return True

    new_fieldnames = list(existing_cols)

    # Insert missing base columns after their predecessor in the canonical order
    for col in new_base_cols:
        canon_idx = CANONICAL_BASE.index(col)
        insert_after = None
        for prev in reversed(CANONICAL_BASE[:canon_idx]):
            if prev in new_fieldnames:
                insert_after = new_fieldnames.index(prev) + 1
                break
        if insert_after is not None:
            new_fieldnames.insert(insert_after, col)
        else:
            new_fieldnames.insert(0, col)

    # Insert missing metric columns before system_info columns
    if new_metric_cols:
        all_metrics = set(get_all_metric_columns())
        base_set = set(CANONICAL_BASE)
        # Find the first system_info key (first column after base that isn't a metric)
        system_info_start = None
        for i, c in enumerate(new_fieldnames):
            if c not in base_set and c not in all_metrics:
                system_info_start = i
                break

        if system_info_start is not None:
            for j, col in enumerate(new_metric_cols):
                new_fieldnames.insert(system_info_start + j, col)
        else:
            new_fieldnames.extend(new_metric_cols)

    tmp_path = path.with_suffix(".csv.tmp")
    with open(tmp_path, "w", newline="") as f:
        writer = csv.DictWriter(f, fieldnames=new_fieldnames, extrasaction="ignore")
        writer.writeheader()
        for row in rows:
            for col in all_new_cols:
                row.setdefault(col, "False" if col == "allow_thinking" else "")
            writer.writerow(row)

    tmp_path.replace(path)
    return True


def repair_shifted_rows(path: Path, dry_run: bool = False) -> bool:
    """Fix rows where columns were shifted due to header/fieldnames mismatch.

    Detects the problem by checking if a known system_info column (like
    'nobodywho_commit') contains a value that looks wrong (e.g. 'Linux'
    instead of a commit hash), then realigns by matching values to the
    correct columns.

    Returns True if any rows were repaired.
    """
    with open(path, newline="") as f:
        reader = csv.DictReader(f)
        if reader.fieldnames is None:
            return False
        fieldnames = list(reader.fieldnames)
        rows = list(reader)

    if not rows:
        return False

    # Known system_info columns and their expected positions in the canonical order
    SYSTEM_INFO_COLS = [
        "cpu_model", "cpu_count", "memory_total_gb", "gpu_device",
        "os", "nobodywho_version", "nobodywho_commit", "nobodywho_dirty",
    ]

    # Quick detection: check if any row has 'nobodywho_commit' that looks like
    # an OS name instead of a hex hash (a clear sign of column shift)
    repaired = False
    for row_idx, row in enumerate(rows):
        commit_val = row.get("nobodywho_commit", "")
        os_val = row.get("os", "")

        # Detect shift: commit should be hex hash or empty, not "Linux"/"Windows"
        is_shifted = (
            commit_val in ("Linux", "Windows", "Darwin")
            or (os_val and os_val.replace(".", "").isdigit())  # os has a version number
        )
        if not is_shifted:
            continue

        # Find the shift amount by locating the actual OS value
        # Walk the system_info columns to find where the values actually start
        sys_info_indices = [fieldnames.index(c) for c in SYSTEM_INFO_COLS if c in fieldnames]
        if not sys_info_indices:
            continue

        # Read raw values at the system_info column positions
        first_sys_idx = sys_info_indices[0]  # cpu_model position in header

        # The actual system_info values start somewhere after first_sys_idx.
        # Find the offset by looking for the cpu_model value pattern
        # (it's typically a non-numeric string like "QEMU Virtual CPU...")
        raw_values = [row.get(fieldnames[i], "") for i in range(first_sys_idx, len(fieldnames))]

        # Find where the actual cpu_model value starts in raw_values
        # cpu_model is always a text string, cpu_count is always a small integer
        shift = 0
        for offset in range(len(raw_values) - 1):
            val = raw_values[offset]
            next_val = raw_values[offset + 1] if offset + 1 < len(raw_values) else ""
            # cpu_model is text, cpu_count is a small integer
            if (val and not val.replace(".", "").replace("-", "").isdigit()
                    and val not in ("True", "False", "")
                    and next_val.isdigit()
                    and offset > 0):
                shift = offset
                break

        if shift == 0:
            continue

        print(f"  REPAIR row {row_idx + 1}: shifting system_info left by {shift} in {path.name}")

        if dry_run:
            repaired = True
            continue

        # Rebuild the row with correct alignment
        # System info values are at positions [first_sys_idx + shift : first_sys_idx + shift + len(SYSTEM_INFO_COLS)]
        # Metric columns that were displaced need to be cleared
        for i, col in enumerate(SYSTEM_INFO_COLS):
            src_idx = first_sys_idx + shift + i
            if col in fieldnames and src_idx < len(fieldnames):
                row[col] = row.get(fieldnames[src_idx], "")

        # Clear the columns that were incorrectly holding shifted values
        # (metric columns between the original and shifted system_info start)
        for i in range(first_sys_idx, first_sys_idx + shift):
            if i < len(fieldnames):
                col = fieldnames[i]
                if col not in SYSTEM_INFO_COLS:
                    row[col] = ""

        # Clear trailing columns that had system_info overflow
        for i in range(first_sys_idx + len(SYSTEM_INFO_COLS), len(fieldnames)):
            col = fieldnames[i]
            if col not in SYSTEM_INFO_COLS:
                # Only clear if it looks like it has a system_info value
                val = row.get(col, "")
                if val in ("True", "False") or (len(val) == 40 and all(c in "0123456789abcdef" for c in val)):
                    row[col] = ""

        repaired = True

    if repaired and not dry_run:
        tmp_path = path.with_suffix(".csv.tmp")
        with open(tmp_path, "w", newline="") as f:
            writer = csv.DictWriter(f, fieldnames=fieldnames, extrasaction="ignore")
            writer.writeheader()
            writer.writerows(rows)
        tmp_path.replace(path)

    return repaired


def main():
    args = sys.argv[1:]
    if not args:
        print(f"Usage: {sys.argv[0]} [--dry-run] <csv-file-or-dir> ...")
        sys.exit(1)

    dry_run = "--dry-run" in args
    paths = [Path(a) for a in args if not a.startswith("--")]

    csv_files: list[Path] = []
    for p in paths:
        if p.is_dir():
            csv_files.extend(sorted(p.glob("*.csv")))
        elif p.suffix == ".csv":
            csv_files.append(p)
        else:
            print(f"Skipping non-CSV: {p}")

    if not csv_files:
        print("No CSV files found.")
        sys.exit(1)

    if dry_run:
        print("DRY RUN — no files will be modified\n")

    changed = 0
    for path in csv_files:
        repaired = repair_shifted_rows(path, dry_run=dry_run)
        migrated = migrate_csv(path, dry_run=dry_run)
        if repaired or migrated:
            changed += 1

    print(f"\nDone. {changed}/{len(csv_files)} files {'would be ' if dry_run else ''}updated.")


if __name__ == "__main__":
    main()
