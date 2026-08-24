# NobodyWho Flutter device-test app

Host app for the on-device integration tests that run on real phones via
Firebase Test Lab. It is deliberately a plain Flutter app: it depends on
`nobodywho` from pub.dev and does nothing a normal consumer would not do.

If this app ever needs an extra step to work, that is a defect to fix in the
library — not a workaround to add here.

`integration_test/device_inference_test.dart` mirrors the Kotlin binding's
`DeviceInferenceTest`: completion, streaming, and tool calling, against a model
downloaded on-device from Hugging Face.

## Build the test APKs

```bash
flutter pub get
cd android
./gradlew app:assembleAndroidTest \
          app:assembleDebug -Ptarget=integration_test/device_inference_test.dart
```

This produces the pair Firebase Test Lab needs:

- `build/app/outputs/apk/debug/app-debug.apk`
- `build/app/outputs/apk/androidTest/debug/app-debug-androidTest.apk`

With a phone plugged in, running the suite directly is much faster than waiting
on a CI round-trip:

```bash
flutter test integration_test/device_inference_test.dart -d <device-id>
```

`flutter devices` lists the ids.

On NixOS the Flutter SDK sits in a read-only `/nix/store`, so a bare
`./gradlew` cannot write its project cache next to the SDK's included build and
dies with no output at all. Pass the cache dir Flutter itself uses:

```bash
./gradlew app:assembleAndroidTest \
  --project-cache-dir="$HOME/.cache/flutter/nix-flutter-tools-gradle/<hash>/cache"
```

`flutter build apk --debug --verbose | grep executing` prints the exact path.

## Testing against local bindings instead of pub.dev

`pubspec.yaml` depends on the published package, which is what a consumer gets.
To exercise the bindings in this repo instead, add a `pubspec_overrides.yaml`
next to it (pub's standard mechanism for a local override, and gitignored here):

```yaml
dependency_overrides:
  nobodywho:
    path: ../../flutter/nobodywho
```

CI does exactly this in the `flutter` job, and omits it in the
`flutter-released` job. See `.github/workflows/mobile-device-tests.yml`.
