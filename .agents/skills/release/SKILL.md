---
name: release
description: Prepare a release of one or more nobodywho language bindings — draft per-binding changelogs from git history, propose semver bumps with rationale for approval, bump all version files and lockfiles, snapshot Docusaurus docs, and stage everything for the user to tag and push. Use when the user asks to release, publish, cut a release, or bump versions for any binding (Python, Godot, Flutter, Kotlin, Swift, React Native).
compatibility: Designed for Claude Code. Requires cargo, nix, uv, and a Node.js toolchain on PATH; Flutter/Dart toolchain needed for pubspec.lock sync. On a pure Nix setup these are all provided by the flake's devShell (`nix develop`), so enter the devShell before running the Step 6 commands rather than invoking the tools ad hoc.
---

Prepare a release of one or more nobodywho bindings for the user to review, commit, and tag. Each binding is versioned **independently** and tagged separately (`nobodywho-<binding>-vX.Y.Z`). Publishing itself is done by GitHub Actions (`.github/workflows/release.yml`) triggered by pushing those tags — this skill only stages the changes; it never commits, tags, or pushes without explicit user approval (see `AGENTS.md` Git Policy).

The six bindings are: **python, godot, flutter, kotlin, react-native, swift**. The `uniffi` and `core` Rust crates are not independently released but their `Cargo.toml` versions still feed into `Cargo.lock` / `Cargo.nix`.

Run the steps in order. Do not skip the approval gates.

---

## Step 0 — Pre-flight: clean tree, green CI

Before drafting anything, confirm the release is being cut from a healthy state:

```bash
git status --short        # must be empty — no uncommitted changes
git branch --show-current # should be main
```

Then check that CI on `main` is fully green. Open the latest `Build and test` run on the `main` branch in GitHub (the `main.yml` workflow) and confirm every job passed — including `build.yml` (all platforms), `regen_checks`, `python_ci`, `kotlin_ci`, `swift_ci`, and `linting`. **Do not cut a release from a red `main`** — the release workflow reuses these same build jobs and will publish broken artifacts. If any job is failing or pending, stop and tell the user.

---

## Step 1 — Find the last release tag for each binding

```bash
for b in python godot flutter kotlin react-native swift; do
  echo "$b: $(git tag --list "nobodywho-${b}-v*" --sort=-creatordate | head -1)"
done
```

Record the latest tag per binding. Only prepare a release for bindings the user asked about. For each such binding, the diff base is its last tag:

```bash
git log --oneline <last-tag>..HEAD -- <binding path(s)>
```

Binding source paths (for filtering the log):
- python → `nobodywho/python/`, `nobodywho/core/`, `nobodywho/uniffi/`
- godot → `nobodywho/godot/`, `nobodywho/core/`
- flutter → `nobodywho/flutter/`, `nobodywho/core/`
- kotlin → `nobodywho/kotlin/`, `nobodywho/uniffi/`, `nobodywho/core/`
- react-native → `nobodywho/react-native/`, `nobodywho/uniffi/`, `nobodywho/core/`
- swift → `nobodywho/swift/`, `nobodywho/uniffi/`, `nobodywho/core/`

Also scan the full `<last-tag>..HEAD` log (not path-filtered) for PRs whose title names a feature affecting that binding — core changes (e.g. a new sampler option, a new model family) often touch only `core/` but still land in every binding's changelog. Cross-reference each merged PR number against the diff to decide which bindings it affects.

---

## Step 2 — Draft per-binding changelogs

Write one changelog markdown file per binding being released. Use the existing tracked Flutter changelog as the **style template**:

```
nobodywho/flutter/nobodywho/CHANGELOG.md
```

Each new version section is a `## X.Y.Z` header followed by `### <Feature> (#PR)` subsections, with a short prose line. Match that heading style and tone exactly. Group fixes under a `### Fixes` subsection.

**The per-binding changelog files are NOT tracked in version control.** Write them into the gitignored directory `nobodywho/changelogs/` (already in `.gitignore`), one file per binding, **with the version number in the filename** so they can't be confused with each other or with a previous release's drafts:

```
nobodywho/changelogs/python-1.6.0.md
nobodywho/changelogs/godot-9.5.0.md
nobodywho/changelogs/flutter-2.4.0.md
nobodywho/changelogs/kotlin-2.1.0.md
nobodywho/changelogs/react-native-2.4.0.md
nobodywho/changelogs/swift-2.2.0.md
```

These drafts exist only so the user can copy-paste them into the GitHub Release notes after CI completes. They are intentionally not committed.

Do **not** `git add` anything under `nobodywho/changelogs/`. Verify with `git status` that the directory stays untracked (it should not even appear, since it's gitignored).

> Exception: Flutter's `nobodywho/flutter/nobodywho/CHANGELOG.md` **is** tracked and **is** committed — pub.dev requires a changelog inside the published package. That file is handled in Step 5, not here.

---

## Step 3 — Propose semver bumps and get approval

For each binding, read the changelog drafted in Step 2 and propose a version bump. Present a table:

| Binding | Current | Proposed | Bump | Reason |
| --- | --- | --- | --- | --- |

Apply standard semver **to the binding's public API** (not the Rust core):

- **Major** — any breaking change to the binding's public surface: removed/renamed methods, changed signatures, thrown errors where none were before, changed behavior. (Example: Swift `Chat` constructor becoming `throws` → `nobodywho-swift-v2.0.0`; Kotlin restructuring → `nobodywho-kotlin-v2.0.0`.)
- **Minor** — new features added backward-compatibly (new methods, new optional parameters, new model-family support, new exported functions).
- **Patch** — bug fixes and internal improvements with no API change.

Notes from past releases:
- Godot's version is offset (currently `9.x`) — keep its own major/minor cadence; do not try to align it with the others.
- The `uniffi` crate version (`nobodywho/uniffi/Cargo.toml`) is **decoupled** from the kotlin/swift/react-native tag versions. Bump it only when the UniFFI surface itself changes meaningfully; it is otherwise optional. If bumped, `Cargo.lock` and `Cargo.nix` must be regenerated (Step 6).
- `nobodywho/core/Cargo.toml` was bumped in past all-bindings releases even though core is not published standalone. The user has said the core crate version bump is "not necessary" — **ask before bumping core**; if the user declines, skip it (but still regenerate `Cargo.lock`/`Cargo.nix` if any *other* `Cargo.toml` changed).

**Stop and get the user's approval on the proposed versions and reasons before proceeding.** Do not edit any version files until approved.

---

## Step 4 — Get changelog approval, then sync Flutter's tracked changelog

Show the drafted changelogs to the user and get approval of the wording.

Once approved, for Flutter **only**, prepend the approved `## X.Y.Z` section to the top of the tracked file:

```
nobodywho/flutter/nobodywho/CHANGELOG.md
```

This file ships inside the pub.dev package and must be committed. The scratch changelogs for the other bindings (and for Flutter's GitHub release notes — same text) remain untracked.

---

## Step 5 — Bump version files per binding

Edit each released binding's version file(s) to the approved version. The exact files per binding:

**Python** (two files + lockfile in Step 6):
- `nobodywho/python/Cargo.toml` → `version = "..."`
- `nobodywho/python/pyproject.toml` → `version = "..."`

**Godot:**
- `nobodywho/godot/Cargo.toml` → `version = "..."`

**Flutter** (two files; pubspec.lock handled in Step 6):
- `nobodywho/flutter/rust/Cargo.toml` → `version = "..."`
- `nobodywho/flutter/nobodywho/pubspec.yaml` → `version: ...`

> These two versions **must be identical** — they have been in every past release (2.1.0/2.1.0, 2.2.0/2.2.0, 2.3.0/2.3.0). They track the same published Flutter package. Do not bump them independently.

**Kotlin:**
- `nobodywho/kotlin/build.gradle.kts` → root `version = "..."` (under `allprojects {}`). The subproject `build.gradle.kts` files read `project.version` from this — do not edit them.

**React Native** (two files, including the lockfile):
- `nobodywho/react-native/package.json` → `"version": "..."`
- `nobodywho/react-native/package-lock.json` → **two** `"version"` fields: the top-level one and `packages[""].version`. (The v2.2.0 release forgot the lockfile and v2.3.0 had to catch it up — always bump both.)

**Swift:**
- No version file in the repo. The Swift version comes entirely from the git tag (`nobodywho-swift-vX.Y.Z`); `release.yml` strips the tag prefix and passes `-Pversion`/`VERSION` to the build. There is nothing to edit for Swift beyond tagging.

**uniffi** (if approved in Step 3):
- `nobodywho/uniffi/Cargo.toml` → `version = "..."`

**core** (only if the user approved it in Step 3):
- `nobodywho/core/Cargo.toml` → `version = "..."`

---

## Step 6 — Bump lockfiles and generated files

Every lockfile / generated file that pins a version must be brought in sync. Missing one is the most common release bug. Run the sub-steps **in order**: 6a (`Cargo.lock`) must complete before 6b (`crate2nix`), because `crate2nix` reads `Cargo.lock` — running them out of order produces a `Cargo.nix` that still references the old versions.

### 6a. `nobodywho/Cargo.lock`

If **any** `Cargo.toml` version changed (python, godot, flutter, uniffi, and/or core), update the lock:

```bash
cd nobodywho
cargo update -p <crate-name> --precise <new-version>
# repeat per changed crate, e.g.:
cargo update -p nobodywho-python --precise 1.6.0
cargo update -p nobodywho-flutter --precise 2.4.0
```

Note: the `core` crate's package name is `nobodywho` (not `nobodywho-core` — the directory is `core/` but `Cargo.toml` names the package `nobodywho`), so update it with `cargo update -p nobodywho --precise <v>`.

Verify with `git diff nobodywho/Cargo.lock` — only the bumped crate's `version` lines should change.

### 6b. `nobodywho/Cargo.nix` and `nobodywho/crate-hashes.json`

Required whenever `Cargo.toml` or `Cargo.lock` changed (the Nix CI build breaks with "unresolved crate" if this is skipped). Run from `nobodywho/`:

```bash
nix run github:nix-community/crate2nix -- generate -h crate-hashes.json
```

If `nix` is not on PATH:
```bash
/nix/var/nix/profiles/default/bin/nix --extra-experimental-features 'nix-command flakes' run github:nix-community/crate2nix -- generate -h crate-hashes.json
```

Both `Cargo.nix` and `crate-hashes.json` must be committed together.

The `Cargo.nix` diff may include more than the nobodywho version bumps — e.g. git dependency URL/rev changes from prior PRs whose `Cargo.nix` wasn't regenerated, or transitive crate version resolution picks. These are legitimate. If the diff looks unexpectedly large, `git log --oneline -- nobodywho/Cargo.lock` can confirm whether a prior commit changed a git dependency without regenerating `Cargo.nix`.

### 6c. `nobodywho/python/uv.lock`

The lockfile has an editable `[[package]] name = "nobodywho" version = "..."` entry that must match `pyproject.toml`. Update from `nobodywho/python/`:

```bash
cd nobodywho/python
uv lock
```

Verify: `rg -n -A2 'name = "nobodywho"' uv.lock` shows the new version.

### 6d. `nobodywho/flutter/nobodywho/pubspec.lock`

`pubspec.lock` does **not** record the package's own version (only its dependencies), so a pure version bump does not change it. Run from `nobodywho/flutter/nobodywho/` to make sure the lockfile is in sync with any dependency changes that landed on `main` since the last release:

```bash
cd nobodywho/flutter/nobodywho
flutter pub get
```

Use `flutter pub get` rather than `dart pub get` — this package depends on the Flutter SDK, so `dart pub get` fails with "Because nobodywho requires the Flutter SDK, version solving failed."

If the diff is empty, that's expected — nothing to commit for pubspec.lock this release. If deps changed, the lockfile diff will appear; commit it.

### 6e. `nobodywho/react-native/package-lock.json`

Already handled in Step 5 (edit the two `"version"` fields to match `package.json`). To regenerate rather than hand-edit, from `nobodywho/react-native/`:

```bash
npm install --package-lock-only
```

Either way, confirm `git diff` shows both the top-level `version` and `packages[""].version` updated to the new value.

### 6f. Kotlin / Swift lockfiles

There is no Gradle or SwiftPM lockfile tracked for the Kotlin / Swift bindings — nothing to bump.

### 6g. `flake.nix` `npmDepsHash`

Whenever `react-native/package-lock.json` changed (Step 5/6e), the `npmDepsHash` pinned in `flake.nix` (the `react-native-jest` check derivation) is invalidated. This only surfaces during `nix flake check` (Step 6h), where it fails with `npmDepsHash is out of date`. Fix it before running the check:

1. In `flake.nix`, replace the `npmDepsHash` value with the literal sentinel `sha256-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=` (do **not** use `lib.fakeHash` — `lib` is not bound in this flake's `outputs` args).
2. Build the derivation to trigger a hash mismatch:
   ```bash
   nix build .#checks.x86_64-linux.react-native-jest
   ```
3. Copy the `got: sha256-...` value from the mismatch error back into `npmDepsHash`.
4. Rebuild to confirm it succeeds.

This must be done before Step 6h.

### 6h. Verify the Nix workspace

After 6a–6g, the generated `Cargo.nix` / `crate-hashes.json` and the `npmDepsHash` must actually evaluate. **Have the user run this in a separate terminal** — `nix flake check -L` takes a long time and its verbose output will bloat the LLM context if run in this session:

```bash
nix flake check -L
```

On macOS this only evaluates `aarch64-darwin` derivations locally (Linux-only checks are invisible until CI — see the build-integrations skill's "Local Nix blind spot" note). Wait for the user to confirm it passes before moving on. If it fails, the usual causes are a stale `Cargo.nix` (re-run 6b) or a stale `npmDepsHash` (re-run 6g).

---

## Step 7 — Snapshot Docusaurus docs

For each binding being released, freeze the current `main`-branch docs as the new release version. From `docs/`:

```bash
cd docs
npx docusaurus docs:version:<binding> <new-version>
# e.g. npx docusaurus docs:version:python 1.6.0
```

This creates `docs/<binding>_versioned_docs/version-<v>/`, `docs/<binding>_versioned_sidebars/version-<v>-sidebars.json`, and prepends the version to `docs/<binding>_versions.json`. Repeat for every released binding. (See `docs/README.md` → "Cutting docs for a new release".)

Then update the `latestReleases` map at the top of `docs/docusaurus.config.ts` so the new version becomes the default (no banner) and the previous one gets the "unmaintained" banner automatically:

```ts
const latestReleases: Record<string, string> = {
  python: '1.6.0',   // was '1.5.0'
  // ...update each released binding
};
```

Commit the versioned folders, the `*_versions.json` files, and the config change. These are tracked. The new files under `docs/<binding>_versioned_docs/version-<v>/` and `docs/<binding>_versioned_sidebars/version-<v>-sidebars.json` are **untracked**, so `git add -u` will not pick them up — `git add` them explicitly (e.g. `git add 'docs/*_versioned_docs/' 'docs/*_versioned_sidebars/'`) before committing.

> Not every past release snapshotted docs, but the most recent all-bindings release (`#569`) did. Snapshotting is the correct step and should be done for every released binding that has docs.

---

## Step 8 — Verify

Run from `nobodywho/`:

```bash
cargo fmt --all --check
cargo check   # confirms Cargo.lock versions resolve (cheaper than cargo build; no cross-compile targets needed)
```

Spot-check that no version file was missed — confirm each released binding's files show the approved version:

```bash
# Rust crates (only the ones you bumped):
rg -n '^version' nobodywho/python/Cargo.toml nobodywho/godot/Cargo.toml \
        nobodywho/flutter/rust/Cargo.toml nobodywho/uniffi/Cargo.toml nobodywho/core/Cargo.toml

# Python: pyproject + uv.lock agree
rg -n '^version' nobodywho/python/pyproject.toml
rg -n -A2 'name = "nobodywho"' nobodywho/python/uv.lock

# Flutter: pubspec.yaml matches flutter/rust/Cargo.toml
rg -n '^version:' nobodywho/flutter/nobodywho/pubspec.yaml

# Kotlin
rg -n 'version = ' nobodywho/kotlin/build.gradle.kts

# React Native: package.json + both package-lock.json fields
rg -n '"version"' nobodywho/react-native/package.json
rg -n '"version"' nobodywho/react-native/package-lock.json | head -3
```

Confirm the scratch changelogs are still untracked:

```bash
git status --short   # nobodywho/CHANGELOGS.md (or your scratch path) must show as ??, never staged
```

If a Flutter release was done, confirm `nobodywho/flutter/nobodywho/CHANGELOG.md` is modified (tracked) and contains the new `## X.Y.Z` section at the top.

---

## Step 9 — Report and hand off for tagging

Summarize for the user:
- The approved version per binding and its semver rationale.
- The list of files changed (version files, lockfiles, `Cargo.nix`/`crate-hashes.json`, Flutter `CHANGELOG.md`, docs snapshots, `docusaurus.config.ts`).
- The scratch changelog file paths (untracked) and a reminder to copy-paste each into the corresponding GitHub Release notes after CI completes.
- The exact tag commands the user should run once they've committed and pushed:

```bash
git tag nobodywho-python-v1.6.0
git tag nobodywho-godot-v9.5.0
git tag nobodywho-flutter-v2.4.0
git tag nobodywho-kotlin-v2.1.0
git tag nobodywho-react-native-v2.4.0
git tag nobodywho-swift-v2.2.0
# then: git push origin main --tags
```

Tag format **must** be `nobodywho-<binding>-vX.Y.Z` — `.github/workflows/main.yml` triggers `release.yml` on `tags: ['nobodywho-*']`, and each release job gates on `startsWith(github.ref, 'refs/tags/nobodywho-<binding>-...')`. A malformed tag silently publishes nothing.

**Do not commit, tag, or push.** Per `AGENTS.md`, wait for the user to do it (or to explicitly instruct you to).

---

## Checklist (quick reference)

- [ ] Working tree clean, on `main`, CI fully green (Step 0)
- [ ] Per-binding changelog drafted in `nobodywho/changelogs/<binding>-<v>.md` (gitignored, untracked)
- [ ] Semver proposal per binding with rationale → **user approval**
- [ ] Changelog wording → **user approval**
- [ ] Flutter `CHANGELOG.md` (tracked) prepended with new section
- [ ] Version files bumped per binding (Step 5 table); Flutter's two files identical
- [ ] `Cargo.lock` updated (`cargo update -p ... --precise ...`) — before `crate2nix`
- [ ] `Cargo.nix` + `crate-hashes.json` regenerated (`crate2nix`)
- [ ] `nix flake check -L` passes (user runs in separate terminal)
- [ ] `python/uv.lock` updated (`uv lock`)
- [ ] `flutter/.../pubspec.lock` synced (`dart pub get`)
- [ ] `react-native/package-lock.json` — **both** version fields bumped
- [ ] `flake.nix` `npmDepsHash` updated (Step 6g)
- [ ] Docusaurus docs snapshotted per released binding + `latestReleases` updated
- [ ] `cargo fmt --check` + `cargo check` pass
- [ ] `nobodywho/changelogs/` confirmed untracked
- [ ] Tag commands handed to the user (no commit/tag/push by the assistant)
