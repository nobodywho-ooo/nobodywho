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

const LIB_RS: &str = include_str!("../src/lib.rs");
const CORE_CARGO_TOML: &str = include_str!("../../core/Cargo.toml");
const PACKAGE_JSON_TPL: &str = include_str!("../package.json.tpl");
const BUILD_PKG_SH: &str = include_str!("../scripts/build-pkg.sh");
const README_MD: &str = include_str!("../README.md");

// ---------- Helpers ----------

/// Strip bash comments line-by-line. Both whole-line (`# ...`) and inline
/// (`cmd # tail`) comments. Crude — assumes no `#` inside single-quoted
/// strings, which is true for our scripts. Useful for distinguishing
/// "the script says it does X" from "the script's comments mention X."
fn strip_bash_comments(bash: &str) -> String {
    bash.lines()
        .map(|l| {
            let trimmed = l.trim_start();
            if trimmed.starts_with('#') {
                ""
            } else {
                match l.find('#') {
                    Some(i) => &l[..i],
                    None => l,
                }
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Return the doc comment block (consecutive `///` lines, possibly with
/// `#[...]` attributes interleaved) that sits directly above the line
/// matching `signature` in `src`. Empty if the signature isn't preceded
/// by a doc block.
fn doc_above(src: &str, signature: &str) -> String {
    let sig_pos = src.find(signature).expect("signature not found in source");
    let before = &src[..sig_pos];
    let mut doc_lines: Vec<&str> = Vec::new();
    // Walk backwards: take `///` lines, skip attributes/blanks, stop at any other code.
    for line in before.lines().rev() {
        let t = line.trim_start();
        if t.starts_with("///") {
            doc_lines.push(line);
        } else if t.starts_with("#[") || t.is_empty() {
            continue;
        } else {
            break;
        }
    }
    doc_lines.reverse();
    doc_lines.join("\n")
}

// ---------- Findings ----------

/// Finding #1 — stale "Out of scope for v1" comment.
///
/// `ConstraintSpec` is defined (lib.rs ~209-233) and wired into `Chat::new`
/// via `builder.with_sampler(...)` (~lib.rs:253-254). But the comment block
/// at the bottom of lib.rs still lists `Constraint / structured output` as
/// "intentionally not yet wrapped." Confusing to anyone reading the binding
/// to see what's available.
#[test]
fn out_of_scope_section_doesnt_mention_constraint() {
    let after_marker = LIB_RS
        .split("// ---------- Out of scope for v1 ----------")
        .nth(1)
        .expect("missing 'Out of scope for v1' section header in lib.rs");
    assert!(
        !after_marker.contains("`Constraint`"),
        "js/src/lib.rs: 'Out of scope for v1' section still lists `Constraint` \
         — but ConstraintSpec is exposed via Chat::new's options (see lib.rs \
         ~209-233 and ~253-254). Either remove the bullet or, if structured \
         output is still half-wired, describe the actual remaining gap \
         (e.g. the llguidance `Instant::now` panic on wasm32-unknown-unknown)."
    );
}

/// Finding #2 — `Chat::ask` docstring promises streaming that doesn't happen on wasm.
///
/// `Chat::ask`'s docstring says tokens arrive as they're generated. On
/// wasm32-unknown-unknown the inference loop holds the JS event-loop thread
/// until completion, so a `chat.ask(...).then(s => s.nextToken())` loop only
/// drains AFTER the whole generation is done — explicitly documented in
/// `ask_streaming`'s docstring. The `ask` docstring must either point at
/// `askStreaming` for real-time, or describe the buffering explicitly.
#[test]
fn chat_ask_docstring_warns_about_wasm_buffering() {
    let ask_doc = doc_above(LIB_RS, "pub fn ask(&self, prompt: String)");
    let lower = ask_doc.to_lowercase();
    let mentions_workaround = lower.contains("askstreaming")
        || lower.contains("blocks the thread")
        || lower.contains("buffer")
        || lower.contains("only see")
        || lower.contains("only drain");
    assert!(
        mentions_workaround,
        "js/src/lib.rs: Chat::ask docstring doesn't warn that on wasm32 the \
         inference loop holds the JS thread, so the `nextToken()` loop only \
         drains AFTER generation completes — not in real time as the current \
         text implies ('Tokens arrive as they're generated'). Either point \
         readers at askStreaming or describe the buffering directly.\n\n\
         Current docstring:\n{}",
        ask_doc
    );
}

/// Finding #7 — Cargo.toml comment is orphaned from its subject.
///
/// The paragraph about `monty` and `bashkit` being native-only sits at the
/// top of `[dependencies]`, but those crates are declared 35+ lines below
/// in the `[target.'cfg(not(target_arch = "wasm32"))'.dependencies]` block.
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

/// Finding #N1 — `package.json.tpl` hardcodes a release version.
///
/// `version: "0.1.0"` is copied verbatim by `build-pkg.sh` into
/// `pkg-bundler/package.json`. First publish works; second tag (e.g.
/// `nobodywho-js-v0.1.1`) would still publish version "0.1.0", which npm
/// rejects. Acceptable fixes: rewrite the version in `build-pkg.sh` from
/// `$GITHUB_REF_NAME` (or another env var), OR make the template version
/// a clear placeholder so the build step knows it has to substitute.
#[test]
fn package_version_is_placeholder_or_script_rewrites_it() {
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

    // Strip comments first — "Could `npm version` here; keep template as
    // source of truth" is a comment saying we DON'T bump, not evidence we do.
    let code = strip_bash_comments(BUILD_PKG_SH);
    let script_rewrites_version = code.contains("GITHUB_REF_NAME")
        || code.contains("JS_PKG_VERSION")
        || code.contains("npm version")
        || code.contains("npm --no-git-tag-version version")
        // sed/jq edit of the version field in the bundled package.json:
        || (code.contains("pkg-bundler/package.json")
            && (code.contains("sed -i") || code.contains("jq ")));

    assert!(
        is_placeholder || script_rewrites_version,
        "js/package.json.tpl declares a real-looking version ({:?}), but \
         js/scripts/build-pkg.sh copies the template verbatim with no \
         version rewrite. First publish works; second tag would try to \
         re-publish the same version and npm rejects it. Either (a) have \
         build-pkg.sh derive the version from $GITHUB_REF_NAME and rewrite \
         it in the bundled package.json, or (b) make the template version a \
         clearly-placeholder value so the rewrite step is obvious.",
        version_val
    );
}

/// Finding #N2 — `ChatOptions` allows unknown fields, silently dropping them.
///
/// `ConstraintSpec` (lib.rs:213) has `#[serde(deny_unknown_fields, ...)]`.
/// `ChatOptions` (lib.rs:183) doesn't. A JS caller writing
/// `new Chat(model, { tools: [...] })` gets no error — serde drops `tools`
/// and the user wonders why nothing changed. Match `ConstraintSpec`.
#[test]
fn chat_options_denies_unknown_fields() {
    let pos = LIB_RS
        .find("struct ChatOptions")
        .expect("missing ChatOptions struct definition");
    // Walk backward over the immediate attribute lines.
    let preceding = &LIB_RS[..pos];
    let attrs_block = preceding
        .lines()
        .rev()
        .take_while(|l| {
            let t = l.trim_start();
            t.starts_with('#') || t.is_empty()
        })
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>()
        .join("\n");

    assert!(
        attrs_block.contains("deny_unknown_fields"),
        "js/src/lib.rs: ChatOptions doesn't have \
         #[serde(deny_unknown_fields)]. A JS caller passing an unknown \
         option (e.g. `new Chat(model, {{tools: [...]}})`) silently has the \
         field discarded by serde — same shape as ConstraintSpec (line \
         213) which DOES deny unknown fields. Add the attribute for \
         consistency and to make typos surface as errors.\n\n\
         Attributes block above ChatOptions:\n{}",
        attrs_block
    );
}

/// Finding #N3 — `build-pkg.sh` header claims a version bump it doesn't do.
///
/// Line ~10 of the docblock at the top of `build-pkg.sh` says the script
/// produces a `package.json — copied from package.json.tpl, version-bumped`.
/// The actual copy at the bottom (`cp ... pkg-bundler/package.json`) does
/// nothing of the sort, with a comment a few lines later admitting "Could
/// `npm version` here; keep the template as the source of truth." Fix:
/// either implement the bump (which also closes #N1) or drop the claim.
#[test]
fn build_pkg_sh_header_doesnt_lie_about_version_bumping() {
    let header = BUILD_PKG_SH.split("set -euo pipefail").next().unwrap_or("");
    let claims_bump = header.contains("version-bumped") || header.contains("version bumped");
    let code = strip_bash_comments(BUILD_PKG_SH);
    let actually_bumps = code.contains("npm version")
        || code.contains("GITHUB_REF_NAME")
        || code.contains("JS_PKG_VERSION")
        || (code.contains("pkg-bundler/package.json")
            && (code.contains("sed -i") || code.contains("jq ")));

    assert!(
        !claims_bump || actually_bumps,
        "js/scripts/build-pkg.sh: header docblock claims the produced \
         package.json is 'version-bumped', but the script just copies \
         package.json.tpl unchanged. Either drop the 'version-bumped' claim \
         from the header or implement the bump (which also addresses #N1)."
    );
}

/// Finding #R1 — misleading "npm publish will refuse" warning.
///
/// build-pkg.sh's no-version branch said "npm publish will refuse this" —
/// but `0.0.0-PLACEHOLDER` is a valid semver pre-release identifier and
/// npm would happily accept it. The behavior should be: by default, exit
/// non-zero if no version is set (CI always has GITHUB_REF_NAME; this
/// fails closed for local misuse), with an opt-in env var
/// (`JS_ALLOW_PLACEHOLDER=1`) for `npm link` workflows that don't care.
/// Either way the script must not claim that npm rejects the placeholder.
#[test]
fn build_pkg_sh_no_version_path_exits_and_doesnt_lie() {
    // 1. The script must not contain the inaccurate claim.
    assert!(
        !BUILD_PKG_SH.contains("npm publish will refuse"),
        "js/scripts/build-pkg.sh contains the inaccurate claim 'npm \
         publish will refuse this' — but `0.0.0-PLACEHOLDER` is valid \
         semver (pre-release identifier) and npm would accept it. \
         Either tighten the script to actually refuse to proceed (exit \
         non-zero), or reword the warning to describe the real risk."
    );

    // 2. The no-version branch must actually exit (with an opt-out escape
    //    hatch for npm-link workflows). Look for an `exit` keyword in the
    //    else branch's body.
    let code = strip_bash_comments(BUILD_PKG_SH);
    // The structure is `if [[ -n "$VERSION" ]]; then ... else ... fi`.
    // Capture the else branch.
    let else_branch = code
        .split("else")
        .find(|s| s.contains("placeholder") || s.contains("PLACEHOLDER"))
        .unwrap_or("");
    let bounded = else_branch.split("\nfi").next().unwrap_or(else_branch);
    let has_exit = bounded.contains("exit ") || bounded.contains("exit\n");
    let has_allow_escape =
        code.contains("JS_ALLOW_PLACEHOLDER") || code.contains("ALLOW_PLACEHOLDER");

    assert!(
        has_exit && has_allow_escape,
        "js/scripts/build-pkg.sh's no-version else-branch must exit non-\
         zero (so accidental misuse fails loudly) AND provide an opt-in \
         escape (e.g. JS_ALLOW_PLACEHOLDER=1) for `npm link` workflows \
         where the version doesn't matter.\n\n\
         else-branch found:\n{}\n\
         has_exit = {}, has_allow_escape = {}",
        bounded,
        has_exit,
        has_allow_escape
    );
}

/// Finding #R2 — sed substitution doesn't validate `$VERSION`.
///
/// `build-pkg.sh` interpolates `$VERSION` directly into a sed expression.
/// Demonstrated empirically: JS_PKG_VERSION='1.0.0&XXX' produces
/// `"version": "1.0.0"version": "0.0.0-PLACEHOLDER"XXX"` — corrupted JSON.
/// VERSION should be validated against a semver-shaped regex before sed.
#[test]
fn build_pkg_sh_validates_version_before_sed() {
    let code = strip_bash_comments(BUILD_PKG_SH);
    // The validation must specifically operate on $VERSION — look for patterns
    // where the validation construct and $VERSION appear on the same line, so
    // an unrelated `case "$PROFILE" in` elsewhere doesn't satisfy the check.
    let validates_pre = code.lines().any(|l| {
        l.contains("$VERSION")
            && (l.contains("=~") || l.contains("case \"$VERSION\"") || l.contains("grep"))
    });

    assert!(
        validates_pre,
        "js/scripts/build-pkg.sh: $VERSION is interpolated into a sed \
         expression without shape validation. Tested empirically: \
         JS_PKG_VERSION='1.0.0&XXX' produces corrupted JSON because sed \
         treats `&` as the matched-pattern marker. Validate $VERSION (e.g. \
         `[[ \"$VERSION\" =~ ^[0-9]+\\.[0-9]+\\.[0-9]+ ]]`) before sed and \
         exit with a clear error if the tag or env var has an unexpected \
         shape."
    );
}

/// Finding #R4 — README status table contradicts the code.
///
/// The status table in `js/README.md` previously said
/// `Structured output (Constraint) | not yet wired up to the JS surface`.
/// In fact `ConstraintSpec` IS wired through `Chat::new`'s options
/// (see lib.rs `ChatOptions { constraint: Option<ConstraintSpec> }`
/// and the `into_sampler` call). A reader of the table concludes the
/// API doesn't exist yet, when really it does — just with a runtime
/// caveat on wasm32. The status row must reflect reality.
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
         isn't wired up, but ConstraintSpec is exposed via Chat::new's \
         options (see lib.rs:184 ChatOptions.constraint). Update the row \
         to reflect what's actually wired and what the runtime caveat is.\n\n\
         Current row:\n{}",
        row
    );
}

// ---------- Streaming-hook findings (resolved by removal) ----------
//
// Earlier findings (#S1-#S4) all dealt with sharp edges of the per-thread
// streaming-hook design in core::llm: install must reject overlaps, the
// docstrings must warn about concurrency, etc. The hook was originally
// the only way to get tokens-as-they-stream on wasm32-unknown-unknown
// (the sync inference loop blocked the JS event loop, so the
// channel-based TokenStream only drained AFTER completion).
//
// During the Emscripten port we rewrote `Chat::ask_streaming` to drain
// `TokenStream::next_token().await` in a loop instead, then deleted the
// hook module entirely. The drain-the-stream version is equivalent
// under wasm32-unknown-unknown's spawn_local model (each await yields
// the event loop, letting the worker future produce the next token) and
// under future pthread/SharedArrayBuffer models. So findings #S1-#S4
// no longer apply — there's no hook to mis-route tokens through.

// Note: the four "disproof" tests that lived here have been removed.
// They were a one-time verification tool — each one attempted to invalidate
// a finding by looking for a mitigation in one specific place (release.yml
// instead of build-pkg.sh, a separate history element instead of a guarded
// clear, etc.). After the findings were fixed via different paths, they
// became brittle without adding regression value: the positive tests above
// already cover the substantive invariants.
