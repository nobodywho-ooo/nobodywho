plugins {
    id("com.android.library")
    id("org.jetbrains.kotlin.android")
    id("maven-publish")
    signing
}

// Pinned independently of this module's own version — bump deliberately to adopt
// a newer nobodywho-android-vX.Y.Z release.
val nobodywhoNativeVersion = "2.5.0"
val localNativeAar = providers.gradleProperty("nobodywhoNativeAar")
    .orElse(providers.environmentVariable("NOBODYWHO_UNIFFI_ANDROID_AAR"))
val localNativeRoot = layout.buildDirectory.dir("localNativeAar")
val extractLocalNativeAar by tasks.registering(Sync::class) {
    from({ localNativeAar.orNull?.let { zipTree(it) } ?: files() }) {
        include("jni/**/*.so")
    }
    into(localNativeRoot)
}

android {
    namespace = "ai.nobodywho"
    compileSdk = 35

    defaultConfig {
        minSdk = 26
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_11
        targetCompatibility = JavaVersion.VERSION_11
    }

    kotlinOptions {
        jvmTarget = "11"
    }

    sourceSets {
        getByName("main") {
            // A local override is unpacked here. Normal builds consume the same
            // libraries, including libc++_shared.so, from the Maven AAR.
            jniLibs.srcDir(localNativeRoot.map { it.dir("jni") })
        }
    }
}

dependencies {
    // Exclude the JNA JAR from :common — Android needs the AAR variant instead
    api(project(":nobodywho-core")) {
        exclude(group = "net.java.dev.jna")
    }
    // JNA AAR for Android runtime
    implementation("net.java.dev.jna:jna:5.14.0@aar")
    // Android-specific coroutines dispatcher
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-android:1.8.1")

    if (!localNativeAar.isPresent) {
        implementation("ai.nobodywho:nobodywho-uniffi-android:$nobodywhoNativeVersion")
    }
}

tasks.named("preBuild") {
    dependsOn(extractLocalNativeAar)
}

publishing {
    publications {
        register<MavenPublication>("release") {
            artifactId = "nobodywho-android"

            afterEvaluate {
                from(components["release"])
            }
        }
    }
}
