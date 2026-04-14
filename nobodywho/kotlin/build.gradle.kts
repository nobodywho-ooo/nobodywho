plugins {
    id("com.android.library")
    id("org.jetbrains.kotlin.android")
    id("maven-publish")
}

group = "ooo.nobodywho"
version = "0.1.0"

android {
    namespace = "ooo.nobodywho"
    compileSdk = 36

    defaultConfig {
        minSdk = 26
        ndk {
            abiFilters += listOf("arm64-v8a", "x86_64")
        }
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
            // Include both wrapper and generated Kotlin sources
            kotlin.srcDirs("src", "generated")
            // Native shared libraries resolved by CI or local build
            jniLibs.srcDirs(layout.buildDirectory.dir("jniLibs"))
        }
    }
}

dependencies {
    // JNA is required by UniFFI-generated Kotlin bindings
    implementation("net.java.dev.jna:jna:5.14.0@aar")
    // Coroutines for suspend functions and Flow
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-core:1.8.1")
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-android:1.8.1")
    // Reflection for Tool function introspection
    implementation(kotlin("reflect"))
}

publishing {
    publications {
        register<MavenPublication>("release") {
            groupId = "ooo.nobodywho"
            artifactId = "nobodywho"
            version = project.version.toString()

            afterEvaluate {
                from(components["release"])
            }
        }
    }
}
