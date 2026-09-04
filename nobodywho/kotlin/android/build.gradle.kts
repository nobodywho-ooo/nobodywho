plugins {
    id("com.android.library")
    id("org.jetbrains.kotlin.android")
    id("maven-publish")
    signing
}

// The published nobodywho-android AAR embeds the native libraries. Building it
// requires the local UniFFI AAR produced by ../../android (see its README).
val localNativeAar = providers.gradleProperty("nobodywhoNativeAar")
    .orElse(providers.environmentVariable("NOBODYWHO_UNIFFI_ANDROID_AAR"))
val localNativeRoot = layout.buildDirectory.dir("localNativeAar")
val extractLocalNativeAar by tasks.registering(Sync::class) {
    doFirst {
        require(localNativeAar.isPresent) {
            "Building the Kotlin Android binding requires -PnobodywhoNativeAar=<path> " +
                "or NOBODYWHO_UNIFFI_ANDROID_AAR. Published nobodywho-android AARs already contain these libraries."
        }
    }
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
