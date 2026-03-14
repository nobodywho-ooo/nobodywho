plugins {
    id("com.android.library")
    id("org.jetbrains.kotlin.android")
    id("maven-publish")
}

android {
    namespace = "com.nobodywho"
    compileSdk = 35

    defaultConfig {
        minSdk = 24
        consumerProguardFiles("consumer-rules.pro")
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_1_8
        targetCompatibility = JavaVersion.VERSION_1_8
    }

    kotlinOptions {
        jvmTarget = "1.8"
    }

    sourceSets {
        getByName("main") {
            jniLibs.srcDirs("src/main/jniLibs")
        }
    }
}

dependencies {
    // UniFFI Kotlin bindings use JNA for FFI calls
    implementation("net.java.dev.jna:jna:5.14.0@aar")
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-android:1.8.1")
}

// MARK: - Native library download

val soVersion = project.findProperty("VERSION_NAME")?.toString() ?: "1.0.0"
val githubRepo = project.findProperty("NOBODYWHO_GITHUB_REPO")?.toString()
    ?: "Intiserahmed/nobodywho"

// ABI → release asset filename
val abiAssets = mapOf(
    "arm64-v8a" to "libuniffi_nobodywho-arm64-v8a.so",
    "x86_64"    to "libuniffi_nobodywho-x86_64.so"
)

val downloadNativeLibs by tasks.registering {
    description = "Downloads pre-built native libraries from GitHub Releases if not already present."

    val missingAbis = abiAssets.filter { (abi, _) ->
        !layout.projectDirectory.file("src/main/jniLibs/$abi/libuniffi_nobodywho.so").asFile.exists()
    }
    onlyIf { missingAbis.isNotEmpty() }

    doLast {
        missingAbis.forEach { (abi, assetName) ->
            val destDir = layout.projectDirectory.dir("src/main/jniLibs/$abi").asFile
            destDir.mkdirs()
            val url = "https://github.com/$githubRepo/releases/download/" +
                      "nobodywho-android-v$soVersion/$assetName"
            logger.lifecycle("Downloading $abi native library v$soVersion...")
            logger.lifecycle("  $url")
            uri(url).toURL().openStream().use { input ->
                File(destDir, "libuniffi_nobodywho.so").outputStream()
                    .use { output -> input.copyTo(output) }
            }
            logger.lifecycle("  ✓ libuniffi_nobodywho.so ($abi)")
        }
    }
}

tasks.named("preBuild") { dependsOn(downloadNativeLibs) }

afterEvaluate {
    publishing {
        publications {
            create<MavenPublication>("release") {
                from(components["release"])
                groupId = "com.nobodywho"
                artifactId = "android"
                version = project.findProperty("VERSION_NAME")?.toString() ?: "1.0.0"

                pom {
                    name.set("NobodyWho Android")
                    description.set("On-device LLM inference for Android via llama.cpp")
                    url.set("https://github.com/nobodywho-ooo/nobodywho")
                    licenses {
                        license {
                            name.set("MIT License")
                            url.set("https://opensource.org/licenses/MIT")
                        }
                    }
                }
            }
        }
    }
}
