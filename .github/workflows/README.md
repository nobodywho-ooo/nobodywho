# CI overview

`build-and-test.yml` runs a **`plan`** job that reads the event, changed paths, and labels and emits a `run_*` flag per bucket; every other job just gates on those flags. Nothing else inspects paths/labels/events.

## Workflow

- **Open a PR and push freely.** Each push runs only the bucket(s) whose paths it touched — cheap, fast feedback. (No draft/non-draft distinction.)
- **Per-push CI is intentionally partial** — it's not complete or thorough, so green on a PR push does **not** mean the whole project passes. For the full matrix, add the `full-ci` label or comment `/full-ci`.
- **Merge on green PR checks; `main` runs the full matrix.** Because PR CI is partial, the **post-merge full run on `main`** is the real safety net — if a change breaks something its PR didn't cover, `main` goes red and you fix forward. Run `/full-ci` on the PR first when you want the full matrix before merging.

## Buckets

| bucket | what runs | path trigger | always on |
|---|---|---|---|
| `lint` | rustfmt + clippy | any | every event |
| `regen` | uniffi/flutter bindings regen-drift | `core/`, `uniffi/`, `grammar/`, `Cargo.*`, `*/generated/`, binding config | main, tag |
| `rust_core` | `nix flake check` | `core/` | main, tag |
| `python` | wheels + pytest + pip-install + multimodal (+ always-on static checks: ruff/ty/stubs) | `python/` | main, tag |
| `python_models` | 6-model tool-calling matrix (linux wheel only) | `core/` | main, tag |
| `godot` | godot build (linux/win/macos/android) | `godot/` | main, tag |
| `flutter` | flutter build + multimodal tests + xcframework | `flutter/`, `android/` | main, tag |
| `swift` | uniffi Apple build + xcframework + tests | `swift/`, `uniffi/` | main, tag |
| `kotlin` | uniffi build + JVM/Android tests | `kotlin/`, `uniffi/`, `android/` | main, tag |
| `react_native` | uniffi build + RN xcframework | `react-native/`, `uniffi/`, `android/` | main, tag |
| `apple_extended` | uniffi visionOS/watchOS device+sim (nightly rust, ORT from source) | — (never path-triggered) | main, tag |
| `docs` | docusaurus build + Cloudflare Pages deploy | — | main only |
| `device` | on-device tests on real phones (Firebase Test Lab) | — | nightly on main; release tag (that binding only) |
| `release` | publish PyPI / pub.dev / npm / Maven / Swift | — | release tag |

Cross-bucket: `core/**` → `rust_core` + `python_models`; `uniffi/**` → `swift`/`kotlin`/`react_native`; `android/**` and the Android-specific Cargo/CMake build controls → `flutter`/`kotlin`/`react_native`. Otherwise a bucket runs only on its own path. `Cargo.lock` and `.github/workflows/**` do **not** auto-trigger full CI — use `full-ci`, or they're caught by the post-merge full run on `main`.

## Triggers

| trigger | runs |
|---|---|
| PR push | buckets touched by that push (+ `uniffi → swift/kotlin/RN`) |
| `full-ci` label / `/full-ci` comment / `workflow_dispatch` (full_ci) | everything |
| `/<bucket>-ci` comment(s) | exactly the named buckets (see PR comment commands below) |
| tag `nobodywho-*` | everything + release (+ that binding's device tests, if it has any) |
| push `main` | everything (post-merge full CI) + docs deploy |
| nightly 04:00 UTC on `main` | all device tests (`build-and-test.yml` schedule) |
| `[skip ci]` | nothing |

Always-on floor (every event): lint + flutter doctest-drift. Concurrency: PR runs cancel on a new push; `main`/tags/dispatch run to completion.

Device tests are the exception to "push to `main` runs everything": they run on real
phones and bill per device-minute, so a push never triggers them. Main is covered by
the nightly run instead.

### PR comment commands

Comment on a PR to run CI by hand (write/admin only). The comment must be **only** slash-commands, whitespace/newline separated — any other text, or any unknown command, is rejected and nothing runs. Multiple commands combine.

| comment | runs |
|---|---|
| `/full-ci` | everything (shows a `full-ci` label while running) |
| `/core-ci` | `rust_core` |
| `/python-ci` | `python` |
| `/python-models-ci` | `python_models` |
| `/godot-ci` `/flutter-ci` `/swift-ci` `/kotlin-ci` `/react-native-ci` | that binding |
| `/apple-extended-ci` | `apple_extended` — the 4 visionOS/watchOS targets only (compile check; no xcframework) |
| `/regen-ci` | `regen` |
| `/device-ci` | all six on-device jobs |
| `/kotlin-device-source-ci` `/flutter-device-source-ci` `/react-native-device-source-ci` | that binding on real phones, built from this repo |
| `/kotlin-device-released-ci` `/flutter-device-released-ci` `/react-native-device-released-ci` | that binding on real phones, from its published package |

Example: `/swift-ci /kotlin-ci /python-ci` runs those three. `/full-ci` overrides any others in the same comment.

Device commands are never part of `/full-ci` — they cost device minutes, so they are
always opt-in. `source` builds the binding from this repo (what you are about to
ship); `released` builds against the published package (what users have today), so a
red `released` job means something already shipped is broken.

## macOS granularity

`cargo-build-macos` is the priciest job, so `matrix-gen` emits an explicit `{integration, target}` list — only the combos a triggered consumer downloads (all release):

| consumer | macOS targets |
|---|---|
| godot / flutter | macOS (x86_64+arm64) + iOS device/sim |
| swift | uniffi: macOS + iOS device/sim |
| react-native | uniffi: iOS device/sim |
| kotlin | none (CI tests use the Linux lib) |
| apple_extended | uniffi: visionOS device/sim + watchOS device/sim |

visionOS/watchOS are tier-3 Rust targets: they need `cargo +nightly -Z build-std` and compile ONNX Runtime from source, so they never run on a path trigger. They build on full runs, or on demand via `/apple-extended-ci`.

`apple_extended` is a bucket of its own, independent of `swift` — `/apple-extended-ci` alone builds exactly those 4 targets and nothing else, which is the cheap way to check a change still compiles on nightly. It skips `build-swift-xcframework` (gated on `run_swift`), so combine it as `/swift-ci /apple-extended-ci` when you want the packaged 7-slice xcframework. Partial swift PRs package the xcframework from whatever slices exist; `swift-ci` tests the macOS slice, so it passes.

## Release gating

`release` depends on every other job, including `mobile-device-tests`. It uses
`always() && … && !contains(needs.*.result, 'failure' | 'cancelled')`, so a *skipped*
job does not block a release while a *failed* one does. That distinction matters
because device tests are skipped for bindings that have none — a Swift or Python tag
releases normally, while a `nobodywho-kotlin-v*` tag cannot publish unless its
`kotlin-source` device job passed on real hardware.

Android source-device tests consume
`nobodywho-android-binding-candidates`, built once from `build.yml`'s
arm64-v8a and x86_64 artifacts. The candidate workflow adds `libc++_shared.so`,
strips, and validates Flutter's and React Native's native AARs, each versioned by its owning
binding, plus the UniFFI native AAR that Kotlin embeds into `nobodywho-android`.

The source-device jobs do not link binding source directories into the test apps.
They first create the same distributor shape as a release: a runner-local Maven
repository containing the Kotlin AAR/POM publications, pub's generated Flutter
`.tar.gz`, or npm's generated React Native `.tgz`. The app installs that candidate
and is then tested on Firebase hardware. The native AARs are resolved through
their normal, same-version Maven coordinates in an isolated runner-local
repository. A successful binding release publishes that exact tested AAR before
publishing its wrapper package. Candidate jobs have no registry credentials.

## Workflow files

```
plan.yml            Source of truth: paths/labels/event → run_* flags.
build-and-test.yml  Entry point: calls plan and gates children.
linting.yml         Always-on rustfmt + clippy.
regen-checks.yml    Bindings regen-drift checks (gated by run_regen).
build.yml           Per-platform cargo builds; matrix-gen computes integration + macOS matrix.
package-android-candidates.yml  Builds the three exact binding-owned Android candidates and local Maven repository.
test.yml            nix flake check (run_rust_core) + flutter tests (run_flutter) + always-on doctest-drift.
python-ci.yml       Static checks always; wheels/tests by run_python; model matrix by run_python_models.
swift-ci.yml        Swift tests. kotlin-ci.yml  Kotlin/Android tests. (both gated upstream)
docs.yml            Docusaurus deploy (main only).
release.yml         Package publish (release tag).
mobile-device-tests.yml  On-device tests on real phones via Firebase Test Lab.
                    Only called from build-and-test (nightly, release gating,
                    /device comments, or a dispatch with
                    `buckets: device_<binding>_<mode>`). Six jobs:
                    <binding>-source (built from this repo) and
                    <binding>-released (from the published package).
                    Shared steps live in .github/actions/run-ftl.
ci-command.yml      Parses /<bucket>-ci PR comments (strict; hard-rejects unknown) →
                    dispatches build-and-test with the selected buckets.
ai-review.yml       `/ai-review` comment. (independent, not plan-gated)
rust-install-test.yml / test-npm-publish.yml  Standalone smoke tests (dispatch only).
```

## Adding a bucket

1. Add a `run_<bucket>` output in `plan.yml` and its trigger in the `decide` step.
2. In `build-and-test.yml`, add the call with `needs: plan` + `if: needs.plan.outputs.run_<bucket> == 'true'`. Add it to `release`'s `needs` only if a release should be blocked when it fails.
3. If it should be reachable from a PR comment, add the token to `ci-command.yml`'s `MAP`.
4. Update the table above.
