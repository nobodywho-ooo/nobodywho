#!/usr/bin/env bash
# Build the publishable npm package for nobodywho-wasm.
#
# Produces nobodywho/wasm/pkg-bundler/ with:
#   nobodywho_wasm.js          — entrypoint
#   nobodywho_wasm_bg.js       — Chat/Model/Encoder classes + wasm-bindgen glue
#   nobodywho_wasm_bg.wasm     — compiled wasm (release-stripped)
#   nobodywho_wasm.d.ts        — TS typings
#   nobodywho_wasm_bg.wasm.d.ts
#   package.json               — copied from package.json.tpl, version-bumped
#   README.md
#
# Prereqs:
#   - $WASI_SDK_PATH points at wasi-sdk-XX (see ../README.md)
#   - rustup target add wasm32-unknown-unknown
#   - cargo install wasm-bindgen-cli --version 0.2.121
set -euo pipefail

if [[ -z "${WASI_SDK_PATH:-}" ]]; then
  echo "error: WASI_SDK_PATH not set. Install wasi-sdk and point this env var at it." >&2
  echo "       Releases: https://github.com/WebAssembly/wasi-sdk/releases" >&2
  exit 1
fi
if [[ ! -x "$WASI_SDK_PATH/bin/clang" ]]; then
  echo "error: \$WASI_SDK_PATH=$WASI_SDK_PATH but bin/clang not found there." >&2
  exit 1
fi

cd "$(dirname "$0")/.."   # nobodywho/wasm
WASM_DIR="$(pwd)"
WORKSPACE_ROOT="$(cd ../.. && pwd)"

PROFILE="${PROFILE:-release}"
echo "==> Building wasm32-unknown-unknown ($PROFILE)…"
(
  cd "$WORKSPACE_ROOT/nobodywho"
  case "$PROFILE" in
    release) "$(command -v cargo || echo ~/.cargo/bin/cargo)" build --target wasm32-unknown-unknown --release -p nobodywho-wasm ;;
    debug)   "$(command -v cargo || echo ~/.cargo/bin/cargo)" build --target wasm32-unknown-unknown          -p nobodywho-wasm ;;
    *)       echo "error: PROFILE must be release or debug, got $PROFILE" >&2; exit 1 ;;
  esac
)

WASM_PATH="$WORKSPACE_ROOT/nobodywho/target/wasm32-unknown-unknown/$PROFILE/nobodywho_wasm.wasm"
ls -lh "$WASM_PATH"

echo "==> Running wasm-bindgen…"
rm -rf "$WASM_DIR/pkg-bundler"
"$(command -v wasm-bindgen || echo ~/.cargo/bin/wasm-bindgen)" \
  --target bundler "$WASM_PATH" --out-dir "$WASM_DIR/pkg-bundler/"

echo "==> Copying package.json + README…"
cp "$WASM_DIR/package.json.tpl" "$WASM_DIR/pkg-bundler/package.json"
# Could `npm version` here; keep the template as the source of truth.
cp "$WASM_DIR/README.md" "$WASM_DIR/pkg-bundler/README.md"

echo "==> Done."
ls -lh "$WASM_DIR/pkg-bundler/"
echo ""

# `bash build-pkg.sh --link` runs `npm link` inside pkg-bundler/ so the
# package becomes available to downstream projects via `npm link @nobodywho/wasm`
# without going through a real npm publish. Mirrors maturin's `develop`
# command — the Python binding equivalent.
if [[ "${1:-}" == "--link" ]]; then
  if ! command -v npm >/dev/null 2>&1; then
    echo "error: 'npm' not on PATH. Install Node.js to use --link." >&2
    exit 1
  fi
  echo "==> npm link…"
  ( cd "$WASM_DIR/pkg-bundler" && npm link )
  echo ""
  echo "In a consumer project:  npm link @nobodywho/wasm"
  echo ""
fi

echo "To smoke-test:"
echo "  node $WASM_DIR/examples/run.mjs --encode /path/to/embedding.gguf 'text'"
echo "  node $WASM_DIR/examples/run.mjs /path/to/chat.gguf 'prompt'"
echo ""
echo "To publish (manually):"
echo "  cd $WASM_DIR/pkg-bundler && npm publish --access public"
