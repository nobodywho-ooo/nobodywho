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
    from eval import get_all_metric_columns
    new_metric_cols = [c for c in get_all_metric_columns() if c not in existing_cols]

    if not new_metric_cols:
        print(f"  OK (up to date): {path.name}")
        return False

    print(f"  MIGRATE: {path.name} — adding {new_metric_cols}")

    if dry_run:
        return True

    new_fieldnames = existing_cols + new_metric_cols

    tmp_path = path.with_suffix(".csv.tmp")
    with open(tmp_path, "w", newline="") as f:
        writer = csv.DictWriter(f, fieldnames=new_fieldnames, extrasaction="ignore")
        writer.writeheader()
        for row in rows:
            for col in new_metric_cols:
                row.setdefault(col, "")
            writer.writerow(row)

    tmp_path.replace(path)
    return True


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
        if migrate_csv(path, dry_run=dry_run):
            changed += 1

    print(f"\nDone. {changed}/{len(csv_files)} files {'would be ' if dry_run else ''}updated.")


if __name__ == "__main__":
    main()
