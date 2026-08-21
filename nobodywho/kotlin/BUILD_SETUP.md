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

The Kotlin wrapper code is identical across platforms. What differs is how the native library (`libnobodywho_uniffi`) is packaged:

- **Android** needs the native `.so` in `jniLibs/{arm64-v8a,x86_64}/` inside an AAR, and JNA must be the AAR variant (`jna:5.14.0@aar`).
- **Desktop JVM** needs native libs for all platforms in `src/main/resources/` following JNA's naming convention (`linux-x86-64/`, `darwin-aarch64/`, `win32-x86-64/`), inside a regular JAR with JNA as a normal JAR dependency.

A single module can't produce both an AAR and a JAR, so we split into three:

- **`:common`** (published as `nobodywho-core`) — contains all Kotlin code. Pure JVM, no Android dependency. This is where compilation and tests happen.
- **`:android`** — empty shell that depends on `:common`, applies the Android Gradle plugin, and packages the AAR with Android-specific JNI libs.
- **`:jvm`** — empty shell that depends on `:common`, packages a JAR with desktop native libs in JNA layout.

## Published artifacts

Three artifacts are published to Maven Central:

| Artifact | Type | Contains |
|---|---|---|
| `ai.nobodywho:nobodywho-core` | JAR | Kotlin wrappers + generated UniFFI bindings (~100KB) |
| `ai.nobodywho:nobodywho-android` | AAR | Android native libs (arm64-v8a, x86_64), depends on `nobodywho-core` |
| `ai.nobodywho:nobodywho` | JAR | Desktop native libs (Linux, macOS, Windows), depends on `nobodywho-core` |

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

The `publishAggregationToCentralPortal` task collects all three publications, signs them, and uploads them as a single atomic deployment bundle. Maven Central validates them together — all succeed or all fail.

POM metadata (name, description, license, developers, SCM) is configured once in the root `build.gradle.kts` and applied to all subprojects via `afterEvaluate`. Signing uses in-memory PGP keys from environment variables (`SIGNING_KEY`, `SIGNING_PASSWORD`).

### Required environment variables for publishing

| Variable | Purpose |
|---|---|
| `MAVEN_CENTRAL_USERNAME` | Central Portal API token username |
| `MAVEN_CENTRAL_PASSWORD` | Central Portal API token password |
| `SIGNING_KEY` | ASCII-armored GPG private key |
| `SIGNING_PASSWORD` | GPG key passphrase |

### Testing locally

Publish to the local Maven repository (no credentials needed):

```bash
./gradlew publishToMavenLocal
```

Artifacts go to `~/.m2/repository/ai/nobodywho/`. Inspect the POMs to verify dependencies and metadata.

## Version management

The version is set once in the root `build.gradle.kts`:

```kotlin
allprojects {
    version = "0.1.0"
}
```

All three artifacts share the same version. The CI release job also passes `-Pversion=X.Y.Z` from the git tag, though currently the hardcoded version must match.
