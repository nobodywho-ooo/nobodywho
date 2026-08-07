# NobodyWho Kotlin Test App

Minimal Android app for exercising the Kotlin bindings on real hardware —
chat, streaming, tool calling, and vision (camera). It doubles as the host for
the on-device instrumentation tests that run in Firebase Test Lab.

This is a **standalone Gradle build**. It consumes the bindings from
`../../kotlin` via a composite build (`includeBuild` in `settings.gradle.kts`),
so the bindings' own Gradle configuration stays free of app/compose concerns.
There is nothing to wire into `kotlin/settings.gradle.kts`.

## Build & run manually

1. Build the native library for Android arm64 and place it — plus the NDK C++
   runtime it dynamically links against (`libc++_shared.so`) — in the app's
   jniLibs:
   ```bash
   nix develop .#android --command bash -c \
     'cd nobodywho && cargo build -p nobodywho-uniffi --target aarch64-linux-android --release'
   DEST=nobodywho/testing-apps/kotlin/src/main/jniLibs/arm64-v8a
   mkdir -p "$DEST"
   cp nobodywho/target/aarch64-linux-android/release/libnobodywho_uniffi.so "$DEST/"
   # Without libc++_shared.so, loading the .so fails at runtime with
   # "dlopen failed: library libc++_shared.so not found".
   cp "$ANDROID_NDK_ROOT/toolchains/llvm/prebuilt/linux-x86_64/sysroot/usr/lib/aarch64-linux-android/libc++_shared.so" "$DEST/"
   ```

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

## On-device tests (Firebase Test Lab)

`src/androidTest` holds instrumentation tests that run on physical devices in
CI via the `mobile-device-tests` workflow (manual `workflow_dispatch`). The
workflow builds the arm64 `.so`, assembles the app + test APKs, pushes the model
to the device with `gcloud ... --other-files`, and runs the suite on the pinned
devices. See `.github/workflows/mobile-device-tests.yml`.

To build the test APK locally:
```bash
./gradlew assembleDebug assembleDebugAndroidTest
```
