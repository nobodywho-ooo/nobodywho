# NobodyWho Kotlin Test App

Instrumentation host for the on-device tests that run on real phones via
Firebase Test Lab. It deliberately has no UI: `src/androidTest` drives the
Kotlin bindings directly — chat, streaming, and tool calling — and the app
exists only as the target that the test APK is installed against.

This is a **standalone Gradle build**. It consumes the bindings from
`../../kotlin` via a composite build (`includeBuild` in `settings.gradle.kts`),
so the bindings' own Gradle configuration stays free of app concerns. There is
nothing to wire into `kotlin/settings.gradle.kts`.

## Running the tests locally

1. Build the native library for Android arm64 and drop it where the `:android`
   module expects it:
   ```bash
   nix develop .#android --command bash -c \
     'cd nobodywho && cargo build -p nobodywho-uniffi --target aarch64-linux-android --release'
   mkdir -p nobodywho/kotlin/android/build/jniLibs/arm64-v8a
   cp nobodywho/target/aarch64-linux-android/release/libnobodywho_uniffi.so \
     nobodywho/kotlin/android/build/jniLibs/arm64-v8a/
   ```

2. With a phone plugged in, run the suite — much faster than a CI round-trip:
   ```bash
   ./gradlew connectedDebugAndroidTest
   ```
   Results land in `build/reports/androidTests/connected/`.

The test downloads its model from HuggingFace into the app's own cache on first
run, so there is no `adb push` step — the device just needs network access.

To build the APK pair without running anything:
```bash
./gradlew assembleDebug assembleDebugAndroidTest
```

## Building against a released version

By default this app is built against the bindings in this repo. Passing a
version instead resolves the published artifact from Maven Central, which needs
no Rust toolchain and no NDK:

```bash
./gradlew assembleDebug assembleDebugAndroidTest -PnobodywhoVersion=2.2.0
```

That mode is what a real consumer does, so it is the one that tells you whether
what we *shipped* works. Keep this app as simple as any consumer's app would
be — if it needs an extra step to work, that is a library defect to fix in the
library, not a workaround to add here.

## On-device tests (Firebase Test Lab)

CI runs the suite on pinned physical devices through the `mobile-device-tests`
workflow: nightly on `main`, on a `nobodywho-kotlin-v*` release tag, and on
demand via a `/kotlin-device-source-ci` or `/kotlin-device-released-ci` comment
on a PR. See `.github/workflows/mobile-device-tests.yml`.
