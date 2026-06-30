LIB_EXT := if os() == "macos" { "dylib" } else { "so" }

check: fmt clippy regen-python regen-flutter ruff regen-uniffi flutter-analyze godot-build

fmt:
    cd nobodywho && cargo fmt --all
    git diff --exit-code -- '*.rs' || (echo "cargo fmt made changes — commit them before pushing" && exit 1)

clippy:
    cd nobodywho/core && cargo clippy --no-deps -- -D warnings

regen-python:
    cd nobodywho/python && maturin develop --uv && cargo run --bin make_stubs && uv run ruff format nobodywho.pyi && uv run ty check
    git diff --exit-code nobodywho/python/nobodywho.pyi || (echo "Python stubs are out of date — commit them before pushing" && exit 1)

regen-flutter:
    cd nobodywho/flutter/nobodywho && dart run tool/doctest.dart ../../../docs/docs-flutter --generate-only
    git diff --exit-code nobodywho/flutter/nobodywho/test/doctest_generated_test.dart || (echo "Flutter doctests are out of date — commit them before pushing" && exit 1)

ruff:
    cd nobodywho/python && uv run ruff format && uv run ruff check
    git diff --exit-code nobodywho/python/ || (echo "ruff format made changes — commit them before pushing" && exit 1)

flutter-analyze:
    cd nobodywho/flutter/nobodywho && flutter analyze lib/

godot-build:
    cd nobodywho && cargo build -p nobodywho-godot

regen-uniffi:
    cd nobodywho && cargo build -p nobodywho-uniffi --locked
    cd nobodywho && target/debug/uniffi-bindgen generate --library target/debug/libnobodywho_uniffi.{{LIB_EXT}} --language swift --out-dir swift/generated
    cd nobodywho && target/debug/uniffi-bindgen generate --library target/debug/libnobodywho_uniffi.{{LIB_EXT}} --language kotlin --out-dir kotlin/common/generated
    cd nobodywho && npx --prefix react-native uniffi-bindgen-react-native generate jsi bindings --library --ts-dir react-native/generated/ts --cpp-dir react-native/generated/cpp $(pwd)/target/debug/libnobodywho_uniffi.{{LIB_EXT}}
    git diff --exit-code nobodywho/swift/generated/ nobodywho/kotlin/common/generated/ nobodywho/react-native/generated/ || (echo "Uniffi bindings are out of date — commit them before pushing" && exit 1)
