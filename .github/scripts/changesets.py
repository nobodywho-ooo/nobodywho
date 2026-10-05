#!/usr/bin/env -S uv run --script
#
# /// script
# requires-python = ">=3.11"
# ///
"""Manage .changeset/*.md and turn them into version bumps and CHANGELOG.md release entries.

Usage:
  changesets.py new                        interactively write a change file
  changesets.py check [--pr-base REV [--pr-labels L,...]]
                                           validate change files; with --pr-base, also check that the PR
                                           adds one and leaves the changelogs alone, unless its labels say otherwise
  changesets.py preview                    print the CHANGELOG.md entry the next release would get
  changesets.py release                    bump versions, write the changelogs and delete the change files
  changesets.py tag [--create]             list (or create on HEAD) the newest release's missing tags
  changesets.py push-tags                  tag HEAD and push those tags one at a time, waiting for each CI run
"""

import argparse
import datetime
import enum
import json
import pathlib
import re
import subprocess
import sys
import time
from dataclasses import dataclass

ROOT = pathlib.Path(__file__).resolve().parents[2]
CHANGESET_DIR = ROOT / ".changeset"
CHANGELOG = ROOT / "CHANGELOG.md"
FLUTTER_CHANGELOG = ROOT / "nobodywho/flutter/nobodywho/CHANGELOG.md"
# PR labels that waive the check's requirement of a new change file and ban on changelog edits.
NO_CHANGELOG_LABEL = "no-changelog"
EDIT_CHANGELOG_LABEL = "edit-changelog"
# Gitignored drafts for the GitHub release pages.
NOTES_DIR = ROOT / "nobodywho/changelogs"

# In the order CHANGELOG.md headings list them.
BINDINGS = {
    "python": "Python",
    "flutter": "Flutter",
    "godot": "Godot",
    "kotlin": "Kotlin",
    "react-native": "React Native",
    "swift": "Swift",
    "csharp": "C#",
}


class Bump(enum.IntEnum):
    """A semver bump, ordered so that `max()` picks the biggest."""

    PATCH = 1
    MINOR = 2
    MAJOR = 3


# How change files spell each bump.
BUMPS = {bump.name.lower(): bump for bump in Bump}
# In the order CHANGELOG.md lists them.
SECTIONS = ["added", "changed", "deprecated", "fixed", "removed", "security"]
# The bump `just change` suggests for each section. None for `changed`, where breaking changes hide.
SUGGESTED_BUMPS = {
    "added": "minor",
    "deprecated": "minor",
    "removed": "major",
    "fixed": "patch",
    "security": "patch",
}


# Regex flags used below: (?m) makes ^ match at the start of every line, not just the
# file's, and (?s) lets . match newlines too, so .*? can skip over several lines.


def cargo_toml(path: str) -> tuple[str, str]:
    # The first `version = "..."` line after `[package]`, skipping the lines in between
    # and never reaching dependency versions, which are written inline after the name.
    return path, r'(?ms)^\[package\].*?^version = "(?P<version>[^"]+)"'


def cargo_lock(crate: str) -> tuple[str, str]:
    # The lock entry for one crate, where `version` is the line right after `name`.
    return (
        "nobodywho/Cargo.lock",
        rf'(?m)^name = "{crate}"\nversion = "(?P<version>[^"]+)"',
    )


# The package's own `"version"`, indented two spaces at the top of the JSON object; nested
# dependency versions are indented deeper, so they don't match.
NPM_TOP_LEVEL = r'(?m)^  "version": "(?P<version>[^"]+)"'
# The SwiftPM install line, `.package(url: ".../nobodywho-swift.git", from: "X.Y.Z")`.
SWIFT_SNIPPET = r'nobodywho-swift\.git", from: "(?P<version>[^"]+)"'

# Every place a binding's version is written, as (path, regex with a `version` group).
# All matches must hold the same version, and a release replaces every one.
VERSION_FILES = {
    "python": [
        cargo_toml("nobodywho/python/Cargo.toml"),
        # The first `version = "..."` line after `[project]`, like `cargo_toml`.
        (
            "nobodywho/python/pyproject.toml",
            r'(?ms)^\[project\].*?^version = "(?P<version>[^"]+)"',
        ),
        cargo_lock("nobodywho-python"),
        # The lock entry for the package itself, which uv marks `source = { editable = "." }`.
        (
            "nobodywho/python/uv.lock",
            r'(?m)^name = "nobodywho"\nversion = "(?P<version>[^"]+)"\nsource = \{ editable',
        ),
    ],
    "godot": [
        cargo_toml("nobodywho/godot/Cargo.toml"),
        cargo_lock("nobodywho-godot"),
    ],
    "flutter": [
        cargo_toml("nobodywho/flutter/rust/Cargo.toml"),
        # The top-level `version:` key; nested keys are indented, so they don't match.
        ("nobodywho/flutter/nobodywho/pubspec.yaml", r"(?m)^version: (?P<version>\S+)"),
        cargo_lock("nobodywho-flutter"),
    ],
    "kotlin": [
        # `version = "X.Y.Z"` in `allprojects {}`, the only such assignment in the file.
        (
            "nobodywho/kotlin/build.gradle.kts",
            r'version = "(?P<version>\d+\.\d+\.\d+)"',
        ),
        # The Gradle coordinates in both install snippets, `ai.nobodywho:nobodywho:X.Y.Z`
        # and `ai.nobodywho:nobodywho-android:X.Y.Z`.
        (
            "docs/docs-kotlin/index.md",
            r"ai\.nobodywho:nobodywho(-android)?:(?P<version>\d+\.\d+\.\d+)",
        ),
    ],
    "react-native": [
        ("nobodywho/react-native/package.json", NPM_TOP_LEVEL),
        ("nobodywho/react-native/package-lock.json", NPM_TOP_LEVEL),
        # The lockfile repeats the version under `"packages": { "": { ... } }`, the entry
        # for the package itself.
        (
            "nobodywho/react-native/package-lock.json",
            r'(?m)^    "": \{\n      "name": "[^"]*",\n      "version": "(?P<version>[^"]+)"',
        ),
    ],
    # SwiftPM reads the version from the tag, so only the install snippets carry it.
    "swift": [
        ("nobodywho/swift/README.md", SWIFT_SNIPPET),
        ("docs/docs-swift/index.md", SWIFT_SNIPPET),
    ],
    "csharp": [
        (
            "nobodywho/csharp/src/NobodyWho/NobodyWho.csproj",
            r"<Version>(?P<version>[^<]+)</Version>",
        ),
    ],
}


@dataclass
class Change:
    path: pathlib.Path
    section: str
    bumps: dict[str, Bump]
    body: str


def parse_change_file(path: pathlib.Path) -> tuple[Change | None, list[str]]:
    errors = []
    # A leading `---` line, the frontmatter up to the next `---` line, then the body.
    match = re.match(
        r"---\n(.*?)^---\n(.*)", path.read_text(), re.DOTALL | re.MULTILINE
    )
    if not match:
        return None, ["must start with a `---` frontmatter block"]
    frontmatter, body = match.groups()

    section = None
    bumps = {}
    keys, parent = set(), None
    for line in frontmatter.splitlines():
        # `key: value`, indented when it's one of the `<binding>: <bump>` lines under `bindings:`.
        entry = re.fullmatch(r"( *)([\w-]+): *(\S*)", line)
        if not entry:
            errors.append(
                f"unreadable frontmatter line {line!r}, expected `key: value`"
            )
            continue
        indent, key, value = entry.groups()

        if indent and parent == "bindings":
            if key not in BINDINGS:
                errors.append(
                    f"unknown binding {key!r}, expected one of {list(BINDINGS)}"
                )
            if value not in BUMPS:
                errors.append(
                    f"{key}: bump must be one of {list(BUMPS)}, got {value!r}"
                )
            if key in bumps:
                errors.append(f"{key} listed twice")
            if key in BINDINGS and value in BUMPS:
                bumps[key] = BUMPS[value]
            continue
        if indent:
            errors.append(f"unexpected indented line {line!r}")
            continue

        parent = key
        if key in keys:
            errors.append(f"{key} listed twice")
        keys.add(key)
        if key == "section":
            if value not in SECTIONS:
                errors.append(f"section must be one of {SECTIONS}, got {value!r}")
            section = value
        elif key == "bindings":
            if value:
                errors.append(
                    "put each `<binding>: <bump>` on its own indented line under `bindings:`"
                )
        else:
            errors.append(f"unknown key {key!r}, expected `section` or `bindings`")
    if section is None:
        errors.append(f"frontmatter needs a `section:` line, one of {SECTIONS}")
    if not bumps:
        errors.append(
            "frontmatter needs a `bindings:` block naming at least one binding"
        )
    body = body.strip()
    if not body:
        errors.append("has no description")

    if errors or section is None:
        return None, errors
    return Change(path, section, bumps, body), []


def load_changes() -> list[Change]:
    """Parse every change file, oldest first, exiting if any is invalid."""
    changes, failed = [], False
    for path in sorted(CHANGESET_DIR.glob("*")):
        rel = path.relative_to(ROOT)
        if path.suffix != ".md":
            print(f"::error file={rel}::only .md change files belong in .changeset/")
            failed = True
            continue
        change, errors = parse_change_file(path)
        for error in errors:
            print(f"::error file={rel}::{error}")
        failed |= bool(errors)
        if change:
            changes.append(change)
    if failed:
        sys.exit(1)

    added = added_times()
    return sorted(changes, key=lambda c: added.get(c.path.name, float("inf")))


def added_times() -> dict[str, int]:
    """When each committed change file was added, so entries keep their chronological order."""
    out = git(
        "log", "--diff-filter=A", "--name-only", "--format=%ct", "--", ".changeset/"
    )
    times, current = {}, 0
    for line in out.splitlines():
        if line.isdigit():
            current = int(line)
        elif line:
            # git log is newest first, so the last hit is the oldest add.
            times[pathlib.Path(line).name] = current
    return times


def git(*args: str) -> str:
    return subprocess.run(
        ["git", *args], cwd=ROOT, check=True, capture_output=True, text=True
    ).stdout


def current_version(package: str) -> str:
    """Read a package's version, exiting unless every version file agrees on it."""
    found = []
    for path, pattern in VERSION_FILES[package]:
        matches = list(re.finditer(pattern, (ROOT / path).read_text()))
        if not matches:
            sys.exit(f"::error file={path}::no {package} version matching {pattern!r}")
        found += [(path, m["version"]) for m in matches]
    if len({version for _, version in found}) > 1:
        listing = ", ".join(f"{version} in {path}" for path, version in found)
        sys.exit(f"::error::{package} version files disagree: {listing}")
    return found[0][1]


def update_version_files(package: str, version: str) -> None:
    for path, pattern in VERSION_FILES[package]:
        file = ROOT / path
        text = file.read_text()
        # Replace from the end so earlier spans stay valid.
        for match in reversed(list(re.finditer(pattern, text))):
            start, end = match.span("version")
            text = text[:start] + version + text[end:]
        file.write_text(text)


def bump_version(version: str, kind: Bump) -> str:
    major, minor, patch = map(int, version.split("."))
    match kind:
        case Bump.MAJOR:
            return f"{major + 1}.0.0"
        case Bump.MINOR:
            return f"{major}.{minor + 1}.0"
        case Bump.PATCH:
            return f"{major}.{minor}.{patch + 1}"


def next_versions(changes: list[Change]) -> dict[str, str]:
    kinds = {}
    for change in changes:
        for package, kind in change.bumps.items():
            kinds[package] = max(kinds.get(package, kind), kind)
    return {
        p: bump_version(current_version(p), kinds[p]) for p in BINDINGS if p in kinds
    }


def join(names: list[str]) -> str:
    return names[0] if len(names) == 1 else ", ".join(names[:-1]) + " and " + names[-1]


def bullet(text: str) -> str:
    """Make `text` a Markdown list item, indenting its continuation lines."""
    lines = text.splitlines()
    return "\n".join(
        ["- " + lines[0]] + [f"  {line}" if line else "" for line in lines[1:]]
    )


def central_entry(change: Change) -> str:
    """Render a change for CHANGELOG.md, which has to say which bindings it concerns."""
    packages = [p for p in BINDINGS if p in change.bumps]
    names = [BINDINGS[p] for p in packages]
    breaking = [BINDINGS[p] for p in packages if change.bumps[p] == Bump.MAJOR]

    prefix = ""
    if breaking == names:
        prefix += "**Breaking:** "
    elif breaking:
        prefix += f"**Breaking for {join(breaking)}:** "
    if len(packages) == 1:
        return bullet(f"{prefix}**{names[0]}:** {change.body}")

    verb = "Available for" if change.section == "added" else "Affects"
    scope = "all bindings" if len(packages) == len(BINDINGS) else join(names)
    # A body with several paragraphs gets the scope as a paragraph of its own.
    separator = "\n\n" if "\n\n" in change.body else " "
    return bullet(f"{prefix}{change.body}{separator}{verb} {scope}.")


def binding_entry(change: Change, package: str) -> str:
    prefix = "**Breaking:** " if change.bumps[package] == Bump.MAJOR else ""
    return bullet(prefix + change.body)


def sections(entries: list[tuple[str, str]]) -> str:
    """Group (section, rendered entry) pairs under `###` headings."""
    out = []
    for section in SECTIONS:
        items = [entry for s, entry in entries if s == section]
        if items:
            out.append(f"### {section.capitalize()}\n\n" + "\n".join(items))
    return "\n\n".join(out)


def central_release(changes: list[Change], versions: dict[str, str]) -> str:
    packages = ", ".join(f"{BINDINGS[p]} v{v}" for p, v in versions.items())
    today = datetime.datetime.now(datetime.UTC).date().isoformat()
    body = sections([(c.section, central_entry(c)) for c in changes])
    return f"## [{packages}] - {today}\n\n{body}\n"


def binding_release(changes: list[Change], package: str, version: str) -> str:
    body = sections(
        [(c.section, binding_entry(c, package)) for c in changes if package in c.bumps]
    )
    return f"## {version}\n\n{body}\n"


def prepend_release(path: pathlib.Path, release: str) -> None:
    """Insert a release above the newest `## ` heading, below any preamble."""
    text = path.read_text()
    # The first line that starts with `## `, which is the newest release heading.
    match = re.search(r"^## ", text, re.MULTILINE)
    at = match.start() if match else len(text)
    path.write_text(text[:at] + release + "\n" + text[at:])


def newest_release_tags() -> list[str]:
    """The `nobodywho-<binding>-v<version>` tags for the newest CHANGELOG.md release."""
    # The bracketed list in the first `## [...]` heading, e.g. `Python v3.1.0, React Native v4.0.1`.
    heading = re.search(r"^## \[([^\]]*)\]", CHANGELOG.read_text(), re.MULTILINE)
    if not heading:
        sys.exit("::error file=CHANGELOG.md::no `## [...]` release heading")
    bindings = {name: binding for binding, name in BINDINGS.items()}
    tags = []
    for release in heading[1].split(", "):
        # A binding's display name and its version, e.g. `React Native v4.0.1`.
        parsed = re.fullmatch(r"(.+) v(\d+\.\d+\.\d+)", release)
        if not parsed or parsed[1] not in bindings:
            sys.exit(
                f"::error file=CHANGELOG.md::the newest `## [...]` heading lists {release!r}, "
                "not `<Binding> v<X.Y.Z>`"
            )
        tags.append(f"nobodywho-{bindings[parsed[1]]}-v{parsed[2]}")
    return tags


def ask(question: str, valid=lambda answer: True, default: str = "") -> str:
    """Prompt until the answer is valid; an empty answer takes `default`, if there is one."""
    prompt = f"{question} [{default}]: " if default else f"{question}: "
    while True:
        try:
            answer = input(prompt).strip() or default
        except (EOFError, KeyboardInterrupt):
            print()
            sys.exit(1)
        if answer and valid(answer):
            return answer


def cmd_new(args: argparse.Namespace) -> int:
    print(f"Bindings: {', '.join(BINDINGS)}")
    answer = ask(
        "Affected bindings (comma-separated, or `all`)",
        lambda a: a == "all" or all(b.strip() in BINDINGS for b in a.split(",")),
    )
    packages = (
        list(BINDINGS) if answer == "all" else [b.strip() for b in answer.split(",")]
    )
    section = ask(f"Changelog section ({', '.join(SECTIONS)})", lambda a: a in SECTIONS)
    kind = ask(
        f"Bump ({', '.join(reversed(BUMPS))}; edit the file to differ per binding)",
        lambda a: a in BUMPS,
        default=SUGGESTED_BUMPS.get(section, ""),
    )
    description = ask("Description for users (you can add detail in the file later)")

    # Suggest the description's first eight words, without punctuation, as the file name.
    suggestion = "_".join(re.findall(r"[a-z0-9]+", description.lower())[:8])
    name = ask(
        "File name, without .md (lowercase letters, digits, _ and -)",
        # Lowercase words joined by `_` or `-`, naming a file that doesn't exist yet.
        lambda a: (
            bool(re.fullmatch(r"[a-z0-9_-]+", a))
            and not (CHANGESET_DIR / f"{a}.md").exists()
        ),
        default=suggestion,
    )
    path = CHANGESET_DIR / f"{name}.md"
    bindings = "".join(f"  {p}: {kind}\n" for p in packages)
    CHANGESET_DIR.mkdir(exist_ok=True)
    path.write_text(
        f"---\nsection: {section}\nbindings:\n{bindings}---\n\n{description}\n"
    )
    print(f"Wrote {path.relative_to(ROOT)}")
    return cmd_check(argparse.Namespace(pr_base=None, pr_labels=""))


def cmd_check(args: argparse.Namespace) -> int:
    load_changes()
    if not args.pr_base:
        return 0
    labels = set(args.pr_labels.split(","))
    failed = False

    added = git(
        "diff",
        "--name-only",
        "--diff-filter=A",
        f"{args.pr_base}...HEAD",
        "--",
        ".changeset/",
    )
    if NO_CHANGELOG_LABEL not in labels and not any(
        p.endswith(".md") for p in added.splitlines()
    ):
        print(
            "::error::No change file added. Run `just change` to describe this change for users, "
            f"or add the `{NO_CHANGELOG_LABEL}` label if it has no user-facing effect."
        )
        failed = True

    changelogs = [
        str(path.relative_to(ROOT)) for path in (CHANGELOG, FLUTTER_CHANGELOG)
    ]
    edited = git("diff", "--name-only", f"{args.pr_base}...HEAD", "--", *changelogs)
    if EDIT_CHANGELOG_LABEL not in labels:
        for path in edited.splitlines():
            print(
                f"::error file={path}::Edited directly, but releases write it from the change files. "
                "Run `just change` to describe this change for users instead, or add the "
                f"`{EDIT_CHANGELOG_LABEL}` label if the edit is deliberate (a release, or a fix to a past entry)."
            )
            failed = True
    return int(failed)


def cmd_preview(args: argparse.Namespace) -> int:
    changes = load_changes()
    if not changes:
        print("No change files in .changeset/, nothing to release.")
        return 0
    print(central_release(changes, next_versions(changes)), end="")
    return 0


def cmd_release(args: argparse.Namespace) -> int:
    changes = load_changes()
    if not changes:
        print("No change files in .changeset/, nothing to release.")
        return 1
    versions = next_versions(changes)

    for package, version in versions.items():
        update_version_files(package, version)
    prepend_release(CHANGELOG, central_release(changes, versions))
    if "flutter" in versions:
        prepend_release(
            FLUTTER_CHANGELOG, binding_release(changes, "flutter", versions["flutter"])
        )
    NOTES_DIR.mkdir(exist_ok=True)
    for package, version in versions.items():
        notes = NOTES_DIR / f"{package}-{version}.md"
        notes.write_text(binding_release(changes, package, version))
        print(f"Release notes for GitHub: {notes.relative_to(ROOT)}")
    for change in changes:
        change.path.unlink()
    return 0


def cmd_tag(args: argparse.Namespace) -> int:
    existing = set(git("tag", "--list", "nobodywho-*").split())
    for tag in newest_release_tags():
        if tag in existing:
            continue
        if args.create:
            git("tag", tag)
            print(f"created {tag}")
        else:
            print(tag)
    return 0


def cmd_push_tags(args: argparse.Namespace) -> int:
    """Tag HEAD and push the tags one at a time, each once the previous tag's CI run has started.

    GitHub starts no workflows for a push of more than three tags, and runs on the same commit
    share a concurrency group that cancels a pending run when a newer one queues.
    """
    head = git("rev-parse", "HEAD").strip()
    pushed = remote_tags()
    for tag in newest_release_tags():
        if tag in pushed and pushed[tag] != head:
            sys.exit(
                f"::error::{tag} is already on origin, on {pushed[tag]} instead of HEAD"
            )
        if tag not in pushed:
            if git("tag", "--list", tag).strip():
                if git("rev-list", "-n1", tag).strip() != head:
                    sys.exit(f"::error::{tag} already exists locally on another commit")
            else:
                git("tag", tag)
            git("push", "origin", f"refs/tags/{tag}")
            print(f"pushed {tag}")
        wait_for_run(tag, head)
    return 0


def remote_tags() -> dict[str, str]:
    """The release tags on origin, mapped to the commit each points at."""
    tags = {}
    for line in git("ls-remote", "--tags", "origin", "nobodywho-*").splitlines():
        sha, ref = line.split("\t")
        # An annotated tag gets a second `<tag>^{}` line naming its commit.
        tags[ref.removeprefix("refs/tags/").removesuffix("^{}")] = sha
    return tags


def wait_for_run(tag: str, commit: str) -> None:
    """Wait until the tag's Build and test run starts, exiting if it ended without success."""
    last = None
    while True:
        out = subprocess.run(
            ["gh", "run", "list", "--workflow", "build-and-test.yml", "--branch", tag]
            + ["--limit", "5", "--json", "status,conclusion,url,headSha"],
            cwd=ROOT,
            check=True,
            capture_output=True,
            text=True,
        ).stdout
        runs = [run for run in json.loads(out) if run["headSha"] == commit]
        status = runs[0]["status"] if runs else "not created yet"
        if status != last:
            print(f"  {tag}: {status}")
            last = status
        if status == "in_progress":
            return
        if status == "completed":
            if runs[0]["conclusion"] == "success":
                return
            sys.exit(
                f"::error::{tag}'s run ended with {runs[0]['conclusion']}: {runs[0]['url']}"
            )
        time.sleep(30)


def main() -> int:
    parser = argparse.ArgumentParser()
    commands = parser.add_subparsers(required=True)
    commands.add_parser("new").set_defaults(run=cmd_new)
    check = commands.add_parser("check")
    check.add_argument("--pr-base", metavar="REV")
    check.add_argument("--pr-labels", default="", metavar="LABEL,...")
    check.set_defaults(run=cmd_check)
    commands.add_parser("preview").set_defaults(run=cmd_preview)
    commands.add_parser("release").set_defaults(run=cmd_release)
    tag = commands.add_parser("tag")
    tag.add_argument("--create", action="store_true")
    tag.set_defaults(run=cmd_tag)
    commands.add_parser("push-tags").set_defaults(run=cmd_push_tags)
    args = parser.parse_args()
    return args.run(args)


if __name__ == "__main__":
    sys.exit(main())
