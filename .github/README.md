# CI overview

`build-and-test.yml` runs a **`plan`** job that reads the event, changed paths, and labels and emits a `run_*` flag per bucket; every other job just gates on those flags. Nothing else inspects paths/labels/events.

## Workflow

- **Open a PR and push freely.** Each push runs only the bucket(s) whose paths it touched — cheap, fast feedback. (No draft/non-draft distinction.)
- **Want the full matrix on the PR first?** Add the `full-ci` label or comment `/full-ci`.
- **Merge via the merge queue.** It runs **full CI against the merged tree** — that's the gate. A push to `main` afterwards only deploys docs (the queue already validated it).

## Buckets

| bucket | what runs | path trigger | always on |
|---|---|---|---|
| `lint` | rustfmt + clippy | any | every event |
| `regen` | uniffi/flutter regen-drift checks | any | every event |
| `rust_core` | `nix flake check` | `core/` | tag, merge queue |
| `python` | wheels + pytest + pip-install + multimodal (+ always-on static checks: ruff/ty/stubs) | `python/` | tag, merge queue |
| `python_models` | 6-model tool-calling matrix (linux wheel only) | `core/` | tag, merge queue |
| `godot` | godot build (linux/win/macos/android) | `godot/` | tag, merge queue |
| `flutter` | flutter build + multimodal tests + xcframework | `flutter/` | tag, merge queue |
| `swift` | uniffi Apple build + xcframework + tests | `swift/`, `uniffi/` | tag, merge queue |
| `kotlin` | uniffi build + JVM/Android tests | `kotlin/`, `uniffi/` | tag, merge queue |
| `react_native` | uniffi build + RN xcframework | `react-native/`, `uniffi/` | tag, merge queue |
| `docs` | docusaurus build + Cloudflare Pages deploy | — | main only |
| `release` | publish PyPI / pub.dev / npm / Maven / Swift | — | release tag |

Cross-bucket: `core/**` → `rust_core` + `python_models`; `uniffi/**` → `swift`/`kotlin`/`react_native`. Otherwise a bucket runs only on its own path. `Cargo.lock` and `.github/workflows/**` do **not** auto-trigger full CI — use `full-ci` or the merge queue.

## Triggers

| trigger | runs |
|---|---|
| PR push | buckets touched by that push (+ `uniffi → swift/kotlin/RN`) |
| `full-ci` label / `/full-ci` / `workflow_dispatch` (full_ci) | everything |
| `merge_group` (merge queue) | everything, against the merged tree |
| tag `nobodywho-*` | everything + release |
| push `main` | docs deploy + always-on floor only |
| `[skip ci]` | nothing |

Always-on floor (every event): lint, regen-drift, flutter doctest-drift. Concurrency: PR runs cancel on a new push; `merge_group`/`main`/tags/dispatch run to completion.

## macOS granularity

`cargo-build-macos` is the priciest job, so `matrix-gen` emits an explicit `{integration, target}` list — only the combos a triggered consumer downloads (all release):

| consumer | macOS targets |
|---|---|
| godot / flutter | macOS (x86_64+arm64) + iOS device/sim |
| swift | uniffi: macOS + iOS device/sim, **+ visionOS/watchOS (full runs only)** |
| react-native | uniffi: iOS device/sim |
| kotlin | none (CI tests use the Linux lib) |

visionOS/watchOS compile ONNX Runtime from source and are swift-only, so they build only on full runs (`plan`'s `is_full` → `build.yml`'s `apple_extended`). Partial swift PRs package the xcframework from whatever slices exist; `swift-ci` tests the macOS slice, so it passes. The merge queue (full) verifies all 7 platforms before merge.

## `required` aggregator

`build-and-test.yml` has one `required` job depending on all others; it passes when each result is `success` or `skipped`, fails on `failure`/`cancelled`. **Branch protection should require exactly `required`** — matrix renames never break it.

## Workflow files

```
plan.yml            Source of truth: paths/labels/event → run_* flags.
build-and-test.yml  Entry point: calls plan, gates children, defines `required`.
linting.yml         Always-on rustfmt + clippy.
regen-checks.yml    Always-on regen-drift checks.
build.yml           Per-platform cargo builds; matrix-gen computes integration + macOS matrix.
test.yml            nix flake check (run_rust_core) + flutter tests (run_flutter) + always-on doctest-drift.
python-ci.yml       Static checks always; wheels/tests by run_python; model matrix by run_python_models.
swift-ci.yml        Swift tests. kotlin-ci.yml  Kotlin/Android tests. (both gated upstream)
docs.yml            Docusaurus deploy (main only).
release.yml         Package publish (release tag).
full-ci-comment.yml `/full-ci` comment → label + dispatch full CI.
ai-review.yml       `/ai-review` comment. (independent, not plan-gated)
rust-install-test.yml / test-npm-publish.yml  Standalone smoke tests (dispatch only).
```

## Adding a bucket

1. Add a `run_<bucket>` output in `plan.yml` and its trigger in the `decide` step.
2. In `build-and-test.yml`, add the call with `needs: plan` + `if: needs.plan.outputs.run_<bucket> == 'true'`, and to `required`'s `needs`.
3. Update the table above.
