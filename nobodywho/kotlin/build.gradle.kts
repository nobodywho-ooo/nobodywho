plugins {
    id("com.android.library")
    id("org.jetbrains.kotlin.android")
    id("maven-publish")
}

group = "ai.nobodywho"
version = "0.1.0"

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
            // Include both wrapper and generated Kotlin sources
            kotlin.srcDirs("src", "generated")
            // Native shared libraries resolved by CI or local build
            jniLibs.srcDirs(layout.buildDirectory.dir("jniLibs"))
        }
        getByName("test") {
            kotlin.srcDirs("test")
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
    // JSON library (provided by Android runtime, needed explicitly for unit tests)
    implementation("org.json:json:20240303")

    // Unit tests (JNA jar for host JVM, not @aar)
    testImplementation("junit:junit:4.13.2")
    testImplementation("net.java.dev.jna:jna:5.14.0")
}

tasks.withType<Test> {
    val libDir = System.getenv("NOBODYWHO_LIB_DIR") ?: ""
    systemProperty("java.library.path", libDir)
    systemProperty("jna.library.path", libDir)
}

publishing {
    publications {
        register<MavenPublication>("release") {
            groupId = "ai.nobodywho"
            artifactId = "nobodywho"
            version = project.version.toString()

            afterEvaluate {
                from(components["release"])
            }
        }
    }
}
