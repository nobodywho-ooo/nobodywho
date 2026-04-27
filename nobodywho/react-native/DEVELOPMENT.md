# nobodywho — React Native bindings (UniFFI)

React Native TurboModule for running LLMs locally on iOS and Android (via Vulkan/Metal GPU acceleration). Uses [UniFFI](https://mozilla.github.io/uniffi-rs/) to generate C++ JSI bindings from Rust, with [uniffi-bindgen-react-native](https://github.com/aspect-build/aspect-build) as the code generator.

## Prerequisites

- Rust toolchain (stable) with Android targets: `rustup target add aarch64-linux-android x86_64-linux-android`
- Node.js 22+
- For Android: Nix (`nix develop .#android` provides NDK r28, cmake, JDK 17, and all cross-compilation env vars)
- For iOS: Xcode, CocoaPods

## Project structure

```
react-native/
├── ubrn.config.yaml             # uniffi-bindgen-react-native config
├── package.json                 # npm package metadata
├── jest.config.js               # Jest test configuration
├── Nobodywho.podspec            # CocoaPods spec (iOS) — customized, do not regenerate
│
├── src/                         # Hand-written TypeScript wrappers
│   ├── wrapper.ts               # Public entry point (re-exports public API)
│   ├── chat.ts                  # Chat wrapper (fromPath, destroy, etc.)
│   ├── model.ts                 # Model wrapper (Model.load factory)
│   ├── encoder.ts               # Encoder wrapper (fromPath, destroy)
│   ├── cross_encoder.ts         # CrossEncoder wrapper (fromPath, destroy, rankAndSort)
│   ├── streaming.ts             # TokenStream with AsyncIterable support
│   ├── tool.ts                  # Tool with declarative parameter API
│   ├── prompt.ts                # Prompt with Text/Image/Audio factories
│   ├── message.ts               # Message type + internal conversion
│   ├── sampler_presets.ts       # SamplerPresets static class
│   ├── index.tsx                # Native init + generated re-exports (generated, do not edit)
│   └── NativeNobodywho.ts      # TurboModule spec (generated, do not edit)
│
├── __tests__/                   # Jest tests (pure TS, no native deps)
│   └── convertValue.test.ts     # Tests for tool parameter type conversion
│
├── generated/                   # Generated bindings (committed, regenerate when Rust API changes)
│   ├── ts/
│   │   ├── nobodywho.ts         # TypeScript bindings
│   │   └── nobodywho-ffi.ts     # Low-level FFI types
│   └── cpp/
│       ├── nobodywho.cpp        # C++ JSI bridge
│       └── nobodywho.hpp        # C++ header
│
├── cpp/                         # TurboModule C++ glue (generated once, rarely changes)
│   ├── react-native-nobodywho.cpp
│   ├── react-native-nobodywho.h
│   ├── nobodywho-react-native.cpp
│   └── nobodywho-react-native.h
│
├── ios/                         # iOS native module (generated once, rarely changes)
│   ├── Nobodywho.h
│   └── Nobodywho.mm
│
├── android/                     # Android native module
│   ├── build.gradle             # Customized — downloads .so from GitHub Releases
│   ├── CMakeLists.txt           # Customized — links shared lib + uniffi headers
│   ├── cpp-adapter.cpp          # Generated glue
│   └── src/main/
│       ├── AndroidManifest.xml
│       ├── AndroidManifestNew.xml
│       └── java/ooo/nobodywho/
│           ├── NobodywhoModule.kt
│           └── NobodywhoPackage.kt
│
└── test-app/                    # Minimal React Native app for testing
    ├── App.tsx
    └── android/
```

## When to regenerate what

There are three layers of generated code. Each layer only needs regeneration for specific changes:

### 1. Bindings (`generated/ts/`, `generated/cpp/`)

**Regenerate when:** Rust API changes — adding/removing/renaming functions, types, errors, or changing their signatures in `uniffi/src/lib.rs`.

```bash
# From nobodywho/ (workspace root)
cargo build -p nobodywho-uniffi
npx --prefix react-native uniffi-bindgen-react-native generate jsi bindings \
  --library --ts-dir react-native/generated/ts --cpp-dir react-native/generated/cpp \
  target/debug/libnobodywho_uniffi.so
```

**Do not regenerate for:** TypeScript wrapper changes, build config changes, version bumps.

### 2. TurboModule glue (`ios/`, `cpp/`, `src/NativeNobodywho.ts`, `src/index.tsx`)

**Regenerate when:** Module name changes, `codegenConfig` in `package.json` changes, or upgrading `uniffi-bindgen-react-native` version.

```bash
cd react-native
npx uniffi-bindgen-react-native generate jsi turbo-module --config ubrn.config.yaml nobodywho
```

**WARNING:** This overwrites `Nobodywho.podspec`, `android/build.gradle`, and `android/CMakeLists.txt` with defaults, destroying custom build logic (binary download, xcframework support, etc.). After running, restore these files:

```bash
git checkout -- Nobodywho.podspec android/build.gradle android/CMakeLists.txt
```

**Do not regenerate for:** Rust API changes, adding new functions/types — those only affect the bindings layer above.

### 3. TypeScript wrappers (`src/*.ts` except `NativeNobodywho.ts` and `index.tsx`)

**Never regenerated** — these are hand-written. Edit freely. Metro hot-reloads changes automatically.

### Quick reference

| What changed | Regenerate bindings | Regenerate turbo-module | Rebuild native libs |
|---|---|---|---|
| Rust API (`uniffi/src/lib.rs`) | Yes | No | Yes |
| Core Rust library (`core/src/`) | No | No | Yes |
| TypeScript wrappers (`src/*.ts`) | No | No | No |
| Module name / `codegenConfig` | No | Yes (then restore build files) | No |
| `uniffi-bindgen-react-native` version | Yes | Yes (then restore build files) | No |

## Build system overview

### Generate bindings from Rust

Build the UniFFI crate for the host, then run the bindgen to produce TypeScript + C++:

```bash
# From nobodywho/ (workspace root)
cargo build -p nobodywho-uniffi

npx --prefix react-native uniffi-bindgen-react-native generate jsi bindings \
  --library \
  --ts-dir react-native/generated/ts \
  --cpp-dir react-native/generated/cpp \
  target/debug/libnobodywho_uniffi.so
```

This reads the UniFFI metadata embedded in the compiled `.so`/`.dylib` and generates:
- `generated/ts/nobodywho.ts` — TypeScript classes, enums, free functions
- `generated/ts/nobodywho-ffi.ts` — low-level FFI type bridge
- `generated/cpp/nobodywho.{cpp,hpp}` — C++ JSI bridge implementation

### Build native shared libraries for mobile targets

The Android build expects shared libraries (`.so` files). These are prebuilt at CI time and downloaded by Gradle, so the consumer's NDK version does not affect the Rust code.

For local development, use the nix android shell:

```bash
# From project root (where flake.nix is)

# Android ARM64 (physical devices)
nix develop .#android --command bash -c \
  'cd nobodywho && cargo build -p nobodywho-uniffi --target aarch64-linux-android --release'

# Android x86_64 (emulator)
nix develop .#android --command bash -c \
  'cd nobodywho && cargo build -p nobodywho-uniffi --target x86_64-linux-android --release'
```

Then copy the `.so` files to where the Android build expects them:

```bash
mkdir -p nobodywho/react-native/android/build/nobodywho-native/{arm64-v8a,x86_64}

# ARM64
cp nobodywho/target/aarch64-linux-android/release/libnobodywho_uniffi.so \
  nobodywho/react-native/android/build/nobodywho-native/arm64-v8a/

# x86_64
cp nobodywho/target/x86_64-linux-android/release/libnobodywho_uniffi.so \
  nobodywho/react-native/android/build/nobodywho-native/x86_64/
```

For iOS:
```bash
cargo build -p nobodywho-uniffi --target aarch64-apple-ios --release
cargo build -p nobodywho-uniffi --target aarch64-apple-ios-sim --release
```

### Release builds (CI)

In CI, native `.so` files are cross-compiled and uploaded as GitHub Release assets. At install time:
- **Android:** `build.gradle` downloads `.so` files from the GitHub Release matching the package version
- **iOS:** `Nobodywho.podspec` downloads and extracts `NobodywhoFramework.xcframework.zip` from the same release

This keeps the npm package small (code only, no binaries).

## Running tests

### Jest tests (pure TypeScript)

```bash
cd nobodywho/react-native
npm test
```

These tests run without native code — they test pure TypeScript functions like `convertValue`. They are also run as a nix flake check (`nix build .#checks.x86_64-linux.react-native-jest`).

### Testing on Android

#### Build the test app

```bash
# From project root
nix develop .#android --command bash -c \
  'cd nobodywho/react-native/test-app/android && \
   ./gradlew assembleDebug -PreactNativeArchitectures=arm64-v8a'
```

#### Run on a connected device

Start Metro first, then install and launch:

```bash
# Terminal 1: start Metro bundler (must be running before the app launches)
cd nobodywho/react-native/test-app
npx react-native start --port 8081

# Terminal 2: install, set up port forwarding, and launch
adb install -r nobodywho/react-native/test-app/android/app/build/outputs/apk/debug/app-debug.apk
adb reverse tcp:8081 tcp:8081
adb shell am start -n com.nobodywhotest/.MainActivity
```

## Customized files (do not regenerate)

These files were initially generated but have been customized with project-specific logic:

- **`Nobodywho.podspec`** — Downloads prebuilt xcframework from GitHub Releases, custom authors/source fields
- **`android/build.gradle`** — Downloads prebuilt `.so` files from GitHub Releases at build time, optional NDK version
- **`android/CMakeLists.txt`** — Links shared lib with `IMPORTED_NO_SONAME` for correct runtime resolution
- **`android/src/main/java/ooo/nobodywho/NobodywhoModule.kt`** — Loads `libnobodywho_uniffi.so` before the bridge lib
- **`android/src/main/java/ooo/nobodywho/NobodywhoPackage.kt`** — Uses `BaseReactPackage` instead of deprecated `TurboReactPackage`
- **`android/cpp-adapter.cpp`** — JNI symbols use `ooo_nobodywho` package path (generator defaults to `com_nobodywho`)

If you regenerate the turbo-module glue, these get overwritten with defaults. Always restore them with `git checkout`.

## Known issues

- **uniffi-bindgen-react-native `async static` bug:** Async constructors generate invalid JS
  (`async static` instead of `static async`). Workaround: use free functions instead of async
  constructors in the Rust UniFFI crate. This is why `Model` uses `loadModel()` instead of
  `Model.load()`.
- **Async tool callbacks:** JavaScript cannot synchronously await a Promise, so tool callbacks
  must currently be synchronous. Async support is planned via a channel-based architecture.
