# NobodyWho React Native device-test app

Host app for the on-device tests that run on real phones via Firebase Test Lab.
It is a plain React Native app that depends on `react-native-nobodywho` from
npm and does nothing a normal consumer would not do.

If this app ever needs an extra step to work, that is a defect to fix in the
library — not a workaround to add here.

## How the test is structured

`App.tsx` drives the public JavaScript API — completion, streaming and tool
calling, the same three checks as the Kotlin and Flutter device tests — and
renders `PASS` or `FAIL:<reason>`.
`android/app/src/androidTest/.../DeviceInferenceTest.java` launches the app and
waits for that outcome.

Detox is React Native's usual e2e runner, but it drives tests from a Node
process on the host and the test APK is only a bridge to it. Firebase Test Lab
runs the APK pair with no host runner, so the instrumentation test has to be
self-contained; it observes the app through UI Automator instead.

## Build the test APKs

```bash
npm ci
cd android
./gradlew app:assembleRelease app:assembleReleaseAndroidTest
```

Release, not debug: a debug React Native build fetches its JS bundle from Metro
at runtime, which does not exist on a test-lab device. Release embeds the
bundle, and the template signs it with the debug key.

This produces the pair Firebase Test Lab needs:

- `android/app/build/outputs/apk/release/app-release.apk`
- `android/app/build/outputs/apk/androidTest/release/app-release-androidTest.apk`

Run them against a connected device with
`./gradlew app:connectedReleaseAndroidTest`.

## Testing against the bindings in this repo

`package.json` depends on the published package, which is what a consumer gets.
To exercise the in-repo binding instead:

```bash
npm install ../../react-native
# stage a locally built .so where the binding's resolver looks for it,
# otherwise it downloads the last released one
cargo build -p nobodywho-uniffi --target aarch64-linux-android --release
mkdir -p ../../react-native/android/build/nobodywho-native/<version>/arm64-v8a
cp ../../target/aarch64-linux-android/release/libnobodywho_uniffi.so \
   ../../react-native/android/build/nobodywho-native/<version>/arm64-v8a/
```

`<version>` is the `version` field of `nobodywho/react-native/package.json`.
Mixing a locally built binding with a downloaded release binary for another ABI
fails to link on mismatched uniffi symbols, which is why
`reactNativeArchitectures` is pinned to `arm64-v8a`.

CI does exactly this in the `react-native` job, and skips it in the
`react-native-released` job. See `.github/workflows/mobile-device-tests.yml`.
