# Kotlin Build Setup

This document explains how the Kotlin bindings are structured, built, and published.

## Module structure

```
kotlin/
├── build.gradle.kts          # Root: shared version, POM metadata, signing
├── settings.gradle.kts       # Plugin versions, nmcp config, project rename
├── common/                   # All Kotlin source code
│   ├── build.gradle.kts      # kotlin("jvm"), maven-publish, signing
│   ├── src/                  # Wrapper classes (Chat, Model, Tool, etc.)
│   ├── generated/            # UniFFI-generated bindings
│   └── test/                 # Unit + integration tests
├── android/                  # Android packaging
│   ├── build.gradle.kts      # com.android.library, maven-publish, signing
│   └── src/main/AndroidManifest.xml
└── jvm/                      # Desktop JVM packaging
    └── build.gradle.kts      # kotlin("jvm"), maven-publish, signing
```

### Why three modules?

The Kotlin wrapper code is identical across platforms. What differs is how the native library (`libnobodywho_uniffi`) is supplied:

- **Android** depends on the complete multi-ABI `nobodywho-uniffi-android` AAR, which supplies the native entry point and matching `libc++_shared.so`, and uses the AAR variant of JNA (`jna:5.14.0@aar`).
- **Desktop JVM** needs native libs for all platforms in `src/main/resources/` following JNA's naming convention (`linux-x86-64/`, `darwin-aarch64/`, `win32-x86-64/`), inside a regular JAR with JNA as a normal JAR dependency.

A single module can't produce both an AAR and a JAR, so we split into three:

- **`:common`** (published as `nobodywho-core`) — contains all Kotlin code. Pure JVM, no Android dependency. This is where compilation and tests happen.
- **`:android`** — empty shell that depends on `:common` and the complete native AAR and applies the Android Gradle plugin.
- **`:jvm`** — empty shell that depends on `:common`, packages a JAR with desktop native libs in JNA layout.

## Published artifacts

Three artifacts are published to Maven Central:

| Artifact | Type | Contains |
|---|---|---|
| `ai.nobodywho:nobodywho-core` | JAR | Kotlin wrappers + generated UniFFI bindings (~100KB) |
| `ai.nobodywho:nobodywho-android` | AAR | Android facade; depends on `nobodywho-core` and the complete `nobodywho-uniffi-android` runtime AAR |
| `ai.nobodywho:nobodywho` | JAR | Desktop native libs (Linux, macOS, Windows), depends on `nobodywho-core` |

The Android-native release separately publishes
`ai.nobodywho:nobodywho-uniffi-android`. Flutter and React Native use the same
release pipeline, so native binaries are not copied into each wrapper package.

Consumers add one dependency:

```kotlin
// Android
implementation("ai.nobodywho:nobodywho-android:0.1.0")

// Desktop JVM
implementation("ai.nobodywho:nobodywho:0.1.0")
```

Gradle automatically pulls `nobodywho-core` as a transitive dependency.

## The project rename

The `:common` directory is named `common/` on disk, but the Gradle project is renamed in `settings.gradle.kts`:

```kotlin
project(":common").name = "nobodywho-core"
```

This matters because when Gradle publishes `:jvm` or `:android`, their POM files reference dependencies by Gradle project name. Without the rename, the POM would say `<artifactId>common</artifactId>`, which is a poor name for a Maven Central artifact. With the rename, it correctly says `<artifactId>nobodywho-core</artifactId>`.

After the rename, other modules reference it as `project(":nobodywho-core")` instead of `project(":common")`.

## JNA conflict

JNA ships as two artifacts with identical Java classes:
- `jna:5.14.0` (JAR) — for desktop JVM, includes native libs for all desktop platforms
- `jna:5.14.0@aar` (AAR) — for Android, includes the Android JNI native lib

`:common` uses `implementation("net.java.dev.jna:jna:5.14.0")` (the JAR). This is correct for desktop JVM and for compilation. But on Android, both the JAR and AAR would end up on the classpath, causing duplicate class errors.

The `:android` module solves this with an exclude:

```kotlin
api(project(":nobodywho-core")) {
    exclude(group = "net.java.dev.jna")  // Remove JNA JAR from core's transitive deps
}
implementation("net.java.dev.jna:jna:5.14.0@aar")  // Provide JNA AAR instead
```

This exclude also works for the published artifact — when a consumer depends on `nobodywho-android`, Gradle excludes JNA from `nobodywho-core`'s transitive dependencies and uses the AAR variant from the Android module.

## Maven Central publishing

Publishing uses [nmcp](https://github.com/GradleUp/nmcp) (New Maven Central Publishing), a Gradle settings plugin that handles the Central Portal upload API.

```kotlin
// settings.gradle.kts
plugins {
    id("com.gradleup.nmcp.settings") version "1.4.4"
}
nmcpSettings {
    centralPortal {
        username = System.getenv("MAVEN_CENTRAL_USERNAME")
        password = System.getenv("MAVEN_CENTRAL_PASSWORD")
        publishingType = "AUTOMATIC"
    }
}
```

The Kotlin `publishAggregationToCentralPortal` task collects all three wrapper
publications, signs them, and uploads them as one deployment. The shared native
AAR is published by the Android-native release before a Kotlin version pins it.

POM metadata (name, description, license, developers, SCM) is configured once in the root `build.gradle.kts` and applied to all subprojects via `afterEvaluate`. Signing uses in-memory PGP keys from environment variables (`SIGNING_KEY`, `SIGNING_PASSWORD`).

### Required environment variables for publishing

| Variable | Purpose |
|---|---|
| `MAVEN_CENTRAL_USERNAME` | Central Portal API token username |
| `MAVEN_CENTRAL_PASSWORD` | Central Portal API token password |
| `SIGNING_KEY` | ASCII-armored GPG private key |
| `SIGNING_PASSWORD` | GPG key passphrase |

### Testing locally

After staging the native AAR inputs, publish the native artifacts before the
Kotlin wrappers (no credentials needed):

```bash
./gradlew -p ../android publishToMavenLocal
./gradlew publishToMavenLocal
```

Artifacts go to `~/.m2/repository/ai/nobodywho/`. Inspect the POMs to verify dependencies and metadata.

## Version management

The version is set once in the root `build.gradle.kts`:

```kotlin
allprojects {
    version = providers.gradleProperty("version").getOrElse("3.0.0")
}
```

All three Kotlin artifacts share the same version. The CI release job passes
`-Pversion=X.Y.Z` from the git tag; otherwise the root build uses its default
version. The native AAR has its own version, set by the `nobodywho-android-vX.Y.Z`
release tag; this project's `android/build.gradle.kts` pins the exact native
version it depends on.
