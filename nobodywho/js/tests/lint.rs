//! Source-text invariants that exist because past code-review passes
//! kept missing them on the human-eye-on-the-PR pass.
//!
//! Each test here was added in response to a specific finding from a
//! code review on the `webassembly` branch (see commit history). They
//! run on the *native* target — `cargo test -p nobodywho-js` — and
//! lint the wasm binding's source / examples / sibling Cargo.toml
//! without needing a wasm toolchain.
//!
//! When a finding is fixed, the corresponding test goes green and is
//! valuable as a regression guard. When a finding is added, the test
//! goes red and the message points at the file + the rationale.

const CORE_CARGO_TOML: &str = include_str!("../../core/Cargo.toml");
const PACKAGE_JSON_TPL: &str = include_str!("../package.json.tpl");
const README_MD: &str = include_str!("../README.md");

// ---------- Findings ----------

/// Finding #7 — Cargo.toml comment is orphaned from its subject.
///
/// The paragraph about `monty` and `bashkit` being native-only sits at the
/// top of `[dependencies]`, but those crates are declared 35+ lines below
/// in the `[target.'cfg(not(target_family = "wasm"))'.dependencies]` block.
/// A reader follows the comment expecting to see the deps it describes;
/// instead they see `serde`, `chrono`, `llama-cpp-2`. Move the comment
/// to be adjacent to the deps it explains.
#[test]
fn cargo_toml_monty_comment_adjacent_to_monty_dep() {
    // The exact prefix style of the explainer comment doesn't matter
    // (`# \`monty\``, `# - \`monty\``, etc.) — only that whichever comment
    // line mentions `monty` is reasonably close to the `monty = ...` dep
    // line, so a reader doesn't lose the thread.
    let monty_dep_pos = CORE_CARGO_TOML
        .find("\nmonty = ")
        .expect("missing monty dep line in core/Cargo.toml");

    let comment_pos = CORE_CARGO_TOML
        .match_indices("monty")
        .map(|(i, _)| i)
        // The `monty` mention must come from a comment line (starts with `#`
        // after the last newline) AND be different from the dep line itself.
        .filter(|&i| {
            if i == monty_dep_pos + 1 {
                return false;
            }
            let line_start = CORE_CARGO_TOML[..i].rfind('\n').map_or(0, |n| n + 1);
            CORE_CARGO_TOML[line_start..i].trim_start().starts_with('#')
        })
        .next()
        .expect("no comment line mentions `monty` in core/Cargo.toml");

    let between = &CORE_CARGO_TOML[comment_pos..monty_dep_pos];
    let lines_between = between.matches('\n').count();
    assert!(
        lines_between < 10,
        "core/Cargo.toml: the comment line that mentions `monty` sits {} \
         lines away from the `monty = ...` dep it describes. Move it next \
         to the deps so readers don't lose the thread looking for the \
         subject.",
        lines_between
    );
}

// ---------- Pass #3 findings (verifying before committing to them) ----------

/// Finding #N1 — `package.json.tpl` must use a clear placeholder version.
///
/// The npm publish path (release.yml's `publish-js-npm` job) renders
/// `pkg-bundler/package.json` from this template, substituting the
/// placeholder with the version from the `nobodywho-js-v*` tag. A
/// hardcoded release-looking version here would defeat that, so the
/// template must keep a clear placeholder.
#[test]
fn package_version_is_placeholder() {
    let version_line = PACKAGE_JSON_TPL
        .lines()
        .find(|l| l.trim_start().starts_with("\"version\":"))
        .expect("missing \"version\" key in package.json.tpl");
    let version_val = version_line
        .split(':')
        .nth(1)
        .unwrap()
        .trim()
        .trim_end_matches(',')
        .trim()
        .trim_matches('"');

    let is_placeholder = version_val == "0.0.0"
        || version_val.contains("TEMPLATE")
        || version_val.contains("PLACEHOLDER")
        || version_val.contains("VERSION");

    assert!(
        is_placeholder,
        "js/package.json.tpl declares a real-looking version ({:?}). \
         Use a clear placeholder (e.g. `0.0.0-PLACEHOLDER`) so the future \
         publish step has an obvious string to substitute.",
        version_val
    );
}

/// Finding #R4 — README status table must mention Constraint.
///
/// The status table in `js/README.md` must have a row mentioning
/// "Constraint" to confirm structured output is documented.
#[test]
fn readme_status_table_doesnt_lie_about_constraint() {
    // Find the Constraint row in the status table and verify its description
    // doesn't claim the API is unwired.
    let row = README_MD
        .lines()
        .find(|l| l.contains("Constraint") && l.starts_with("|"))
        .expect("Constraint row missing from README status table");

    let claims_not_wired = row.contains("not yet wired")
        || row.contains("not wired")
        || row.contains("no JS wrapping")
        || row.contains("not exposed");
    assert!(
        !claims_not_wired,
        "js/README.md: Constraint row in the status table claims the API \
         isn't wired up, but constraints are exposed via SamplerPresets. \
         Update the row to reflect reality.\n\n\
         Current row:\n{}",
        row
    );
}

// ---------- Streaming-hook findings (resolved by removal) ----------
//
// Earlier findings (#S1-#S4) all dealt with sharp edges of the per-thread
// streaming-hook design in core::llm: install must reject overlaps, the
// docstrings must warn about concurrency, etc. The hook was originally
// the only way to get tokens-as-they-stream on wasm32 (the sync
// inference loop blocked the JS event loop, so the channel-based
// TokenStream only drained AFTER completion).
//
// During the Emscripten port we rewrote `Chat::ask_streaming` to drain
// `TokenStream::next_token().await` in a loop instead, then deleted the
// hook module entirely. The drain-the-stream version is equivalent
// under wasm32's spawn_local model (each await yields the event loop,
// letting the worker future produce the next token) and under future
// pthread/SharedArrayBuffer models. So findings #S1-#S4 no longer apply
// — there's no hook to mis-route tokens through.

// Note: the four "disproof" tests that lived here have been removed.
// They were a one-time verification tool — each one attempted to invalidate
// a finding by looking for a mitigation in one specific place (release.yml
// instead of build-pkg.sh, a separate history element instead of a guarded
// clear, etc.). After the findings were fixed via different paths, they
// became brittle without adding regression value: the positive tests above
// already cover the substantive invariants.
