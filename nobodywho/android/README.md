# NobodyWho Android native artifacts

This is the single Android-native distribution project. It packages the
prebuilt Rust libraries into two Maven AARs:

- `ai.nobodywho:nobodywho-flutter-android`
- `ai.nobodywho:nobodywho-uniffi-android`

Both contain `arm64-v8a` and `x86_64`, including the matching
`libc++_shared.so`. Android selects the matching directory when it builds or
installs an app; none of the bindings detect an ABI or search for native-library
paths at runtime.

There are two artifacts because Flutter and UniFFI have different entry-point
libraries. Kotlin consumes the complete UniFFI AAR. React Native extracts the
same AAR but omits its `libc++_shared.so`, because React Native supplies the
process-wide C++ runtime itself.

## Shared C++ runtime conflicts

The Flutter and Kotlin AARs must package their matching `libc++_shared.so`
because the entry-point and dynamic backend libraries share one C++ runtime.
If another dependency packages the same file, Android Gradle Plugin may stop at
`mergeNativeLibs` with a duplicate-file error. Resolve that in the consuming
application module:

```kotlin
android {
    packaging {
        jniLibs {
            pickFirsts += "**/libc++_shared.so"
        }
    }
}
```

`pickFirsts` selects one process-wide runtime; it does not make incompatible
runtime versions compatible. Ensure the dependencies ship compatible NDK C++
runtimes. React Native consumers do not need this rule because NobodyWho's
React Native module excludes its copy before packaging.

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

### Required release order

The native release must be available from Maven Central before releasing any
binding that pins it. For version `2.5.0`:

1. Push `nobodywho-android-v2.5.0`.
2. Wait until both `nobodywho-flutter-android:2.5.0` and
   `nobodywho-uniffi-android:2.5.0` resolve from Maven Central.
3. Only then push the Flutter, Kotlin, or React Native release tags that use
   that native version.

Before step 2 completes, source builds must use the local AAR overrides below;
dependency resolution without an override is expected to fail.

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
