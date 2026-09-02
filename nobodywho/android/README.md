# NobodyWho Android packaging

This Gradle project packages prebuilt Rust libraries for the three Android
bindings. Each binding owns the Android artifact it releases:

- Flutter publishes `ai.nobodywho:nobodywho-flutter-android` with the version
  from its `pubspec.yaml`.
- React Native publishes `ai.nobodywho:nobodywho-react-native-android` with
  the version from its `package.json`.
- Kotlin publishes `ai.nobodywho:nobodywho-android`, containing its Kotlin
  API and native libraries together.

The packaging project also creates an unpublished
`nobodywho-uniffi-android` AAR. It is the input from which the complete Kotlin
AAR is built, locally and in CI.

Every artifact contains `arm64-v8a` and `x86_64`. Android selects the
matching ABI when it builds an application; bindings do not search for native
libraries at runtime.

## Shared C++ runtime conflicts

Flutter and Kotlin package their matching `libc++_shared.so` because the
entry-point and dynamic backend libraries share one C++ runtime. If another
dependency packages the same file, Android Gradle Plugin may stop at
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
runtime versions compatible. React Native excludes NobodyWho's copy while
extracting its native AAR because React Native supplies the process-wide C++
runtime itself.

## Candidate and release flow

`.github/workflows/package-android-candidates.yml` downloads the exact Android
outputs from `build.yml`, stages both ABIs, strips and validates the native
libraries, and builds the Flutter and React Native native AARs plus the UniFFI
input for Kotlin. The two native AARs are also placed in an isolated Maven
repository for source-device tests.

On a Flutter or React Native release tag:

1. The release-shaped package and this run's native AAR run on Firebase
   hardware.
2. That same AAR file is published to Maven Central, unless that version
   already exists there (Central versions are immutable, so a re-run of a
   partially failed release does not stop here).
3. CI waits until normal Maven resolution can see it.
4. The pub or npm package is published.

On a Kotlin release tag, the device test and the release both build
`nobodywho-android` from this run's UniFFI AAR, then the Kotlin artifacts are
published to Maven Central as before.

There is no separate Android version or Android release tag. Every Android
artifact shares the version of its owning binding.

## Local builds

Stage Cargo's Android outputs in these ignored directories:

```text
build/flutter/jniLibs/<abi>/*.so
build/uniffi/jniLibs/<abi>/*.so
```

Then build whichever local input you need:

```bash
# Flutter
./nobodywho/kotlin/gradlew -p nobodywho/android nativeAar \
  -PnobodywhoBinding=flutter

# React Native
./nobodywho/kotlin/gradlew -p nobodywho/android nativeAar \
  -PnobodywhoBinding=react-native

# Kotlin source-build input
./nobodywho/kotlin/gradlew -p nobodywho/android nativeAar \
  -PnobodywhoBinding=uniffi
```

Local builds default to `0.0.0-local`. Point a binding source build at the
corresponding file:

```bash
export NOBODYWHO_FLUTTER_ANDROID_AAR="$PWD/nobodywho/android/build/outputs/nobodywho-flutter-android-0.0.0-local.aar"
export NOBODYWHO_REACT_NATIVE_ANDROID_AAR="$PWD/nobodywho/android/build/outputs/nobodywho-react-native-android-0.0.0-local.aar"
export NOBODYWHO_UNIFFI_ANDROID_AAR="$PWD/nobodywho/android/build/outputs/nobodywho-uniffi-android-0.0.0-local.aar"
```

Published Flutter and React Native packages resolve their same-version native
AAR from Maven Central. Published Kotlin consumers need no second native
dependency because the libraries are inside `nobodywho-android`.
