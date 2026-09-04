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
outputs from `build.yml`, adds the NDK's `libc++_shared.so`, strips and
validates the native libraries, and builds the Flutter and React Native native AARs plus the UniFFI
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

Place Cargo's Android outputs in `build/inputs/<integration>/` (ignored), in
the layout `build.yml` uploads. `<integration>` is `flutter` or `uniffi`;
`<target>` is `aarch64-linux-android` or `x86_64-linux-android`:

```bash
INTEGRATION=uniffi   # or flutter
TARGET=aarch64-linux-android
NDK_LIBS="$ANDROID_NDK/toolchains/llvm/prebuilt/$(uname -s | tr A-Z a-z)-x86_64/sysroot/usr/lib"
IN="nobodywho/android/build/inputs/$INTEGRATION"
OUT="nobodywho/target/$TARGET/release"
mkdir -p "$IN"
cp "$OUT/libnobodywho_$INTEGRATION.so" "$IN/libnobodywho-$INTEGRATION-$TARGET-release.so"
cp -R "$OUT/nobodywho-runtime" "$IN/nobodywho-runtime-$TARGET"
cp "$NDK_LIBS/$TARGET/libc++_shared.so" "$IN/nobodywho-runtime-$TARGET/"
```

Repeat for the other target; the AAR always ships both ABIs. The x86_64 build
also needs the `libonnxruntime.so` that `build.yml` fetches from Microsoft's
ONNX Runtime AAR, placed at `$IN/libonnxruntime.so`.

Then build the AAR you need (`-PnobodywhoInputDir=` overrides the input
directory; local builds default to version `0.0.0-local`):

```bash
./nobodywho/kotlin/gradlew -p nobodywho/android nativeAar -PnobodywhoBinding=flutter
./nobodywho/kotlin/gradlew -p nobodywho/android nativeAar -PnobodywhoBinding=react-native
./nobodywho/kotlin/gradlew -p nobodywho/android nativeAar -PnobodywhoBinding=uniffi
```

Flutter and React Native resolve their native AAR through Maven, so a local
build is consumed the same way: publish it to `~/.m2` under the version the
binding declares, then let the test app read that repository.

```bash
./nobodywho/kotlin/gradlew -p nobodywho/android publishToMavenLocal \
  -PnobodywhoBinding=flutter -Pversion=<version from flutter/nobodywho/pubspec.yaml>
export NOBODYWHO_CANDIDATE_MAVEN_REPO="$HOME/.m2/repository"
```

Any other app adds `mavenLocal()` to its repositories instead. Kotlin embeds
the libraries, so its source build takes the UniFFI AAR file directly:

```bash
export NOBODYWHO_UNIFFI_ANDROID_AAR="$PWD/nobodywho/android/build/outputs/nobodywho-uniffi-android-0.0.0-local.aar"
```
