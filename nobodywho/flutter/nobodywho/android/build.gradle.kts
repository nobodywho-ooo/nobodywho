// NobodyWho Flutter Plugin - Android Build Configuration
//
// Android binaries are distributed as one Maven AAR containing every supported
// ABI, released together with this package under the same version. Gradle
// handles downloading, caching, and ABI selection.

plugins {
    id("com.android.library")
}

group = "ooo.nobodywho.nobodywho"
version = "1.0"

val nobodywhoVersion = file("../pubspec.yaml").useLines { lines ->
    lines.map(String::trim)
        .first { it.startsWith("version:") }
        .substringAfter("version:")
        .trim()
}

android {
    namespace = "ooo.nobodywho.nobodywho"
    compileSdk = 36

    findProperty("android.ndkVersion")?.let { ndkVersion = it.toString() }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_11
        targetCompatibility = JavaVersion.VERSION_11
    }

    defaultConfig {
        minSdk = 24
    }
}

dependencies {
    implementation("ai.nobodywho:nobodywho-flutter-android:$nobodywhoVersion")
}
