#!/usr/bin/env python3
"""Copy files listed in a NobodyWho runtime manifest."""

import argparse
import json
import shutil
from pathlib import Path

KINDS = ("libraries", "backends")


def fail(message):
    raise SystemExit(f"runtime-bundle: {message}")


def load(path):
    try:
        manifest = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        fail(f"cannot read {path}: {error}")
    if not isinstance(manifest, dict) or set(manifest) != set(KINDS):
        fail(f"invalid manifest in {path}")

    all_names = []
    for kind in KINDS:
        names = manifest[kind]
        if not isinstance(names, list) or not names:
            fail(f"invalid {kind} in {path}")
        if any(
            not isinstance(name, str)
            or not name
            or any(character.isspace() for character in name)
            or Path(name).name != name
            or "\\" in name
            for name in names
        ):
            fail(f"invalid filename in {path}")
        if names != sorted(set(names)):
            fail(f"{kind} must be sorted and unique in {path}")
        all_names += names

    if len(all_names) != len(set(all_names)):
        fail(f"duplicate filename in {path}")
    if any(not (path.parent / name).is_file() for name in all_names):
        fail(f"manifest lists a missing file in {path}")
    return manifest


def selected(manifest, kind):
    return manifest[kind] if kind else [name for key in KINDS for name in manifest[key]]


def copy_files(args):
    path = args.manifest.resolve()
    names = selected(load(path), args.kind)
    unknown = set(args.exclude) - set(names)
    if unknown:
        fail(f"cannot exclude unknown files: {', '.join(sorted(unknown))}")

    destination = args.destination.resolve()
    destination.mkdir(parents=True, exist_ok=True)
    for name in names:
        if name not in args.exclude:
            shutil.copy2(path.parent / name, destination / name)


def list_names(args):
    print("\n".join(selected(load(args.manifest.resolve()), args.kind)))


def merge(args):
    manifests = [load(path.resolve()) for path in args.manifests]
    merged = {
        kind: sorted({name for manifest in manifests for name in manifest[kind]})
        for kind in KINDS
    }
    output = args.output.resolve()
    output.write_text(json.dumps(merged, indent=2, sort_keys=True), encoding="utf-8")
    load(output)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)

    copy = commands.add_parser("copy")
    copy.add_argument("manifest", type=Path)
    copy.add_argument("destination", type=Path)
    copy.add_argument("--kind", choices=KINDS)
    copy.add_argument("--exclude", action="append", default=[])
    copy.set_defaults(run=copy_files)

    names = commands.add_parser("names")
    names.add_argument("manifest", type=Path)
    names.add_argument("--kind", choices=KINDS)
    names.set_defaults(run=list_names)

    merge_command = commands.add_parser("merge")
    merge_command.add_argument("output", type=Path)
    merge_command.add_argument("manifests", type=Path, nargs="+")
    merge_command.set_defaults(run=merge)

    args = parser.parse_args()
    args.run(args)


if __name__ == "__main__":
    main()
