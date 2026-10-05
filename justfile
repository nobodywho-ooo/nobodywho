LIB_EXT := if os() == "macos" { "dylib" } else { "so" }

GODOT := env("GODOT", "godot")
GODOT_PROJECT := "nobodywho/godot/tests"

# `--require-clean` fails if the checked paths differ from the git index.
# `clean` must be passed to every dependency that (transitively) reaches a
# recipe taking it; a missed edge silently runs that subtree lenient.
[arg("clean", long="require-clean", value="true")]
check clean="false": _check-start (fmt clean) clippy (regen-python clean) (regen-flutter clean) (ruff clean) (regen-uniffi clean) flutter-analyze (testing-apps clean) godot-build
    @n=$(find nobodywho/target/debug/build -path '*/llama-cpp-sys-2-*/output' -newer nobodywho/target/.check-start | wc -l); \
    [ "$n" -le 1 ] || echo "⚠  llama.cpp was built $((n)) times; the cargo builds above no longer share one llama-cpp-sys-2 build."

# Marks the start of `check`, so it can tell how many llama.cpp builds ran.
_check-start:
    @mkdir -p nobodywho/target && touch nobodywho/target/.check-start

[arg("clean", long="require-clean", value="true")]
fmt clean="false":
    cd nobodywho && cargo fmt --all
    [ "{{clean}}" != true ] || git diff --exit-code -- '*.rs' || (echo "cargo fmt made changes — commit them before pushing" && exit 1)

clippy:
    cd nobodywho/core && cargo clippy --no-deps --all-targets -- -D warnings

[arg("clean", long="require-clean", value="true")]
regen-python clean="false":
    cd nobodywho/python && uv sync --no-install-project && maturin develop --uv && cargo run -p make_stubs && uv run ruff format nobodywho.pyi && uv run ty check
    [ "{{clean}}" != true ] || git diff --exit-code nobodywho/python/nobodywho.pyi || (echo "Python stubs are out of date — commit them before pushing" && exit 1)

[arg("clean", long="require-clean", value="true")]
regen-flutter clean="false":
    cd nobodywho/flutter/nobodywho && dart run tool/doctest.dart ../../../docs/docs-flutter --generate-only
    [ "{{clean}}" != true ] || git diff --exit-code nobodywho/flutter/nobodywho/test/doctest_generated_test.dart || (echo "Flutter doctests are out of date — commit them before pushing" && exit 1)

[arg("clean", long="require-clean", value="true")]
ruff clean="false":
    cd nobodywho/python && uv run ruff format && uv run ruff check
    [ "{{clean}}" != true ] || git diff --exit-code nobodywho/python/ || (echo "ruff format made changes — commit them before pushing" && exit 1)

flutter-analyze:
    cd nobodywho/flutter/nobodywho && flutter analyze lib/

# Forwards `clean`; see `check`.
testing-apps clean="false": testapp-flutter (testapp-react-native clean) testapp-kotlin

testapp-flutter:
    cd nobodywho/testing-apps/flutter && flutter analyze

# Forwards `clean`; see `check`.
testapp-react-native clean="false": (regen-uniffi clean)
    #!/usr/bin/env bash
    set -euo pipefail
    cd nobodywho/testing-apps/react-native
    # npm's hidden lockfile mirrors the installed tree; if it predates
    # package-lock.json the install is missing or stale (same guard as
    # regen-uniffi).
    [ node_modules/.package-lock.json -nt package-lock.json ] || npm ci
    npm run typecheck

testapp-kotlin:
    #!/usr/bin/env bash
    set -euo pipefail
    if [[ -z "${ANDROID_HOME:-}${ANDROID_SDK_ROOT:-}" ]]; then
        echo "⏭  skipping Kotlin test app: no Android SDK in this shell."
        echo "   Run \`just testapp-kotlin\` under \`nix develop .#android\` to include it."
        exit 0
    fi
    cd nobodywho/testing-apps/kotlin
    ./gradlew --console=plain compileDebugKotlin compileDebugAndroidTestKotlin

godot-build:
    cd nobodywho && cargo build -p nobodywho-godot

# Run the Godot integration-test project against a freshly built extension.
godot-test: godot-build
    #!/usr/bin/env bash
    set -euo pipefail
    # Extensions load only from .godot/extension_list.cfg, which the editor
    # writes on first import. That import segfaults during editor-docs
    # generation at cleanup, after having done its work — hence `|| true`,
    # with an explicit check that it actually produced the file.
    if [[ ! -f "{{GODOT_PROJECT}}/.godot/extension_list.cfg" ]]; then
        echo "⏳ first run: importing project to register the gdextension..."
        {{GODOT}} --headless --path "{{GODOT_PROJECT}}" --import >/dev/null 2>&1 || true
        if [[ ! -f "{{GODOT_PROJECT}}/.godot/extension_list.cfg" ]]; then
            echo "❌ import did not register the gdextension"
            exit 1
        fi
    fi
    {{GODOT}} --headless --path "{{GODOT_PROJECT}}"

[arg("clean", long="require-clean", value="true")]
regen-uniffi clean="false":
    cd nobodywho && cargo build -p nobodywho-uniffi --locked
    cd nobodywho && target/debug/uniffi-bindgen generate --library target/debug/libnobodywho_uniffi.{{LIB_EXT}} --language swift --out-dir swift/generated
    cd nobodywho && target/debug/uniffi-bindgen generate --library target/debug/libnobodywho_uniffi.{{LIB_EXT}} --language kotlin --out-dir kotlin/common/generated
    # npm's hidden lockfile mirrors the installed tree; if it predates
    # package-lock.json the install is missing or stale. Without this,
    # npx below silently fetches an unpinned uniffi-bindgen-react-native.
    cd nobodywho/react-native && { [ node_modules/.package-lock.json -nt package-lock.json ] || npm ci; }
    cd nobodywho && npx --prefix react-native uniffi-bindgen-react-native generate jsi bindings --library --ts-dir react-native/generated/ts --cpp-dir react-native/generated/cpp $(pwd)/target/debug/libnobodywho_uniffi.{{LIB_EXT}}
    [ "{{clean}}" != true ] || git diff --exit-code nobodywho/swift/generated/ nobodywho/kotlin/common/generated/ nobodywho/react-native/generated/ || (echo "Uniffi bindings are out of date — commit them before pushing" && exit 1)
