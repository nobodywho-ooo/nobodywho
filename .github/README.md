# CI overview

This directory holds the GitHub Actions workflows that build, test, and release nobodywho across its language bindings (Rust core, Python, Godot, Flutter, Swift, Kotlin, React Native).

The headline rule: **`main.yml` invokes a `plan` job that reads the event, paths, and labels, then emits a `run_*` boolean per CI bucket. Every other job gates on those outputs.** No other workflow inspects labels, paths, or the event type.

## The buckets

| bucket | what runs | path triggers (auto-on) | label opt-in | always on |
|---|---|---|---|---|
| `lint` | `cargo fmt --check`, `cargo clippy` | any | — | every event |
| `regen` | uniffi swift / kotlin / RN bindings regen + flutter doctest regen | any | — | every event |
| `rust_core` | `nix flake check` (canonical core test suite) | `core/`, `Cargo.lock` | `full-ci` | main, tag, merge queue |
| `python` | wheel build, pytest, pip-install, multimodal, model-downloading, **plus the always-run python static checks** (ruff/ty/stub regen) | `python/`, `core/`, `Cargo.lock` | `full-ci` | main, tag, merge queue |
| `python_models` | 6-model tool-calling matrix (multi-GB downloads) | — | `test:python-models`, `full-ci` | main, tag, merge queue |
| `godot` | godot integration build (linux, windows, macos, android) | `godot/`, `core/`, `Cargo.lock` | `full-ci` | main, tag, merge queue |
| `flutter` | flutter integration build + multimodal tests + xcframework | `flutter/`, `core/`, `Cargo.lock` | `full-ci` | main, tag, merge queue |
| `swift` | uniffi Apple build + swift xcframework + swift tests | `swift/`, `uniffi/`, `core/`, `Cargo.lock` | `full-ci` | main, tag, merge queue |
| `kotlin` | uniffi build for desktop+android + JVM/Android compile + tests | `kotlin/`, `uniffi/`, `core/`, `Cargo.lock` | `full-ci` | main, tag, merge queue |
| `react_native` | uniffi iOS+android build + RN xcframework | `react-native/`, `uniffi/`, `core/`, `Cargo.lock` | `full-ci` | main, tag, merge queue |
| `docs` | docusaurus build + Cloudflare Pages deploy | `docs/` | — | main only |
| `release` | publish to PyPI / pub.dev / npm / Maven Central / Swift mirror | — | — | release tag only |

Two cascade rules:

- A change to `nobodywho/core/**` cascades to **every** Rust-consuming bucket (each binding links core).
- A change to `nobodywho/uniffi/**` cascades to `swift`, `kotlin`, `react_native`.
- A change to `Cargo.lock` or `.github/workflows/**` forces everything on (safety net).

## Trigger → behavior

| trigger | what runs |
|---|---|
| **Draft PR** opened/updated | only the bucket(s) touched by *the latest push*, no cascade |
| **Non-draft PR** opened/updated | bucket(s) touched by *the whole PR diff* + cross-bucket cascade (core → all bindings, uniffi → swift/kotlin/RN) |
| PR marked **ready-for-review** | re-runs CI in non-draft mode (full diff + cascade) |
| PR with `full-ci` label | every bucket (sticky; remove label to revert) — overrides draft mode |
| PR comment `/full-ci` | one-shot full run via `workflow_dispatch` (see `full_ci_comment.yml`) |
| PR with `test:python-models` label | adds the 6-model matrix on top of path-filtered |
| Push to `main` | every bucket (post-merge canonical run) + docs deploy |
| Push to `nobodywho-*` tag | every bucket + `release.yml` |
| Merge queue (`merge_group`) | every bucket against the merged tree |
| `workflow_dispatch` (Actions UI) | every bucket if `full_ci=true`, else lint+regen only |
| `[skip ci]` in commit message | nothing (GitHub native) |

### Why draft vs non-draft matters

While a PR is **draft**, paths-filter compares only the latest push (`before..after`), and cascade rules are disabled. This lets you push messy core changes without dragging every binding's CI into every iteration. When you flip the PR to **ready-for-review**, CI re-runs with the full cumulative PR diff and the cross-bucket cascade — that's the real verification before merge queue picks it up.

Merge queue refuses to queue drafts, so a draft can't sneak into `main` on the lighter draft-mode CI.

Concurrency: PR runs cancel each other (new push kills stale CI). `merge_group`, `main` pushes, tags, and dispatches always run to completion.

## Workflow files

```
plan.yml             Source of truth. Reads paths/labels/event → emits run_* outputs.
main.yml             Top-level entry. Calls plan, wires every child workflow's gates,
                     defines the `required` aggregator and concurrency.

linting.yml          Always-on Rust lint (rustfmt, clippy).
regen_checks.yml     Always-on regen-drift checks (uniffi bindings + flutter doctests).

build.yml            Per-platform cargo builds (linux, windows, macos, android).
                     A `matrix-gen` job emits the integration list from the bucket
                     flags; each platform job is gated on whether any consumer
                     bucket needs it. Apple xcframeworks (flutter / RN / swift)
                     are separate jobs gated on their owner bucket.
test.yml             nix flake check (gated by run_rust_core),
                     flutter multimodal tests (gated by run_flutter).
python_ci.yml        Static checks always run when invoked; wheel builds + pytest
                     gated by run_python; 6-model matrix by run_python_models.
swift_ci.yml         Swift macro + integration tests. Single job, gated upstream.
kotlin_ci.yml        Kotlin/JVM + Android tests. Single job, gated upstream.

docs.yml             Build + deploy docusaurus to Cloudflare Pages. Gated by main.yml
                     to main branch only.
release.yml          PyPI / pub.dev / npm / Maven Central / Swift mirror publish.
                     Gated by release tag.

full_ci_comment.yml  Handles `/full-ci` PR comments: permission-checks the commenter,
                     dispatches main.yml with full_ci=true.

rust-install-test.yml  Smoke test that `cargo check -p nobodywho-rust` works on
                       Ubuntu / macOS / Windows / Nix. workflow_dispatch only.
test_npm_publish.yml   Standalone test for the React Native npm publish flow.
docs.yml               (above)
```

## The `required` aggregator

`main.yml` defines a single job named `required` that depends on every other top-level job. It passes when each `needs.*.result` is `success` or `skipped` (skipped is correct when the plan job gated a bucket out). Failed or cancelled = it exits 1.

**Branch protection on `main` should require exactly one check: `required`.** Renaming a matrix entry inside `build.yml` doesn't break protection because protection points only at the aggregator.

## How to add a new bucket

If you add a new binding or test scope:

1. Add a `run_<new_bucket>` workflow_call output in `plan.yml` and decide its trigger logic in the `decide` step.
2. In `main.yml`, add the workflow call with `needs: plan` and `if: needs.plan.outputs.run_<new_bucket> == 'true'`.
3. Add the new workflow call to the `required` aggregator's `needs:` list.
4. Update the table at the top of this file.

## Manually re-running a stalled merge queue run

If a `merge_group` run fails for an infrastructure reason (flaky runner, network blip), re-trigger by:

1. Re-queue the PR from the GitHub UI (Merge → Add to queue), or
2. Push an empty commit to the PR branch to bump it, then re-queue.

Don't cancel an in-flight `merge_group` run — `cancel-in-progress` is intentionally `false` for them.
