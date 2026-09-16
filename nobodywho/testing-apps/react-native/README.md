# NobodyWho React Native device-test app

Host app for the on-device tests that run on real phones via Firebase Test Lab.
It is a plain React Native app that depends on `react-native-nobodywho` and otherwise does
nothing a normal consumer would not do.

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
npm ci --prefix ../../react-native   # the linked binding's own deps; npm does not install them for us

# Build this branch's native library and drop it where the binding looks for
# it — otherwise it downloads the last released .so, and this branch's
# JavaScript against an old native library fails to link on uniffi's checksum
# symbols.
cargo build -p nobodywho-uniffi --target aarch64-linux-android --release
VERSION=$(node -p "require('../../react-native/package.json').version")
DEST=../../react-native/android/build/nobodywho-native/$VERSION/arm64-v8a
mkdir -p "$DEST"
cp ../../target/aarch64-linux-android/release/libnobodywho_uniffi.so "$DEST/"

cd android
./gradlew app:assembleRelease app:assembleReleaseAndroidTest
```

Release, not debug: a debug React Native build fetches its JS bundle from Metro
at runtime, which does not exist on a test-lab device. Release embeds the
bundle, and the template signs it with the debug key.

`android/gradle.properties` pins `reactNativeArchitectures` to `arm64-v8a` so
the binding does not also download a released x86_64 `.so` to sit beside the
arm64 one built above — the same mismatch, inside one APK.

This produces the pair Firebase Test Lab needs:

- `android/app/build/outputs/apk/release/app-release.apk`
- `android/app/build/outputs/apk/androidTest/release/app-release-androidTest.apk`

Run them against a connected device with
`./gradlew app:connectedReleaseAndroidTest`.

## Building against a released version

`package.json` depends on `file:../../react-native`, so `npm ci` links the
binding in this repo. To build against the
published package instead, install it by name. That replaces the link, and the
binding resolves the released `.so` that matches it, so none of the Rust setup
above is needed:

```bash
npm install react-native-nobodywho@latest
cd android
./gradlew app:assembleRelease app:assembleReleaseAndroidTest
```

That mode is what a real consumer does, so it is the one that tells you whether
what we *shipped* works.

CI does exactly this in the `react-native-released` job, resolving the newest
published version rather than hardcoding one, and skips it in the
`react-native-source` job. See `.github/workflows/mobile-device-tests.yml`.

## Static checks

```bash
just testapp-react-native
```

It is part of `just check`, and CI runs it on every event from `linting.yml`.
