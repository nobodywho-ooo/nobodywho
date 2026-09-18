# NobodyWho Flutter device-test app

Host app for the on-device integration tests that run on real phones via
Firebase Test Lab. It is deliberately a plain Flutter app: it depends on
`nobodywho` but otherwise does nothing a normal consumer would not do.

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

## Building against a released version

`pubspec.yaml` path-depends on `../../flutter/nobodywho`, so this app builds
against the bindings in this repo and needs no override for everyday work. To
build against the published package instead, add a `pubspec_overrides.yaml` next
to it (pub's standard override mechanism, and gitignored here):

```yaml
dependency_overrides:
  nobodywho: <version>
```

That mode is what a real consumer does, so it is the one that tells you whether
what we *shipped* works.

CI does exactly this in the `flutter-released` job, resolving the newest
published version rather than hardcoding one, and omits it in the
`flutter-source` job. See `.github/workflows/mobile-device-tests.yml`.

## Static checks

```bash
just testapp-flutter
```

It is part of `just check`, and CI runs it on every event from `linting.yml`.
