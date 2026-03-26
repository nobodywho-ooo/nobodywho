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
├── NobodywhoReactNative.podspec # CocoaPods spec (iOS)
│
├── src/
│   ├── wrapper.ts               # Public entry point (hand-written, safe to edit)
│   ├── index.tsx                # Native init + generated re-exports (generated, do not edit)
│   ├── streaming.ts             # streamTokens() async generator (hand-written)
│   ├── sampler_presets.ts       # SamplerPresets static class (hand-written)
│   └── NativeNobodywhoReactNative.ts  # TurboModule spec (generated)
│
├── generated/                   # Generated bindings (gitignored, regenerate from Rust)
│   ├── ts/
│   │   ├── nobodywho.ts         # TypeScript bindings
│   │   └── nobodywho-ffi.ts     # Low-level FFI types
│   └── cpp/
│       ├── nobodywho.cpp        # C++ JSI bridge
│       └── nobodywho.hpp        # C++ header
│
├── cpp/                         # TurboModule C++ glue (generated, committed)
│   ├── nobodywho-react-native.cpp
│   └── nobodywho-react-native.h
│
├── ios/                         # iOS native module (generated, committed)
│   ├── NobodywhoReactNative.h
│   └── NobodywhoReactNative.mm
│
├── android/                     # Android native module (generated, committed)
│   ├── build.gradle
│   ├── CMakeLists.txt
│   ├── cpp-adapter.cpp
│   └── src/main/
│       ├── AndroidManifest.xml
│       ├── AndroidManifestNew.xml
│       └── java/com/nobodywhoreactnative/
│           ├── NobodywhoReactNativeModule.kt
│           └── NobodywhoReactNativePackage.kt
│
└── test-app/                    # Minimal React Native app for testing
    ├── App.tsx                  # Test screen with sanity checks
    └── android/                 # Android project (Gradle)
```

## Build system overview

The build has two code generation steps, then a native compilation step.

### Step 1: Generate bindings from Rust

Build the UniFFI crate for the host, then run the bindgen to produce TypeScript + C++:

```bash
# From nobodywho/ (workspace root)
cargo build -p nobodywho-uniffi

# Generate the bindings (must run from nobodywho/ dir so cargo metadata works)
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

### Step 2: Generate TurboModule glue (one-time)

This produces the native module registration code for iOS and Android. Only needs to be re-run if the package name or structure changes:

```bash
cd react-native
npx uniffi-bindgen-react-native generate jsi turbo-module \
  --config ubrn.config.yaml \
  nobodywho
```

This generates the files in `cpp/`, `ios/`, and `android/`.

### Step 3: Build native static libraries for mobile targets

The Android CMake build expects static libraries (`.a` files), not shared libraries. The `.a` gets linked into the final `libnobodywho-react-native.so` alongside the C++ JSI bridge.

Use the nix android shell which provides NDK, cmake, and all cross-compilation environment variables:

```bash
# From project root (where flake.nix is)

# Android ARM64 (physical devices)
nix develop .#android --command bash -c \
  'cd nobodywho && cargo build -p nobodywho-uniffi --target aarch64-linux-android --release'

# Android x86_64 (emulator)
nix develop .#android --command bash -c \
  'cd nobodywho && cargo build -p nobodywho-uniffi --target x86_64-linux-android --release'
```

Then copy the `.a` files to where the Android CMake build expects them:

```bash
# ARM64
cp nobodywho/target/aarch64-linux-android/release/libnobodywho_uniffi.a \
  nobodywho/react-native/android/src/main/jniLibs/arm64-v8a/

# x86_64
cp nobodywho/target/x86_64-linux-android/release/libnobodywho_uniffi.a \
  nobodywho/react-native/android/src/main/jniLibs/x86_64/
```

For iOS:
```bash
cargo build -p nobodywho-uniffi --target aarch64-apple-ios --release
cargo build -p nobodywho-uniffi --target aarch64-apple-ios-sim --release
```

## Testing on Android

### Build and install the test app

```bash
# From project root
nix develop .#android --command bash -c \
  'cd nobodywho/react-native/test-app/android && \
   ANDROID_HOME=$ANDROID_SDK_ROOT \
   ./gradlew assembleDebug -PreactNativeArchitectures=arm64-v8a'

# Install on connected device
adb install -r nobodywho/react-native/test-app/android/app/build/outputs/apk/debug/app-debug.apk
```

### Run with Metro (development)

```bash
# Terminal 1: start Metro bundler
cd nobodywho/react-native/test-app
npx react-native start --port 8081

# Terminal 2: set up port forwarding and launch
adb reverse tcp:8081 tcp:8081
adb shell am start -n com.nobodywhotest/.MainActivity
```

## Rebuilding after changes

If you change the Rust code in `uniffi/src/lib.rs`:

1. `cargo build -p nobodywho-uniffi` — rebuild the host crate
2. Re-run the `generate jsi bindings` command (Step 1) — regenerate TypeScript + C++
3. Rebuild static libraries for your target platforms (Step 3)
4. Copy `.a` files to `android/src/main/jniLibs/`
5. Rebuild the test app APK

If you only change the TypeScript wrapper (`src/*.ts`), no regeneration or rebuild is needed — Metro hot-reloads automatically.

## Files not committed to git

- `node_modules/` and `package-lock.json`
- `android/src/main/jniLibs/*/libnobodywho_uniffi.a` — build artifacts (produced by CI or local cross-compilation)
- `NobodywhoReactNativeFramework.xcframework` — iOS build artifact (produced by CI)

## Files committed to git

Everything else is committed, including:
- `src/` — hand-written TypeScript wrapper + generated `NativeNobodywhoReactNative.ts`
- `generated/` — generated TypeScript + C++ bindings (regenerate if Rust API changes)
- `cpp/`, `ios/`, `android/` — TurboModule glue (generated once, rarely changes)
- `ubrn.config.yaml`, `package.json`, `NobodywhoReactNative.podspec`

## Known issues

- **uniffi-bindgen-react-native `async static` bug:** Async constructors generate invalid JS
  (`async static` instead of `static async`). Workaround: use free functions instead of async
  constructors in the Rust UniFFI crate. This is why `Model` uses `loadModel()` instead of
  `Model.load()`.
