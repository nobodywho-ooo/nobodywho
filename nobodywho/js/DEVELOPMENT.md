# nobodywho JavaScript bindings (UniFFI)

An amalgamation of:
- A React Native TurboModule for running LLMs locally on iOS and Android (via Vulkan/Metal GPU acceleration).
- A WASM binary for running in browsers with WebGPU.
- In the future, N-API bindings (currently Node.js uses the WASM build).

Which one you get is decided by the `exports` conditions in `package.json`:

| Condition               | Entry point                 | Bindings                                |
|-------------------------|-----------------------------|-----------------------------------------|
| `browser`               | `dist/index.mjs` (built)    | `src/generated/wasm/`                   |
| `react-native`          | `src/index.react-native.ts` | `src/generated/jsi/` + `cpp/generated/` |
| (default, e.g. Node.js) | `dist/index.mjs` (built)    | `src/generated/wasm/`                   |

`browser` is listed before `react-native`, so React Native Web / Expo web gets the WASM build (TODO: Why does it work this way?).

The hand-written wrappers in `src/` are shared between the two, and import the generated bindings through `./generated`. That resolves to
`src/generated/index.native.ts` (JSI) under Metro on iOS/Android via [platform-specific extensions](https://reactnative.dev/docs/platform-specific-code#native-specific-extensions-ie-sharing-code-with-nodejs-and-web),
and to `src/generated/index.ts` (WASM) everywhere else. Conditional `imports` in `package.json` would be the more standard way of doing this,
but `@react-native/typescript-config` sets `resolvePackageJsonImports: false`, so type-checking apps that use this package would fail.

Uses [UniFFI](https://mozilla.github.io/uniffi-rs/) to generate the bindings from Rust, with
[uniffi-bindgen-react-native](https://github.com/aspect-build/aspect-build) as the code generator.

## Prerequisites

- Rust toolchain
  - With Android targets: `rustup target add aarch64-linux-android x86_64-linux-android`
  - With iOS targets: `rustup target add aarch64-apple-ios aarch64-apple-ios-sim`
  - With std sources (for Emscripten): `rustup component add rust-src`
- Node.js 22+
- For Android: Nix (`nix develop .#android` provides NDK r28, cmake, JDK 17, and all cross-compilation env vars)
- For iOS: Xcode, CocoaPods
- For WASM: The Emscripten SDK

## Project structure

```
js/
├── ubrn.config.yaml             # uniffi-bindgen-react-native config
├── package.json                 # npm package metadata
├── tsdown.config.ts             # Builds the WASM entry point into dist/
├── tsconfig.json                # Type-checking only
├── Nobodywho.podspec            # CocoaPods spec (iOS) — customized, do not regenerate
│
├── src/                         # Hand-written TypeScript wrappers
│   ├── index.react-native.ts    # React Native entry point — runs native init, re-exports ./wrapper
│   ├── index.wasm.ts            # WASM entry point — instantiates the module, re-exports ./wrapper
│   ├── wrapper/                 # Hand-written TypeScript wrappers
│   │   ├── index.ts             # Public API re-exports (and downloadModel)
│   │   ├── chat.ts              # Chat wrapper (fromPath, destroy, etc.)
│   │   ├── model.ts             # Model wrapper (Model.load factory)
│   │   ├── encoder.ts           # Encoder wrapper (fromPath, destroy)
│   │   ├── cross_encoder.ts     # CrossEncoder wrapper (fromPath, destroy, rankAndSort)
│   │   ├── streaming.ts         # TokenStream with AsyncIterable support
│   │   ├── tool.ts              # Tool with declarative parameter API
│   │   ├── prompt.ts            # Prompt with Text/Image/Audio factories
│   │   ├── message.ts           # Message type + internal conversion
│   │   ├── sampler_presets.ts   # SamplerPresets static class
│   │   ├── speech_to_text.ts, text_to_speech.ts, vad.ts
│   ├── generated/               # Generated TS bindings (committed, regenerate when Rust API changes)
│   │   ├── index.ts
│   │   └── jsi/                 # React Native bindings
│   └── NativeNobodywho.ts      # TurboModule spec (generated, do not edit)
│
├── __tests__/
│   ├── convertValue.test.ts     # Tests for tool parameter type conversion
│   ├── smoke.test.ts            # Integration test of the WASM build in Node.js
│   └── browser.test.ts          # Same, in headless Chrome (via playwright-core)
│
├── cpp/                         # TurboModule C++ glue (generated once, rarely changes)
│   ├── generated/               # Generated C++ JSI bridge (committed, regenerate when Rust API changes)
│   │   ├── nobodywho.cpp
│   │   └── nobodywho.hpp
│   ├── react-native-nobodywho.cpp
│   ├── react-native-nobodywho.h
│
├── ios/                         # iOS native module (generated once, rarely changes)
│   ├── Nobodywho.h
│   └── Nobodywho.mm
│
├── android/                     # Android native module
│   ├── build.gradle             # Customized — downloads .so from GitHub Releases into src/main/jniLibs
│   ├── CMakeLists.txt           # Generated — links src/main/jniLibs/<abi>/libnobodywho_uniffi.so
│   ├── cpp-adapter.cpp          # Generated glue
│   └── src/main/
│       ├── AndroidManifest.xml
│       ├── AndroidManifestNew.xml
│       └── java/ooo/nobodywho/
│           ├── NobodywhoModule.kt
│           └── NobodywhoPackage.kt
│
├── dist/                        # tsdown output for the WASM entry point, gitignored
│
└── test-app/                    # Minimal React Native app for testing
    ├── App.tsx
    └── android/
```

## When to regenerate what

There are three layers of generated code. Each layer only needs regeneration for specific changes:

### 1. Bindings (`src/generated/jsi/`, `cpp/generated/`)

**Regenerate when:** Rust API changes — adding/removing/renaming functions, types, errors, or changing their signatures in `uniffi/src/lib.rs`.

```bash
cd nobodywho/js
cargo build -pnobodywho-uniffi
# Linux
npm run generate-jsi-bindings -- ../target/debug/libnobodywho_uniffi.so
npm run generate-wasm-bindings -- ../target/debug/libnobodywho_uniffi.so
# macOS
npm run generate-jsi-bindings -- ../target/debug/libnobodywho_uniffi.dylib
npm run generate-wasm-bindings -- ../target/debug/libnobodywho_uniffi.dylib
```

This builds the UniFFI crate for the host (which is valid as long as we assume that there's no `cfg`-guards in our UniFFI bindings that
would make them differ on WASM), reads the UniFFI metadata embedded in the compiled `.so`/`.dylib` and generates:
- `src/generated/jsi/nobodywho.ts` — TypeScript classes, enums, free functions
- `src/generated/jsi/nobodywho-ffi.ts` — low-level FFI type bridge
- `src/generated/wasm/*` — Same as above, but for WASM
- `cpp/generated/nobodywho.{cpp,hpp}` — C++ JSI bridge implementation

**Do not regenerate for:** TypeScript wrapper changes, build config changes, version bumps.

### 2. TurboModule glue (`ios/`, `cpp/`, `src/NativeNobodywho.ts`)

**Regenerate when:** Module name changes, `codegenConfig` in `package.json` changes, or upgrading `uniffi-bindgen-react-native` version.

```bash
cd nobodywho/js
npm run generate-turbo-module
```

**Do not regenerate for:** Rust API changes, adding new functions/types — those only affect the bindings layer above.

### 3. TypeScript wrappers (`src/index.react-native.ts`, `src/index.wasm.ts` and `src/wrapper/`)

**Never regenerated** — these are hand-written. Edit freely. Metro hot-reloads changes automatically; for the WASM build, run `npm run build`.

### Quick reference

| What changed | Regenerate bindings | Regenerate turbo-module | Rebuild native libs |
|---|---|---|---|
| Rust API (`uniffi/src/lib.rs`) | Yes | No | Yes |
| Core Rust library (`core/src/`) | No | No | Yes |
| TypeScript wrappers (`src/wrapper/`) | No | No | No |
| Module name / `codegenConfig` | No | Yes| No |
| `uniffi-bindgen-react-native` version | Yes | Yes| No |

## Build system overview

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
mkdir -p nobodywho/js/android/src/main/jniLibs/{arm64-v8a,x86_64}

# ARM64
cp nobodywho/target/aarch64-linux-android/release/libnobodywho_uniffi.so \
  nobodywho/js/android/src/main/jniLibs/arm64-v8a/

# x86_64
cp nobodywho/target/x86_64-linux-android/release/libnobodywho_uniffi.so \
  nobodywho/js/android/src/main/jniLibs/x86_64/
```

For iOS:
```bash
cd nobodywho/js
npm run ios -- --release
```

`pod install` only download the released binaries if nothing is present at these paths, so locally built libraries are used as-is.
Remember to delete them (or rebuild) after bumping the package version, as stale binaries are not detected.

### Build for WASM

The WASM build uses Emscripten (`wasm32-unknown-emscripten`), since llama.cpp needs a C/C++ toolchain and libc. It needs a nightly
Rust toolchain (for `-Zbuild-std`).

Then build the WASM module, and bundle it with the TypeScript wrappers:

```sh
cd nobodywho/js
# Configures various things, such as passing -fPIC and setting the linker to em++
npm run wasm
npm run build
```

`src/index.wasm.ts` instantiates the module with that glue (which supplies Emscripten's imports, such as WebGPU and the filesystem),
and then hands its exports to `@ubjs/wasm`. We don't use the `uniffiInitAsync` that uniffi-bindgen-react-native generates for WASM,
since it instantiates the module itself and doesn't know about Emscripten; so the generated `src/generated/wasm/index.ts` is unused.

And then run the tests with:

```sh
TEST_MODEL=/path/to/Qwen_Qwen3-0.6B-Q4_K_M.gguf npm test
```

### Release builds (CI)

In CI, native `.so` files are cross-compiled and uploaded as GitHub Release assets. At install time:
- **Android:** `build.gradle` downloads `.so` files from the GitHub Release matching the package version into `android/src/main/jniLibs/`
- **iOS:** `Nobodywho.podspec` downloads and extracts `NobodyWho.xcframework.zip` from the same release into `NobodyWho.xcframework/`

This keeps the npm package small (code only, no binaries).

## Running tests

### Unit tests

```bash
cd nobodywho/js
npm test
npm run typecheck
```

These tests are also run as a nix flake check (`nix build .#checks.x86_64-linux.js-tests`).

The WASM tests need the WASM build (see "Build for WASM"), and are skipped unless `TEST_MODEL` is set.
The browser test uses the installed Google Chrome.

### Testing on Android

#### Build the test app

```bash
# From project root
nix develop .#android --command bash -c \
  'cd nobodywho/js/test-app/android && \
   ./gradlew assembleDebug -PreactNativeArchitectures=arm64-v8a'
```

#### Run on a connected device

Start Metro first, then install and launch:

```bash
# Terminal 1: start Metro bundler (must be running before the app launches)
cd nobodywho/js/test-app
npx react-native start --port 8081

# Terminal 2: install, set up port forwarding, and launch
adb install -r nobodywho/js/test-app/android/app/build/outputs/apk/debug/app-debug.apk
adb reverse tcp:8081 tcp:8081
adb shell am start -n com.nobodywhotest/.MainActivity
```

## Native crate initialization

`index.react-native.ts` runs `installer.installRustCrate()` and `nobodywho.initialize()` as top-level side effects. The generated `index.tsx` template does the same in isolation, but nothing the wrapper API reaches goes through `index.tsx` — the wrappers import the bindings through `./generated`.

This is deliberate. In release mode Metro's `inlineRequires` defers lazy re-exports until first use of one of their named exports, and the public API (`Model`, `Chat`, `downloadModel`, …) doesn't touch any symbol that lives only in `index.tsx`. Routing install through `index.react-native.ts` (the package entry point) guarantees it runs before any FFI call — otherwise consumers would crash with `Cannot read property 'ubrn_…' of undefined` in release-mode builds.

On the web/Node.js, `index.wasm.ts` similarly instantiates the WASM module with a top-level `await` before re-exporting the public API.

`src/index.tsx` is listed in `noOverwrite` in `ubrn.config.yaml`, so ubrn does not regenerate it; `src/index.react-native.ts` replaces it.

## Known issues

- **uniffi-bindgen-react-native `async static` bug:** Async constructors generate invalid JS
  (`async static` instead of `static async`). Workaround: use free functions instead of async
  constructors in the Rust UniFFI crate. This is why `Model` uses `loadModel()` instead of
  `Model.load()`.
- **Async tool callbacks:** JavaScript cannot synchronously await a Promise, so tool callbacks
  must currently be synchronous. Async support is planned via a channel-based architecture.
