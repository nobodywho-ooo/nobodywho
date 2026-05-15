//! Source-text invariants that exist because past code-review passes
//! kept missing them on the human-eye-on-the-PR pass.
//!
//! Each test here was added in response to a specific finding from a
//! code review on the `webassembly` branch (see commit history). They
//! run on the *native* target — `cargo test -p nobodywho-wasm` — and
//! lint the wasm binding's source / examples / sibling Cargo.toml
//! without needing a wasm toolchain.
//!
//! When a finding is fixed, the corresponding test goes green and is
//! valuable as a regression guard. When a finding is added, the test
//! goes red and the message points at the file + the rationale.

const LIB_RS: &str = include_str!("../src/lib.rs");
const BROWSER_CHAT_HTML: &str = include_str!("../examples/browser-chat.html");
const BROWSER_CHAT_WORKER_HTML: &str = include_str!("../examples/browser-chat-worker.html");
const CORE_CARGO_TOML: &str = include_str!("../../core/Cargo.toml");
const CORE_LLM_RS: &str = include_str!("../../core/src/llm.rs");
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
        "wasm/src/lib.rs: 'Out of scope for v1' section still lists `Constraint` \
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
        "wasm/src/lib.rs: Chat::ask docstring doesn't warn that on wasm32 the \
         inference loop holds the JS thread, so the `nextToken()` loop only \
         drains AFTER generation completes — not in real time as the current \
         text implies ('Tokens arrive as they're generated'). Either point \
         readers at askStreaming or describe the buffering directly.\n\n\
         Current docstring:\n{}",
        ask_doc
    );
}

/// Finding #5 — `browser-chat.html` claims to stream tokens, but doesn't.
///
/// The file's header text says it "streams tokens", but its script uses the
/// `chat.ask().nextToken()` loop, which batches on wasm32 (see Finding #2).
/// Either rewrite the demo to use `askStreaming` (then it'd duplicate
/// `browser-chat-worker.html`), or stop claiming streaming in the header.
#[test]
fn browser_chat_html_doesnt_claim_streaming() {
    // The interesting prose lives between <body> and the <script> block.
    let body_start = BROWSER_CHAT_HTML
        .find("<body>")
        .expect("missing <body> in browser-chat.html");
    let script_start = BROWSER_CHAT_HTML[body_start..]
        .find("<script")
        .map(|i| body_start + i)
        .expect("missing <script in browser-chat.html");
    let header = &BROWSER_CHAT_HTML[body_start..script_start];

    assert!(
        !header.contains("streams tokens") && !header.contains("stream tokens"),
        "wasm/examples/browser-chat.html: page header still claims it 'streams \
         tokens', but the JS uses `chat.ask().nextToken()` which batches on \
         wasm32 main-thread. Drop the streaming claim from the header, or \
         delete this file in favour of browser-chat-worker.html (which uses \
         askStreaming + a Web Worker and actually streams)."
    );
}

/// Finding #6 — `browser-chat-worker.html` recreates the chat on every Ask click.
///
/// The $askBtn click handler unconditionally posts `create-chat` to the
/// worker. The worker's `createChat` reassigns the module-scope `chat`,
/// so each click starts a fresh conversation — follow-ups have no memory
/// of earlier turns. Should either be created once after `model-loaded`,
/// or gated on a flag like "chat already exists for this model + options".
#[test]
fn worker_ask_handler_does_not_create_chat_every_time() {
    let after_handler = BROWSER_CHAT_WORKER_HTML
        .split("$askBtn.addEventListener('click', ")
        .nth(1)
        .expect("missing $askBtn click handler in browser-chat-worker.html");
    // Capture everything up to the listener's closing `});`.
    let handler_end = after_handler
        .find("\n    });")
        .expect("malformed click handler — no closing `});`");
    let handler_body = &after_handler[..handler_end];

    // Acceptable shapes: either the handler doesn't post 'create-chat' at
    // all (chat created elsewhere — e.g. on model-loaded), or it gates the
    // post on a flag/sentinel.
    let posts_create_chat = handler_body.contains("'create-chat'");
    let has_creation_guard = handler_body.contains("chatReady")
        || handler_body.contains("chatCreated")
        || handler_body.contains("chatExists")
        || handler_body.contains("hasChat")
        || handler_body.contains("if (!chat")
        || handler_body.contains("if (chat == null")
        || handler_body.contains("if (!hasChat");

    assert!(
        !posts_create_chat || has_creation_guard,
        "wasm/examples/browser-chat-worker.html: every $askBtn click \
         unconditionally posts 'create-chat' to the worker, which replaces \
         the existing chat session and loses conversation history between \
         turns. Create the chat once after 'model-loaded' (or gate the \
         'create-chat' post on a flag), so follow-ups can build on previous \
         answers.\n\nClick-handler body:\n{}",
        handler_body
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
/// `nobodywho-wasm-v0.1.1`) would still publish version "0.1.0", which npm
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
        || code.contains("WASM_PKG_VERSION")
        || code.contains("npm version")
        || code.contains("npm --no-git-tag-version version")
        // sed/jq edit of the version field in the bundled package.json:
        || (code.contains("pkg-bundler/package.json")
            && (code.contains("sed -i") || code.contains("jq ")));

    assert!(
        is_placeholder || script_rewrites_version,
        "wasm/package.json.tpl declares a real-looking version ({:?}), but \
         wasm/scripts/build-pkg.sh copies the template verbatim with no \
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
        "wasm/src/lib.rs: ChatOptions doesn't have \
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
        || code.contains("WASM_PKG_VERSION")
        || (code.contains("pkg-bundler/package.json")
            && (code.contains("sed -i") || code.contains("jq ")));

    assert!(
        !claims_bump || actually_bumps,
        "wasm/scripts/build-pkg.sh: header docblock claims the produced \
         package.json is 'version-bumped', but the script just copies \
         package.json.tpl unchanged. Either drop the 'version-bumped' claim \
         from the header or implement the bump (which also addresses #N1)."
    );
}

/// Finding #N5 — Worker chat demo wipes visible history on every Ask.
///
/// After Fix #6 the model keeps full conversation context across turns. But
/// the click handler still does `$out.textContent = ''` unconditionally at
/// the start of each click, so the previous turn's visible response gets
/// wiped. The user sees a one-turn view even though the model has the full
/// history. Either gate the clear on `!chatReady` (first turn only),
/// append turns with role labels, or rename the demo to match what it does.
#[test]
fn worker_chat_demo_preserves_previous_turn_in_ui() {
    let after_handler = BROWSER_CHAT_WORKER_HTML
        .split("$askBtn.addEventListener('click', ")
        .nth(1)
        .expect("missing $askBtn click handler in browser-chat-worker.html");
    let handler_end = after_handler
        .find("\n    });")
        .expect("malformed click handler — no closing `});`");
    let handler_body = &after_handler[..handler_end];

    // Find the `$out.textContent = ''` line and check if it's gated.
    let clears_pos = handler_body.find("$out.textContent = ''");
    if let Some(pos) = clears_pos {
        // Heuristic: the clear is "guarded" if it appears either inside a
        // visible `if (...)` block in the same handler (rough check: an `if`
        // appears in the preceding 80 chars) or after a `chatReady` check.
        let context_start = pos.saturating_sub(120);
        let preceding = &handler_body[context_start..pos];
        let guarded = preceding.contains("if (") && preceding.contains("chatReady");

        assert!(
            guarded,
            "wasm/examples/browser-chat-worker.html: $askBtn click handler \
             does `$out.textContent = ''` unconditionally at the top, wiping \
             the previous turn's response from the visible area. After \
             Fix #6 the model retains full history but the user sees only \
             the latest turn. Either gate the clear on `!chatReady` (so \
             only the first turn clears), append turns with role labels \
             (`You: ...`, `Assistant: ...`), or be explicit in the demo's \
             header that it shows only the latest turn.\n\n\
             Handler body:\n{}",
            handler_body
        );
    }
}

/// Finding #R1 — misleading "npm publish will refuse" warning.
///
/// build-pkg.sh's no-version branch said "npm publish will refuse this" —
/// but `0.0.0-PLACEHOLDER` is a valid semver pre-release identifier and
/// npm would happily accept it. The behavior should be: by default, exit
/// non-zero if no version is set (CI always has GITHUB_REF_NAME; this
/// fails closed for local misuse), with an opt-in env var
/// (`WASM_ALLOW_PLACEHOLDER=1`) for `npm link` workflows that don't care.
/// Either way the script must not claim that npm rejects the placeholder.
#[test]
fn build_pkg_sh_no_version_path_exits_and_doesnt_lie() {
    // 1. The script must not contain the inaccurate claim.
    assert!(
        !BUILD_PKG_SH.contains("npm publish will refuse"),
        "wasm/scripts/build-pkg.sh contains the inaccurate claim 'npm \
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
        code.contains("WASM_ALLOW_PLACEHOLDER") || code.contains("ALLOW_PLACEHOLDER");

    assert!(
        has_exit && has_allow_escape,
        "wasm/scripts/build-pkg.sh's no-version else-branch must exit non-\
         zero (so accidental misuse fails loudly) AND provide an opt-in \
         escape (e.g. WASM_ALLOW_PLACEHOLDER=1) for `npm link` workflows \
         where the version doesn't matter.\n\n\
         else-branch found:\n{}\n\
         has_exit = {}, has_allow_escape = {}",
        bounded,
        has_exit,
        has_allow_escape
    );
}

/// Finding #R3 — chat-creation failure leaves `onceChatReady` attached and
/// produces duplicate `sendAsk` calls on the next successful Ask.
///
/// `onceChatReady` only checks `e.data.type === 'chat-ready'` before
/// removing itself. If the worker sends `'error'` instead (e.g. unknown
/// chat option after the N2 `deny_unknown_fields` fix), the listener
/// stays. The next Ask click adds another listener. When create-chat
/// finally succeeds, multiple listeners fire and each calls `sendAsk()`.
/// The handler must also clean itself up on `'error'` (and ideally
/// re-enable the button + reset chatReady).
#[test]
fn worker_chat_handler_cleans_up_on_error() {
    let after_handler = BROWSER_CHAT_WORKER_HTML
        .split("$askBtn.addEventListener('click', ")
        .nth(1)
        .expect("missing $askBtn click handler");
    let handler_end = after_handler
        .find("\n    });")
        .expect("malformed click handler");
    let handler_body = &after_handler[..handler_end];

    // Locate the onceChatReady definition; its body has to handle 'error'
    // as well as 'chat-ready'.
    let listener_pos = handler_body
        .find("onceChatReady")
        .expect("onceChatReady listener missing");
    let listener_block = &handler_body[listener_pos..];

    let handles_error = listener_block.contains("'error'") || listener_block.contains("\"error\"");

    assert!(
        handles_error,
        "wasm/examples/browser-chat-worker.html: onceChatReady listener \
         only handles 'chat-ready'. If chat creation fails, the worker \
         sends 'error', the listener stays attached, and the next Ask \
         click registers ANOTHER listener — when create-chat finally \
         succeeds, multiple listeners fire and each calls sendAsk(), \
         producing duplicate generations. The listener must also clean \
         itself up on 'error' (and re-enable the button).\n\n\
         onceChatReady block:\n{}",
        &listener_block[..listener_block.len().min(400)]
    );
}

/// Finding #R2 — sed substitution doesn't validate `$VERSION`.
///
/// `build-pkg.sh` interpolates `$VERSION` directly into a sed expression.
/// Demonstrated empirically: WASM_PKG_VERSION='1.0.0&XXX' produces
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
        "wasm/scripts/build-pkg.sh: $VERSION is interpolated into a sed \
         expression without shape validation. Tested empirically: \
         WASM_PKG_VERSION='1.0.0&XXX' produces corrupted JSON because sed \
         treats `&` as the matched-pattern marker. Validate $VERSION (e.g. \
         `[[ \"$VERSION\" =~ ^[0-9]+\\.[0-9]+\\.[0-9]+ ]]`) before sed and \
         exit with a clear error if the tag or env var has an unexpected \
         shape."
    );
}

/// Finding #R4 — README status table contradicts the code.
///
/// The status table in `wasm/README.md` previously said
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
        "wasm/README.md: Constraint row in the status table claims the API \
         isn't wired up, but ConstraintSpec is exposed via Chat::new's \
         options (see lib.rs:184 ChatOptions.constraint). Update the row \
         to reflect what's actually wired and what the runtime caveat is.\n\n\
         Current row:\n{}",
        row
    );
}

// ---------- Streaming-hook concurrency findings ----------
//
// These cover a single class of bug: the per-thread streaming hook in
// core::llm has one slot, and `Chat::askStreaming` is the only consumer,
// but the original wiring used a save-and-restore pattern that wrongly
// claimed to support "nested or concurrent asks." The worker queue is
// FIFO; save-and-restore is LIFO. Two overlapping installs misroute
// tokens. Fix: install refuses to overwrite an occupied slot and the
// docs name the constraint.

/// Finding #S1 — `HookRestore::install` must reject overlapping installs.
///
/// Originally returned bare `Self` and blindly overwrote the slot. Two
/// `askStreaming` calls in flight at once would land their callbacks in
/// LIFO order while the worker processes asks FIFO — the inner install
/// receives the outer's tokens, then the outer's drop wipes the inner's
/// hook before its tokens arrive. Fix: install returns
/// `Result<Self, JsError>` and refuses an occupied slot, so the second
/// `askStreaming` rejects loudly instead of misrouting silently.
#[test]
fn hook_restore_install_returns_result() {
    let install_pos = LIB_RS
        .find("fn install(hook:")
        .expect("HookRestore::install signature not found in lib.rs");
    // The signature may span lines; read forward enough to capture the
    // return-type arrow + the next token.
    let snippet = &LIB_RS[install_pos..(install_pos + 200).min(LIB_RS.len())];
    assert!(
        snippet.contains("-> Result<Self, JsError>"),
        "wasm/src/lib.rs: HookRestore::install must return \
         `Result<Self, JsError>`, not `Self`. A bare-Self return silently \
         overwrites whatever hook the previous in-flight `askStreaming` \
         installed, which routes its tokens to the wrong JS callback. The \
         Result variant lets the second call reject with a clear JS error.\n\n\
         First 120 chars of signature:\n{}",
        snippet.chars().take(120).collect::<String>()
    );
}

/// Finding #S2 — `ask_streaming` docstring must document the
/// concurrent-call rejection so users know to serialize.
///
/// `HookRestore::install` errors when a hook is already installed, but a
/// user who doesn't read the source has no way to predict that
/// `Promise.all([chat.askStreaming(...), chat.askStreaming(...)])` will
/// reject. The docstring is the contract.
#[test]
fn ask_streaming_doc_warns_about_concurrency() {
    let ask_streaming_doc = doc_above(
        LIB_RS,
        "pub fn ask_streaming(&self, prompt: String, on_token:",
    );
    let lower = ask_streaming_doc.to_lowercase();
    let names_constraint = lower.contains("already in progress")
        || lower.contains("concurrent")
        || lower.contains("one streaming call")
        || lower.contains("one at a time");
    let names_remedy = lower.contains("await") || lower.contains("serialize");
    assert!(
        names_constraint && names_remedy,
        "wasm/src/lib.rs: Chat::ask_streaming docstring must warn that \
         overlapping calls are not supported and point users at the \
         remedy (await the previous promise). Without this, a JS user \
         who fires two askStreaming calls without awaiting gets a \
         confusing rejection with no docstring to consult.\n\n\
         Current docstring:\n{}",
        ask_streaming_doc
    );
}

/// Finding #S3 — `ask_streaming` docstring must warn against mixing
/// with in-flight `ask`.
///
/// `HookRestore::install` guards against two `askStreaming` calls
/// overlapping, but NOT against an `ask` (non-streaming) being mid-flight
/// when `askStreaming` starts. The worker fires the streaming hook for
/// every token regardless of which API initiated the Ask, so the
/// still-running `ask`'s tokens get misrouted to the new `askStreaming`
/// callback. The install guard can't see this — `ask` doesn't install
/// a hook, so the slot looks empty. The only mitigation is the
/// docstring warning users to drain `TokenStream` first.
#[test]
fn ask_streaming_doc_warns_about_mixing_with_ask() {
    let ask_streaming_doc = doc_above(
        LIB_RS,
        "pub fn ask_streaming(&self, prompt: String, on_token:",
    );
    let mentions_ask = ask_streaming_doc.contains("`ask`") || ask_streaming_doc.contains("`ask(");
    let mentions_serialization = ask_streaming_doc.contains("drain")
        || ask_streaming_doc.contains("mix")
        || ask_streaming_doc.contains("in flight")
        || ask_streaming_doc.contains("in-flight")
        || ask_streaming_doc.contains("mid-flight")
        || ask_streaming_doc.contains("serialize");
    assert!(
        mentions_ask && mentions_serialization,
        "wasm/src/lib.rs: Chat::ask_streaming docstring must warn against \
         mixing with an in-flight `ask` call. The chat worker fires the \
         streaming hook for every token regardless of which API initiated \
         the Ask, so an unawaited `ask`'s tokens route through a later \
         `askStreaming` callback. The install-guard does NOT catch this \
         (because `ask` doesn't install a hook), so the docstring is the \
         only line of defence.\n\nCurrent docstring:\n{}",
        ask_streaming_doc
    );
}

/// Finding #S4 — `core::llm::set_streaming_hook` doc must explain why
/// the slot supports only one consumer.
///
/// The original doc told callers to "save and restore" the previous hook
/// for "nested or concurrent" asks. That's wrong for concurrent — the
/// chat worker is FIFO but save/restore is LIFO, so two overlapping
/// consumers would still misroute. The doc must instead name the
/// constraint (one consumer at a time) and explain why, so a future
/// caller doesn't reach for the same broken pattern.
#[test]
fn core_streaming_hook_doc_explains_overlap_constraint() {
    let set_hook_doc = doc_above(CORE_LLM_RS, "pub fn set_streaming_hook(hook:");
    let lower = set_hook_doc.to_lowercase();
    let names_constraint = (lower.contains("one") && lower.contains("consumer"))
        || lower.contains("overlap")
        || lower.contains("misroute");
    let explains_why = lower.contains("fifo")
        || lower.contains("lifo")
        || lower.contains("stack-shaped")
        || lower.contains("worker")
        || lower.contains("displace");
    assert!(
        names_constraint && explains_why,
        "core/src/llm.rs: set_streaming_hook docstring must name the \
         one-consumer-at-a-time constraint AND explain why (FIFO worker \
         vs LIFO save/restore). Without the rationale, a future caller \
         might reach for the same save-and-restore pattern thinking it \
         handles concurrent installs — it doesn't.\n\n\
         Current docstring:\n{}",
        set_hook_doc
    );
}

// Note: the four "disproof" tests that lived here have been removed.
// They were a one-time verification tool — each one attempted to invalidate
// a finding by looking for a mitigation in one specific place (release.yml
// instead of build-pkg.sh, a separate history element instead of a guarded
// clear, etc.). After the findings were fixed via different paths, they
// became brittle without adding regression value: the positive tests above
// already cover the substantive invariants.
