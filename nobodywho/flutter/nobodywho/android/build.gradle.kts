// NobodyWho Flutter Plugin - Android Build Configuration
//
// Android binaries are distributed as one Maven AAR containing every
// supported ABI. Gradle handles downloading, caching, and ABI selection.

plugins {
    id("com.android.library")
}

group = "ooo.nobodywho.nobodywho"
version = "1.0"

// Pinned independently of this plugin's own version — bump deliberately to adopt
// a newer nobodywho-android-vX.Y.Z release.
val nobodywhoNativeVersion = "2.5.0"
val localNativeAar = providers.gradleProperty("nobodywhoFlutterNativeAar").orNull
    ?: System.getenv("NOBODYWHO_FLUTTER_ANDROID_AAR")
val localNativeRoot = layout.buildDirectory.dir("localNativeAar")
val extractLocalNativeAar = localNativeAar?.let { path ->
    tasks.register<Sync>("extractLocalNativeAar") {
        val aar = file(path)
        require(aar.isFile) {
            "NOBODYWHO_FLUTTER_ANDROID_AAR does not exist: ${aar.absolutePath}"
        }
        from(zipTree(aar)) {
            include("jni/**/*.so")
        }
        into(localNativeRoot)
    }
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

    if (extractLocalNativeAar != null) {
        sourceSets {
            getByName("main") {
                jniLibs.srcDir(localNativeRoot.get().dir("jni").asFile)
            }
        }
    }
}

dependencies {
    if (localNativeAar == null) {
        implementation("ai.nobodywho:nobodywho-flutter-android:$nobodywhoNativeVersion")
    }
}

if (extractLocalNativeAar != null) {
    tasks.named("preBuild") {
        dependsOn(extractLocalNativeAar)
    }
}
