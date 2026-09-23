---
name: release
description: Release the nobodywho language bindings — audit the pending `.changeset/` files against the merged PRs, get the computed versions and changelog approved, run `just prepare-release` on a release branch, regenerate Nix and lock files, snapshot Docusaurus docs, then give the user the exact commands to commit, open the release PR and push the tags. Use when the user asks to release, publish, cut a release, or bump versions for any binding (Python, Godot, Flutter, Kotlin, Swift, React Native).
compatibility: Designed for Claude Code. Requires python3, git, gh, cargo, nix and a Node.js toolchain on PATH; the Flutter toolchain is needed for the pubspec.lock sync. On a pure Nix setup these are all provided by the flake's devShell (`nix develop`), so enter the devShell before running the Step 4 commands rather than invoking the tools ad hoc.
---

Release the nobodywho bindings. Each binding is versioned **independently** and tagged separately (`nobodywho-<binding>-vX.Y.Z`). Publishing is done by GitHub Actions (`.github/workflows/release.yml`), triggered by pushing those tags.

**Don't change repository state or anything on GitHub unless the user explicitly asks you to.** That covers creating branches, committing, pushing, opening or merging PRs, creating or pushing tags, and editing GitHub Releases. You run the checks and scripts and edit files in the working copy. At each point where the repository or GitHub has to change (Steps 0, 7 and 8), tell the user exactly what to run, then wait for them to say it's done. Run it yourself only if they ask you to, and only for that step.

**Fail loudly.** Every check in this skill is a stop condition. When one fails, stop and tell the user exactly what failed. Don't work around it, don't repair it quietly, and don't carry on.

The six bindings are **python, godot, flutter, kotlin, react-native, swift**. The `uniffi` and `core` Rust crates are not released on their own, but their `Cargo.toml` versions still feed into `Cargo.lock` / `Cargo.nix`.

## How a release flows

Pending user-facing changes live as one file each in `.changeset/` (format in [CONTRIBUTING.md](../../../CONTRIBUTING.md#changelog-entries)). Each file names the bindings it affects, a bump per binding, and a Keep a Changelog section. **The change files are the single source of truth for both the next versions and the changelog text.** PR checks keep them valid, so they should already be in order at release time. Don't add, edit or delete a change file unless the user asks you to, and never hand-edit what the script writes from them. If something looks wrong, report it; whether and how to fix it is up to the user (Step 1).

`.github/scripts/changesets.py` does the mechanical work:

- `just next-versions` previews the `CHANGELOG.md` entry, including the version each binding would get. It reads only.
- `just prepare-release` publishes nothing. It only edits files in the working copy, preparing the release commit. Every binding named in any change file gets a new version, bumped by the highest level any file gives it. Bindings that no change file names are left alone. It:
  - writes the new version to every location listed in `VERSION_FILES` in the script: the binding `Cargo.toml`s, `pyproject.toml`, `pubspec.yaml`, `build.gradle.kts`, `package.json`, both `package-lock.json` fields, the binding entries in `Cargo.lock` and `uv.lock`, and the Kotlin/Swift install snippets in the docs and README;
  - adds the release's entry to the top of the root `CHANGELOG.md`, covering all released bindings;
  - if Flutter is released, also adds a Flutter-only `## X.Y.Z` section to `nobodywho/flutter/nobodywho/CHANGELOG.md`. That is a separate file: it ships inside the Flutter package, and pub.dev shows it;
  - writes per-binding GitHub release notes to the gitignored `nobodywho/changelogs/<binding>-<version>.md`;
  - deletes the consumed change files.
- `just release-tags` lists the `nobodywho-<binding>-vX.Y.Z` tags for the newest `CHANGELOG.md` heading that don't exist yet. `just push-release-tags` creates and pushes them in Step 8.

Still done by hand: the optional `uniffi`/`core` bumps, `Cargo.nix`, `npmDepsHash`, `pubspec.lock` and the docs snapshot, which you do. The branch, commit, PR, tags and release notes are the user's.

Releases are cut on a **release branch**, named `release-all-bindings-YYYY-MM-DD` by convention, off `origin/main`.

- **The branch fixes the contents.** Creating it decides what the release contains. The branch then gets the release commit and nothing else: no fixes and no merges from `main`.
- **Tags go on the branch head.** The packages are published from there, and a person squash-merges the PR into `main` on GitHub afterwards.
- **It shouldn't conflict with `main`.** The release commit only touches version files, lock and generated files, the changelogs, the consumed change files and the docs snapshot. PRs merged into `main` meanwhile add new change files and never edit `CHANGELOG.md`.

Run the steps in order. Do not skip the approval gates.

---

## Step 0 — Pre-flight

The release must start from a healthy `main`:

```bash
git fetch origin
git status --short        # must be empty
git log -1 --format='%h %s' origin/main
```

Check that the latest `Build and test` run on `main` (`build-and-test.yml`, which runs the full matrix on `main`) is fully green: `build.yml` on all platforms, `regen-checks`, `python-ci`, `kotlin-ci`, `swift-ci` and `linting`. **Do not cut a release from a red `main`.** The tag runs rerun the same jobs, and the release job only runs if they all pass, so a red `main` publishes nothing. Instead, each tag run fails, often hours in, and leaves behind a pushed tag that published nothing. If any job is failing or pending, stop and tell the user.

Then check that the change files are valid and that nothing was written to `CHANGELOG.md` the old way:

```bash
just check-changesets                       # must pass
grep -n '^## \[Unreleased\]' CHANGELOG.md   # must print nothing
ls .changeset/*.md                          # must list at least one file
git ls-remote --heads origin 'release-*'    # check today's branch name isn't taken
```

Stop if any of these fails:

- **An invalid change file** means a PR merged one that CI should have rejected.
- **An `Unreleased` section** means someone edited `CHANGELOG.md` directly. `prepare-release` would put the release above those entries and leave them unreleased. They have to become change files in a PR to `main` first.
- **No change files** means there is nothing to release.

Then ask the user to create the release branch off `origin/main` (fill in today's date), and wait until they have:

```bash
git switch -c release-all-bindings-YYYY-MM-DD origin/main
# or with jj:
jj new main@origin
```

With jj the branch is a bookmark, so it is only created at commit time in Step 7. Do all remaining work in this working copy.

---

## Step 1 — Audit the pending change files

Show what would be released:

```bash
just next-versions
```

Change files are written by many contributors and reviewed one PR at a time. This step is the only time anyone reads them all together, so check them carefully before showing the user anything.

**Coverage: every user-facing change has a file.** List what was merged since the last release, and which PR added each change file:

```bash
last=$(git tag --list 'nobodywho-*' --sort=-creatordate | head -1)
git log --format='%h %s' "$last..origin/main"
for f in .changeset/*.md; do echo "$f  <-  $(git log --diff-filter=A --format=%s -1 -- "$f")"; done
```

Tags usually sit on the previous release branch rather than on `main`. `$last..origin/main` then lists everything merged into `main` since that branch was cut, which is exactly what this release ships. For each PR without its own change file:

- Some are internal: CI, refactors, docs, tests. Leave those out.
- Others may be covered by a change file added in another PR.
- The rest are user-facing changes merged with the `no-changelog` label, or a file was lost. Report them.

**Bumps: each binding's level is right.**

- `major` only for changes that can break existing code: removed or renamed API, changed signature or behaviour, newly rejected input. Also look for the reverse: a breaking change marked `minor` or `patch` is the costly mistake. Read the PR diff when the wording leaves it unclear.
- The binding list matches what users of each binding see. A change in `core/` usually reaches all six. A binding-specific API usually reaches one.
- If one binding needs different wording, that is two change files, not one.

**Wording: the entry reads well as a whole.**

- Merge duplicates, for example two PRs that fixed the same bug.
- The text must not name bindings or say "Breaking". The script adds both from the frontmatter.
- Check that the entries are in sensible sections.

Report every finding to the user, with the change file or PR it concerns and why it looks wrong, and stop there. Don't edit, add or delete change files to fix them unless the user asks you to. The user decides whether something needs fixing and how. Once they say it's settled, re-run `just next-versions` and continue.

---

## Step 2 — Get the versions and the changelog approved

Present the version table built from the `just next-versions` output and the change files:

| Binding | Current | Next | Bump | Driven by |
| --- | --- | --- | --- | --- |

For "Driven by", name the change file or files that set the binding's highest bump. Then show the preview entry itself.

- A binding that no change file names is **not** released. Say so explicitly for each such binding.
- A different version means different bumps in the change files, never edited version files. That change is the user's to make; once they have, re-run the preview and show the table again.
- `prepare-release` has no option to release only some of the pending bindings. If the user wants to hold a binding back, stop and work out with them how to handle its change files before going on.
- Godot's major version is offset from the others (currently `11.x`). Keep its own cadence; do not align it.

Also ask about the two crates that change files don't cover:

- `nobodywho/uniffi/Cargo.toml` is versioned separately from the kotlin/swift/react-native tags. Past releases bumped its minor along with them (0.4.0 → 0.5.0). Propose a bump if the UniFFI surface changed.
- `nobodywho/core/Cargo.toml` (package name `nobodywho`). The user has said a core bump is "not necessary", so **ask before bumping core**.

**Stop and get the user's approval of the versions, the uniffi/core decision and the changelog wording before going on.**

---

## Step 3 — Run the release

```bash
just prepare-release
```

If it stops with `<binding> version files disagree`, stop and tell the user. One of the binding's version locations drifted on `main` since the last release; `git show "nobodywho-<binding>-v<current>:<path>"` shows the released value. The fix belongs in its own PR to `main`, and the release starts again from Step 0 after it lands.

Check the result, and stop if anything differs from this list:

```bash
git status --short
```

- **Expected changes:**
  - every released binding's version files;
  - `nobodywho/Cargo.lock` (python, godot and flutter only) and `nobodywho/python/uv.lock` (python only);
  - `CHANGELOG.md` with one new dated entry at the top;
  - `nobodywho/flutter/nobodywho/CHANGELOG.md` if Flutter is released;
  - every `.changeset/*.md` deleted.
- **Release notes:** `nobodywho/changelogs/` must not appear at all. It is gitignored and holds the GitHub release notes, which are never committed.

If uniffi or core was approved in Step 2, bump them now and update the lock (run from `nobodywho/`):

```bash
# after editing version = "..." in uniffi/Cargo.toml and/or core/Cargo.toml
cargo update -p nobodywho-uniffi --precise <new-version>
cargo update -p nobodywho --precise <new-version>        # core's package name is `nobodywho`
git diff Cargo.lock    # only those crates' version lines should change
```

---

## Step 4 — Regenerate generated files

### 4a. `nobodywho/Cargo.nix` and `nobodywho/crate-hashes.json`

Required whenever any `Cargo.toml` or `Cargo.lock` changed. Without it the Nix CI build fails with "unresolved crate". `crate2nix` reads `Cargo.lock`, so all of Step 3 must be done first. From `nobodywho/`:

```bash
nix run github:nix-community/crate2nix -- generate -h crate-hashes.json
```

If `nix` is not on PATH:
```bash
/nix/var/nix/profiles/default/bin/nix --extra-experimental-features 'nix-command flakes' run github:nix-community/crate2nix -- generate -h crate-hashes.json
```

Commit both files together. The `Cargo.nix` diff can be larger than the version bumps, for example when a prior PR changed `Cargo.lock` without regenerating `Cargo.nix`. That is legitimate; `git log --oneline -- nobodywho/Cargo.lock` shows where it came from.

### 4b. `flake.nix` `npmDepsHash`

Whenever React Native is released, the version change in `package-lock.json` invalidates the `npmDepsHash` of the `react-native-jest` check in `flake.nix`. It changed in every past React Native release. It only surfaces in `nix flake check` (4d), as `npmDepsHash is out of date`. To fix it:

1. In `flake.nix`, replace the `npmDepsHash` value with the literal `sha256-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=`. Do **not** use `lib.fakeHash`, because `lib` is not bound in this flake's `outputs` args.
2. Build to get a hash mismatch: `nix build .#checks.x86_64-linux.react-native-jest`
3. Copy the `got: sha256-...` value into `npmDepsHash`.
4. Build again to confirm it succeeds.

### 4c. `nobodywho/flutter/nobodywho/pubspec.lock`

`pubspec.lock` does not record the package's own version. Still, sync it with any dependency changes that landed since the last release. From `nobodywho/flutter/nobodywho/`:

```bash
flutter pub get
```

Use `flutter pub get`, not `dart pub get`, which fails with "Because nobodywho requires the Flutter SDK". An empty diff is normal.

### 4d. Verify the Nix workspace

**Have the user run this in a separate terminal.** It takes a long time and its output would flood this session:

```bash
nix flake check -L
```

On macOS this only evaluates `aarch64-darwin` derivations; Linux-only checks surface in CI (see the build-integrations skill's "Local Nix blind spot" note). Wait for the user to confirm it passes. The usual failures are a stale `Cargo.nix` (re-run 4a) or a stale `npmDepsHash` (re-run 4b).

---

## Step 5 — Snapshot the Docusaurus docs

For each released binding, freeze the current docs as the new version. From `docs/`:

```bash
cd docs
npx docusaurus docs:version:<binding> <new-version>
# e.g. npx docusaurus docs:version:python 4.0.0
```

This creates `docs/<binding>_versioned_docs/version-<v>/` and `docs/<binding>_versioned_sidebars/version-<v>-sidebars.json`, and prepends the version to `docs/<binding>_versions.json` (see `docs/README.md` → "Cutting docs for a new release").

Then set each released binding's entry in the `latestReleases` map at the top of `docs/docusaurus.config.ts` to its new version. That makes it the default and gives the previous version the "unmaintained" banner.

The new versioned files are untracked with git, so the Step 7 commands use `git add -A` rather than `git add -u`.

---

## Step 6 — Verify

```bash
just check-changesets      # passes with no change files left
just next-versions         # "No change files in .changeset/, nothing to release."
python3 -B -c 'import sys; sys.path.insert(0, ".github/scripts"); import changesets as c; [print(p, c.current_version(p)) for p in c.BINDINGS]'
just release-tags          # exactly one new tag per released binding
```

The `python3` line reads every version location and fails if any of a binding's locations disagree. Check that it prints the approved versions. Also check that `uniffi/Cargo.toml` and `core/Cargo.toml` match what was approved in Step 2.

From `nobodywho/`:

```bash
cargo fmt --all --check
cargo check   # confirms Cargo.lock resolves; cheaper than cargo build
```

---

## Step 7 — Have the user commit and open the release PR

Summarize for the user:

- the version per binding and what drove each bump;
- the files changed: version files, lockfiles, `Cargo.nix`/`crate-hashes.json`, `flake.nix`, both changelogs, the deleted change files, the docs snapshots and `docusaurus.config.ts`;
- the release-note drafts in `nobodywho/changelogs/` (untracked).

Then give them the commands to run, filled in with the branch name and with the bracketed list from the new `CHANGELOG.md` heading as the title:

```bash
git add -A
git commit -m "Release: Python v4.0.0, Flutter v5.0.0, …"
git push -u origin release-all-bindings-YYYY-MM-DD
# or with jj:
jj commit -m "Release: Python v4.0.0, Flutter v5.0.0, …"
jj bookmark create release-all-bindings-YYYY-MM-DD -r @-
jj git push --bookmark release-all-bindings-YYYY-MM-DD   # older jj may also need --allow-new

gh pr create --base main --head release-all-bindings-YYYY-MM-DD \
  --title "Release: Python v4.0.0, Flutter v5.0.0, …" \
  --body "Version bumps, changelogs and docs snapshots for this release." \
  --label no-changelog --label edit-changelog --label full-ci
```

Explain the three labels:

- **`no-changelog`:** the changesets check fails any PR that adds no change file, unless the PR has this label. A release PR deletes change files and adds none.
- **`edit-changelog`:** the same check fails any PR that edits `CHANGELOG.md` or Flutter's changelog, unless the PR has this label. A release PR writes both.
- **`full-ci`:** PR CI only runs the jobs for the paths a PR touches. This label runs the full matrix on the release commit before any tag is pushed, so a failure is caught before anything is published.

Once the user has opened the PR, watch its checks (`gh pr checks <number>` only reads). **Stop if any check fails.** The release branch takes no fixes, so tell the user what failed. The fix goes to `main` in its own PR, and the release starts again from Step 0 after it lands.

---

## Step 8 — Tag and publish

Tags go on the **release branch head**, the commit from Step 7. A person squash-merges the PR afterwards, so the tagged commit never lands on `main` itself.

Once the PR is green, have the user run this with the release commit checked out:

```bash
just push-release-tags
```

Their checkout is right in both cases: with git, the release branch is checked out after Step 7, and with jj, git's `HEAD` is the commit `jj commit` just made. The command:

- creates each missing tag on `HEAD`;
- pushes the tags one at a time, waiting for each tag's `Build and test` run to start before pushing the next;
- stops with an error if a run ends without success, or if a tag already exists on another commit.

Two GitHub behaviours force the one-at-a-time pushing:

- GitHub creates no workflow runs at all when more than three tags are pushed at once.
- Tag runs on the same commit share a concurrency group (`build-and-test.yml`, keyed on the SHA). A group holds one running and one pending run, so a third push cancels the pending one.

The runs therefore execute one after another and the command takes hours. If it is interrupted, running it again skips the tags already pushed. Tell the user all of this when you hand it over.

**If it reports a failed run, stop and tell the user.** No fixes go to the release branch, and no tag is moved or re-pushed, since a published version can't be replaced. Whether to rerun the job or to fix it on `main` and cut a patch release is the user's call.

Each binding's release job creates its GitHub Release with the build artifacts but no notes. The notes come from the `nobodywho/changelogs/<binding>-<version>.md` files that `prepare-release` wrote. As each release appears, give the user both ways to add them:

- **From the terminal**, one command per released binding, filled in:
  ```bash
  gh release edit nobodywho-python-v4.0.0 --notes-file nobodywho/changelogs/python-4.0.0.md
  ```
- **In the GitHub web UI:** open the release (`https://github.com/nobodywho-ooo/nobodywho/releases/tag/<tag>`), click edit, and paste the file's contents into the description.

When every binding is published, tell the user the PR is ready to squash-merge on GitHub. The release branch never gets `main` merged into it.

The PR shouldn't conflict with `main`, since nothing else edits `CHANGELOG.md` and new change files don't overlap the deleted ones. If GitHub still reports a conflict, stop and ask the user. It can happen when `main` edited a consumed change file, or a lockfile or `Cargo.nix`, since the branch was cut.

---

## Checklist

- [ ] `main` clean and fully green; `just check-changesets` passes; change files present; no `Unreleased` section in `CHANGELOG.md` (Step 0)
- [ ] Release branch created off `origin/main` by the user
- [ ] Change files audited: coverage, bumps, wording (Step 1)
- [ ] Version table, uniffi/core decision and changelog preview → **user approval** (Step 2)
- [ ] `just prepare-release` run; diff matches expectations; `nobodywho/changelogs/` untracked (Step 3)
- [ ] uniffi/core bumped and `cargo update -p ... --precise` run, if approved
- [ ] `Cargo.nix` + `crate-hashes.json` regenerated after `Cargo.lock` (4a)
- [ ] `npmDepsHash` updated if React Native is released (4b)
- [ ] `pubspec.lock` synced with `flutter pub get` (4c)
- [ ] `nix flake check -L` passes; the user runs it (4d)
- [ ] Docs snapshotted per released binding, `latestReleases` updated (Step 5)
- [ ] Step 6 checks pass
- [ ] User committed, pushed and opened the PR with `no-changelog`, `edit-changelog` and `full-ci`; every check green (Step 7)
- [ ] User ran `just push-release-tags` and added the GitHub release notes; told the PR is ready to squash-merge (Step 8)
