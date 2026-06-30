# Build Integrations

Wire up a finished core Rust feature across all nobodywho language bindings, regenerate all generated files, and verify everything passes CI checks.

**Usage:** `/build-integrations <feature>` — e.g. `/build-integrations TTS` or `/build-integrations STT`

The feature name (`$ARGUMENTS`) should match the module name in `nobodywho/core/src/lib.rs`.

---

## Step 1 — Discover the feature surface

Read `nobodywho/core/src/lib.rs` to find the module for `$ARGUMENTS`. Read its public API (structs, methods, signatures) from the module's source.

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

---

## Step 4 — Verify

All of these must pass before reporting done. Fix any failures before moving on.

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
