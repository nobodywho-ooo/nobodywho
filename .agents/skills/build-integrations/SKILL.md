---
name: build-integrations
description: Wire up a finished core Rust feature across all nobodywho language bindings, regenerate all generated files, and verify everything passes CI checks. Use when the user asks to build, propagate, or integrate a feature (e.g. TTS, STT, Encoder) across Python, Godot, Flutter, Swift, Kotlin, or React Native.
compatibility: Designed for Claude Code. Requires cargo, nix, uv, and npx on PATH.
---

Wire up a finished core Rust feature across all nobodywho language bindings, regenerate all generated files, and verify everything passes CI checks.

The feature name is taken from the user's invocation message (e.g. "TTS", "STT"). It should match a module name in `nobodywho/core/src/`.

---

## Step 1 — Discover the feature surface

Run `ls nobodywho/core/src` to list available modules and identify the one matching the requested feature. Read its public API (structs, methods, signatures) from the module's source.

Then read `nobodywho/uniffi/src/lib.rs` to identify what is already bridged and what is missing.

Find the closest existing analog that is fully integrated across bindings (e.g. `RustSTT` for a streaming API, `RustEncoder` for a simple sync API) — use it as the implementation template throughout.

---

## Step 2 — Ask clarifying questions

Use `AskUserQuestion` to ask all three at once before writing any code:

1. **API shape** — Should the public API in each language match the Rust API directly (same method names, sync/async as-is), or adapt to that language's idioms (e.g. Kotlin coroutines, Swift async/await, JS Promises)?
2. **Test scope** — What level of testing is wanted: doc-tests / examples only, unit tests, or integration tests that require a real model file?
3. **Documentation** — Which docs pages need updating? (The `docs/` directory has per-binding subdirectories: `docs-python/`, `docs-godot/`, `docs-flutter/`, `docs-kotlin/`, `docs-swift/`.) Are there any bindings where docs are not needed?

---

## Step 3 — Implement across bindings

Work in this order. Each step compiles before moving to the next.

### 3a. UniFFI bridge → Kotlin / Swift / React Native

Add a `Rust<Feature>` struct to `nobodywho/uniffi/src/lib.rs`. Mirror the pattern of `RustSTT` (around line 546): use `Arc<Self>`, `#[uniffi::export]`, wrap core errors with `map_err(|e| e.to_string())`.

Then build and run codegen (from `nobodywho/` — the inner directory containing `Cargo.toml`):

```bash
cargo build -p nobodywho-uniffi

# Detect library extension (macOS vs Linux)
LIBEXT=$([ "$(uname)" = "Darwin" ] && echo "dylib" || echo "so")
LIBPATH="target/debug/libnobodywho_uniffi.$LIBEXT"

# Swift
./target/debug/uniffi-bindgen generate \
  --library "$LIBPATH" --language swift --out-dir swift/generated

# Kotlin
./target/debug/uniffi-bindgen generate \
  --library "$LIBPATH" --language kotlin --out-dir kotlin/common/generated

# React Native
npx --prefix react-native uniffi-bindgen-react-native generate jsi bindings \
  --library \
  --ts-dir react-native/generated/ts \
  --cpp-dir react-native/generated/cpp \
  "$LIBPATH"
```

### 3b. React Native wrapper

Add a TypeScript wrapper class in `react-native/src/<feature-lowercase>.ts` following the pattern of `react-native/src/stt.ts`. Export it from `react-native/src/wrapper.ts`.

### 3c. Godot

Add a `#[derive(GodotClass)]` struct to `nobodywho/godot/src/lib.rs` following the `NobodyWhoChat` / `NobodyWhoEncoder` pattern. Godot uses GDExtension (not UniFFI) — no codegen step needed, but the struct must compile.

**No Tokio inside `godot::task::spawn`.** `godot::task::spawn` runs on gdext's own async executor — it does NOT provide a Tokio runtime. These Tokio APIs will panic at runtime:

- `tokio::task::spawn_blocking` — needs the Tokio blocking thread pool
- `tokio::runtime::Handle::current()` — no runtime handle exists

To run blocking work from a `godot::task::spawn` closure, use `std::thread::spawn` and pass the result back via a `tokio::sync::oneshot` channel (oneshot uses standard Rust wakers and works with any executor):

```rust
godot::task::spawn(async move {
    let (tx, rx) = tokio::sync::oneshot::channel();
    std::thread::spawn(move || {
        let _ = tx.send(my_blocking_fn());
    });
    match rx.await {
        Ok(result) => { /* handle result */ }
        Err(_) => { /* thread panicked */ }
    }
});
```

`tokio::sync::mpsc` and `tokio::sync::oneshot` channels are fine to *use* (create, send, await) inside `godot::task::spawn` — they are just data structures backed by standard Rust wakers.

**Godot integration test lifecycle.** Every Godot inference node requires `start_worker()` followed by `await <node>.worker_started` before calling any inference method (`transcribe_file`, `ask`, etc.). Skipping this causes an "STT/worker not started" error at runtime. See `grammar_test.gd` and `hf_path_test.gd` for the canonical pattern:

```gdscript
node.start_worker()
await node.worker_started
node.some_inference_call(...)
```

### 3d. Python

Add a `#[pyclass]` struct to `nobodywho/python/src/lib.rs` following existing PyO3 patterns. Then regenerate stubs (from `nobodywho/python/`):

```bash
cargo run --bin make_stubs
uv run ruff format nobodywho.pyi
```

`make_stubs` uses compile-time introspection — it does not require the Python dylib to link successfully. Do NOT gate it on `cargo build` succeeding.

### 3e. Flutter

Add the feature to `nobodywho/flutter/rust/src/lib.rs` following existing FRB patterns. FRB codegen runs automatically via `build.rs` when you build — from `nobodywho/flutter/rust/`:

```bash
NOBODYWHO_SKIP_CODEGEN=1 cargo build
# Skip codegen if flutter toolchain is not installed locally; CI handles the regen check.
```

### 3f. Regenerate Cargo.nix

**Always run this step** if any `Cargo.toml` or `Cargo.lock` changed (new deps added, features changed). Forgetting this is what breaks the Nix CI build with "unresolved crate" errors. Run from `nobodywho/`:

```bash
nix run github:nix-community/crate2nix -- generate -h crate-hashes.json
```

If `nix` is not on PATH, try `/nix/var/nix/profiles/default/bin/nix --extra-experimental-features 'nix-command flakes' run github:nix-community/crate2nix -- generate -h crate-hashes.json`.

This regenerates both `Cargo.nix` and updates `crate-hashes.json`. Both files must be committed.

### 3g. Nix test asset paths

If the feature requires a test asset (audio file, image, etc.), expose it via an env var in each binding's `default.nix`. Two rules:

1. **String-interpolate the path.** A bare Nix path value (`env.X = ../../assets/file`) is of type `path` and causes an attribute type error. Always wrap it: `env.X = "${../../assets/file}";`

2. **Count `../` from the `default.nix` file, not the repo root.** The depth varies by binding:
   - `nobodywho/python/default.nix` → `"${../../assets/file}"` (2 levels up to repo root)
   - `nobodywho/flutter/nobodywho/default.nix` → `"${../../../assets/file}"` (3 levels up)

**Local Nix blind spot.** `nix flake check` on macOS only evaluates `aarch64-darwin` derivations. All `x86_64-linux` checks — including Flutter tests and the Linux Rust test suite — are invisible locally and only run in CI. If a Linux-only failure needs investigation, it must be pushed and observed in CI.

---

## Step 4 — Verify

All of these must pass before reporting done. Fix any failures before moving on.

**Check all new module files are declared.** After adding files to a core submodule directory, verify a `mod <name>;` line exists in the parent `mod.rs`. Rust silently ignores files that are not declared — they compile fine but are unreachable dead code and will never be tested or exported.

```bash
# From nobodywho/
cargo fmt --all --check

# From nobodywho/core/
cargo clippy --no-deps -- -D warnings

# From nobodywho/python/
uv run ruff format --check
uv run ruff check
```

If tests were requested in Step 2, run them now:
- Core: `cargo test -- --nocapture --test-threads=1` (requires `TEST_MODEL` env var pointing to a `.gguf` file)
- Python: `cd nobodywho/python && pytest`

---

## Step 5 — Report

Summarize:
- Which bindings were added (and which were skipped, if any)
- Which generated files changed (list them)
- Confirmation that all checks in Step 4 passed
