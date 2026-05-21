plugins {
    id("com.android.library")
    id("org.jetbrains.kotlin.android")
    id("maven-publish")
    signing
}

android {
    namespace = "ai.nobodywho"
    compileSdk = 35

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
            // Native shared libraries resolved by CI or local build
            jniLibs.srcDirs(layout.buildDirectory.dir("jniLibs"))
        }
    }
}

dependencies {
    // Exclude the JNA JAR from :common — Android needs the AAR variant instead
    api(project(":common")) {
        exclude(group = "net.java.dev.jna")
    }
    // JNA AAR for Android runtime
    implementation("net.java.dev.jna:jna:5.14.0@aar")
    // Android-specific coroutines dispatcher
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-android:1.8.1")
}

publishing {
    publications {
        register<MavenPublication>("release") {
            groupId = "ai.nobodywho"
            artifactId = "nobodywho-android"
            version = project.version.toString()

            afterEvaluate {
                from(components["release"])
            }
        }
    }
}
