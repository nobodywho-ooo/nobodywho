# NobodyWho Android native artifacts

This is the single Android-native distribution project. It packages the
prebuilt Rust libraries into two Maven AARs:

- `ai.nobodywho:nobodywho-flutter-android`
- `ai.nobodywho:nobodywho-uniffi-android`

Both contain `arm64-v8a` and `x86_64`. Android selects the matching directory
when it builds or installs an app; none of the bindings detect an ABI or search
for native-library paths at runtime.

There are two artifacts because Flutter and UniFFI have different entry-point
libraries. The Flutter AAR also owns `libc++_shared.so`. The UniFFI AAR omits
it: React Native already supplies the process-wide C++ runtime, while the
Kotlin wrapper adds the matching runtime to its own AAR.

## Versions and releases

The native artifact version comes from the release tag
(`nobodywho-android-vX.Y.Z`) via `-Pversion=`; local/unstamped builds default
to `0.0.0-local`. Flutter, React Native, and Kotlin each pin the native version
they depend on as a `nobodywhoNativeVersion` constant next to their
`ai.nobodywho:...` dependency declaration. This keeps wrapper releases
independent: a Dart, JavaScript, or Kotlin-only change can reuse an existing
native release without touching its pin, and cutting a new native release
doesn't force every wrapper to adopt it immediately.

A tag such as `nobodywho-android-v2.5.0` builds both AARs, attaches them to one
GitHub release, and publishes them to Maven Central.

## Local builds

CI fills these ignored staging directories with Cargo's Android outputs:

```text
build/flutter/jniLibs/<abi>/*.so
build/uniffi/jniLibs/<abi>/*.so
```

Once staged, build and validate both artifacts with the Kotlin Gradle wrapper:

```bash
./nobodywho/kotlin/gradlew -p nobodywho/android check assemble
```

To test an unpublished AAR in a binding, point that binding at the local file:

```bash
export NOBODYWHO_FLUTTER_ANDROID_AAR="$PWD/nobodywho/android/build/outputs/nobodywho-flutter-android-0.0.0-local.aar"
export NOBODYWHO_UNIFFI_ANDROID_AAR="$PWD/nobodywho/android/build/outputs/nobodywho-uniffi-android-0.0.0-local.aar"
```

Normal consumer builds do not use these overrides. Gradle resolves the pinned
Maven coordinate and handles downloading, caching, and packaging the correct
ABI.
