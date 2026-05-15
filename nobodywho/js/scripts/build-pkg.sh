#!/usr/bin/env bash
# Build the publishable npm package for nobodywho-js.
#
# Produces nobodywho/js/pkg-bundler/ with:
#   nobodywho_js.js          — entrypoint
#   nobodywho_js_bg.js       — Chat/Model/Encoder classes + wasm-bindgen glue
#   nobodywho_js_bg.wasm     — compiled wasm (release-stripped)
#   nobodywho_js.d.ts        — TS typings
#   nobodywho_js_bg.wasm.d.ts
#   package.json               — from package.json.tpl with version rewritten
#                                from $JS_PKG_VERSION or $GITHUB_REF_NAME
#   README.md
#
# Prereqs:
#   - $WASI_SDK_PATH points at wasi-sdk-XX (see ../README.md)
#   - rustup target add wasm32-unknown-unknown
#   - cargo install wasm-bindgen-cli --version "$(bash wasm-bindgen-version.sh)"
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

cd "$(dirname "$0")/.."   # nobodywho/js
JS_DIR="$(pwd)"
WORKSPACE_ROOT="$(cd ../.. && pwd)"

# wasm-bindgen-cli must match the wasm-bindgen crate version in Cargo.lock
# exactly. If the installed CLI is different, surface that before doing a
# long C++ build that would just produce an unloadable .wasm.
EXPECTED_WBG=$(bash "$JS_DIR/scripts/wasm-bindgen-version.sh")
if ACTUAL_WBG_LINE=$("$(command -v wasm-bindgen || echo ~/.cargo/bin/wasm-bindgen)" --version 2>/dev/null); then
  ACTUAL_WBG=${ACTUAL_WBG_LINE##* }
  if [[ "$ACTUAL_WBG" != "$EXPECTED_WBG" ]]; then
    echo "error: wasm-bindgen-cli version mismatch." >&2
    echo "       installed: $ACTUAL_WBG" >&2
    echo "       expected:  $EXPECTED_WBG  (from $WORKSPACE_ROOT/nobodywho/Cargo.lock)" >&2
    echo "       fix:       cargo install wasm-bindgen-cli --version $EXPECTED_WBG --locked --force" >&2
    exit 1
  fi
else
  echo "error: wasm-bindgen not found on PATH. Install with:" >&2
  echo "       cargo install wasm-bindgen-cli --version $EXPECTED_WBG --locked" >&2
  exit 1
fi

PROFILE="${PROFILE:-release}"
echo "==> Building wasm32-unknown-unknown ($PROFILE)…"
(
  cd "$WORKSPACE_ROOT/nobodywho"
  case "$PROFILE" in
    release) "$(command -v cargo || echo ~/.cargo/bin/cargo)" build --target wasm32-unknown-unknown --release -p nobodywho-js ;;
    debug)   "$(command -v cargo || echo ~/.cargo/bin/cargo)" build --target wasm32-unknown-unknown          -p nobodywho-js ;;
    *)       echo "error: PROFILE must be release or debug, got $PROFILE" >&2; exit 1 ;;
  esac
)

WASM_PATH="$WORKSPACE_ROOT/nobodywho/target/wasm32-unknown-unknown/$PROFILE/nobodywho_js.wasm"
ls -lh "$WASM_PATH"

echo "==> Running wasm-bindgen…"
rm -rf "$JS_DIR/pkg-bundler"
"$(command -v wasm-bindgen || echo ~/.cargo/bin/wasm-bindgen)" \
  --target bundler "$WASM_PATH" --out-dir "$JS_DIR/pkg-bundler/"

echo "==> Copying package.json + README…"
cp "$JS_DIR/package.json.tpl" "$JS_DIR/pkg-bundler/package.json"
cp "$JS_DIR/README.md" "$JS_DIR/pkg-bundler/README.md"

# Rewrite the version in the bundled package.json. The template ships with
# "0.0.0-PLACEHOLDER" so a forgotten substitution fails npm publish loudly
# instead of silently re-publishing some old release version.
#
# Resolution order (first match wins):
#   1. $JS_PKG_VERSION — explicit override for local/manual builds
#   2. $GITHUB_REF_NAME — the release CI sets this to the tag name
#      (e.g. "nobodywho-js-v0.1.0"); we strip the "nobodywho-js-v" prefix
#   3. nothing — leave the placeholder, useful for `npm link` workflows
VERSION=""
if [[ -n "${JS_PKG_VERSION:-}" ]]; then
  VERSION="$JS_PKG_VERSION"
elif [[ -n "${GITHUB_REF_NAME:-}" && "$GITHUB_REF_NAME" == nobodywho-js-v* ]]; then
  VERSION="${GITHUB_REF_NAME#nobodywho-js-v}"
fi

if [[ -n "$VERSION" ]]; then
  # Validate $VERSION before letting it near sed. Characters like `&` are
  # special to sed's replacement syntax (matched-substring expansion) and
  # would silently corrupt the JSON; `/` would crash sed entirely. Restrict
  # to a semver shape — digits + dots + optional `-suffix` of [A-Za-z0-9.-].
  if [[ ! "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[A-Za-z0-9.-]+)?$ ]]; then
    echo "error: VERSION '$VERSION' doesn't look like semver (X.Y.Z or X.Y.Z-suffix)." >&2
    echo "       source: ${JS_PKG_VERSION:+JS_PKG_VERSION}${GITHUB_REF_NAME:+GITHUB_REF_NAME=$GITHUB_REF_NAME}" >&2
    echo "       refusing to interpolate it into sed; fix the tag/env then re-run." >&2
    exit 1
  fi
  echo "==> Setting package.json version to $VERSION"
  # `-i.bak` is the portable form that works on both GNU and BSD sed.
  sed -i.bak "s/\"version\": \"[^\"]*\"/\"version\": \"$VERSION\"/" \
    "$JS_DIR/pkg-bundler/package.json"
  rm -f "$JS_DIR/pkg-bundler/package.json.bak"
elif [[ "${JS_ALLOW_PLACEHOLDER:-0}" == "1" ]]; then
  # Opt-in for `npm link` workflows where the version doesn't matter.
  echo "==> JS_ALLOW_PLACEHOLDER=1 — leaving '0.0.0-PLACEHOLDER' as the version."
  echo "    Do NOT \`npm publish\` this build."
else
  # Fail closed: '0.0.0-PLACEHOLDER' is valid semver (pre-release identifier),
  # so npm publish would happily accept it. Don't let that happen by accident.
  echo "error: no version provided." >&2
  echo "       Set JS_PKG_VERSION='X.Y.Z' to set one explicitly, or run" >&2
  echo "       in CI under a 'refs/tags/nobodywho-js-v*' tag." >&2
  echo "       For \`npm link\` workflows that don't care about the version," >&2
  echo "       set JS_ALLOW_PLACEHOLDER=1 to skip this check." >&2
  exit 1
fi

echo "==> Done."
ls -lh "$JS_DIR/pkg-bundler/"
echo ""

# `bash build-pkg.sh --link` runs `npm link` inside pkg-bundler/ so the
# package becomes available to downstream projects via `npm link @nobodywho/js`
# without going through a real npm publish. Mirrors maturin's `develop`
# command — the Python binding equivalent.
if [[ "${1:-}" == "--link" ]]; then
  if ! command -v npm >/dev/null 2>&1; then
    echo "error: 'npm' not on PATH. Install Node.js to use --link." >&2
    exit 1
  fi
  echo "==> npm link…"
  ( cd "$JS_DIR/pkg-bundler" && npm link )
  echo ""
  echo "In a consumer project:  npm link @nobodywho/js"
  echo ""
fi

echo "To smoke-test:"
echo "  node $JS_DIR/examples/run.mjs --encode /path/to/embedding.gguf 'text'"
echo "  node $JS_DIR/examples/run.mjs /path/to/chat.gguf 'prompt'"
echo ""
echo "To publish (manually):"
echo "  cd $JS_DIR/pkg-bundler && npm publish --access public"
