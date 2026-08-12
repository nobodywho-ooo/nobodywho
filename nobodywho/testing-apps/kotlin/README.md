# NobodyWho Kotlin Test App

Minimal Android app for exercising the Kotlin bindings on real hardware —
chat, streaming, tool calling, and vision (camera). It doubles as the host for
the on-device instrumentation tests that run in Firebase Test Lab.

This is a **standalone Gradle build**. It consumes the bindings from
`../../kotlin` via a composite build (`includeBuild` in `settings.gradle.kts`),
so the bindings' own Gradle configuration stays free of app/compose concerns.
There is nothing to wire into `kotlin/settings.gradle.kts`.

## Build & run manually

1. Build the native library for Android arm64 and drop it where the `:android`
   module expects it:
   ```bash
   nix develop .#android --command bash -c \
     'cd nobodywho && cargo build -p nobodywho-uniffi --target aarch64-linux-android --release'
   mkdir -p nobodywho/kotlin/android/build/jniLibs/arm64-v8a
   cp nobodywho/target/aarch64-linux-android/release/libnobodywho_uniffi.so \
     nobodywho/kotlin/android/build/jniLibs/arm64-v8a/
   ```
   The `:android` module adds `libc++_shared.so` from the NDK itself (the
   binding dynamically links it), so nothing else needs copying here — this app
   consumes the library exactly as a consumer of the published AAR would.
   An NDK must be discoverable via `ANDROID_NDK_HOME` or the Android SDK.

2. Build and install the app (run from this directory):
   ```bash
   nix develop .#android --command bash -c \
     'cd nobodywho/testing-apps/kotlin && ./gradlew assembleDebug'
   adb install -r build/outputs/apk/debug/*.apk
   ```

3. Grant storage permission, then push a model:
   ```bash
   adb shell appops set ai.nobodywho.testapp MANAGE_EXTERNAL_STORAGE allow
   adb shell mkdir -p /sdcard/models
   adb push /path/to/model.gguf /sdcard/models/
   adb push /path/to/mmproj.gguf /sdcard/models/  # optional, for vision
   ```

Enter the model path (e.g. `/sdcard/models/model.gguf`) and optionally a vision
projector path, then tap "Load Model". Once loaded you can chat, take photos for
vision analysis (camera button appears if a projector is loaded), and test tool
calling (ask "What time is it?").

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

`src/androidTest` holds instrumentation tests that run on physical devices in
CI via the `mobile-device-tests` workflow (manual `workflow_dispatch`). The
workflow builds the arm64 `.so`, assembles the app + test APKs, pushes the model
to the device with `gcloud ... --other-files`, and runs the suite on the pinned
devices. See `.github/workflows/mobile-device-tests.yml`.

To build the test APKs locally:
```bash
./gradlew assembleDebug assembleDebugAndroidTest
```

With a phone plugged in, running the suite directly is much faster than waiting
on a CI round-trip:
```bash
./gradlew connectedDebugAndroidTest
```
Results land in `build/reports/androidTests/connected/`.
