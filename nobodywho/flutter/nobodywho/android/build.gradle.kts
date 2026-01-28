// NobodyWho Flutter Plugin - Android Build Configuration
//
// This build script resolves prebuilt native libraries using a Dart script
// that supports multiple resolution strategies:
// 1. Environment variable override (NOBODYWHO_FLUTTER_LIB_PATH)
// 2. Local cargo build detection
// 3. Cached download
// 4. Download from GitHub releases

import java.io.ByteArrayOutputStream

plugins {
    id("com.android.library")
}

group = "ooo.nobodywho.nobodywho"
version = "1.0"

// Supported ABIs (32-bit not supported due to llama.cpp build issues)
val targetAbis = listOf("arm64-v8a", "x86_64")

// Map Android ABI to NDK triple (for finding libc++_shared.so)
val abiToNdkTriple = mapOf(
    "arm64-v8a" to "aarch64-linux-android",
    "x86_64" to "x86_64-linux-android"
)

android {
    namespace = "ooo.nobodywho.nobodywho"
    compileSdk = 36

    // NDK version can be configured by downstream apps via gradle.properties
    findProperty("android.ndkVersion")?.let { ndkVersion = it.toString() }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_11
        targetCompatibility = JavaVersion.VERSION_11
    }

    defaultConfig {
        minSdk = 24
        ndk {
            abiFilters += targetAbis
        }
    }

    // Point jniLibs to our build output directory instead of src/main/jniLibs
    // This ensures all artifacts are in cleanable locations
    sourceSets {
        getByName("main") {
            jniLibs.srcDirs(layout.buildDirectory.dir("jniLibs"))
        }
    }
}

// Task to resolve and copy native libraries for all ABIs
val resolveNativeLibraries by tasks.registering {
    description = "Resolves NobodyWho native libraries using the Dart resolution script"

    val jniLibsDir = layout.buildDirectory.dir("jniLibs")
    val cacheDir = layout.buildDirectory.dir("nobodywho-cache")

    outputs.dir(jniLibsDir)

    doLast {
        val toolDir = file("${projectDir}/../tool")
        val workingDir = file("${projectDir}/..")

        targetAbis.forEach { abi ->
            val abiOutputDir = jniLibsDir.get().dir(abi).asFile
            abiOutputDir.mkdirs()

            // Run the Dart script to resolve the library path
            val stdout = ByteArrayOutputStream()
            val stderr = ByteArrayOutputStream()

            val execResult = project.exec {
                commandLine(
                    "dart", "run", "${toolDir}/resolve_binary.dart",
                    "--platform=android",
                    "--arch=$abi",
                    "--build-type=release",
                    "--cache-dir=${cacheDir.get().asFile.absolutePath}"
                )
                setWorkingDir(workingDir)
                standardOutput = stdout
                errorOutput = stderr
                isIgnoreExitValue = true
            }

            // Log stderr (contains status messages like "Using cached library...")
            val stderrText = stderr.toString().trim()
            if (stderrText.isNotEmpty()) {
                logger.lifecycle("[$abi] $stderrText")
            }

            if (execResult.exitValue != 0) {
                throw GradleException("Failed to resolve NobodyWho library for $abi:\n$stderrText")
            }

            val resolvedLibPath = stdout.toString().trim()
            logger.lifecycle("[$abi] Resolved library: $resolvedLibPath")

            // Copy the resolved library to jniLibs
            copy {
                from(resolvedLibPath)
                into(abiOutputDir)
                rename { "libnobodywho_flutter.so" }
            }

            // Copy libc++_shared.so from NDK
            val ndkDir = android.ndkDirectory
            val ndkTriple = abiToNdkTriple[abi]
                ?: throw GradleException("Unknown ABI: $abi")

            // Find the prebuilt directory (works on any host platform)
            val prebuiltDir = file("${ndkDir}/toolchains/llvm/prebuilt")
                .listFiles()
                ?.firstOrNull { it.isDirectory }
                ?: throw GradleException("Could not find NDK prebuilt directory")

            val libcxxShared = file("${prebuiltDir}/sysroot/usr/lib/${ndkTriple}/libc++_shared.so")

            if (libcxxShared.exists()) {
                logger.lifecycle("[$abi] Copying libc++_shared.so")
                copy {
                    from(libcxxShared)
                    into(abiOutputDir)
                }
            } else {
                logger.warn("[$abi] libc++_shared.so not found at: ${libcxxShared.absolutePath}")
            }
        }
    }
}

// Ensure native libraries are resolved before they're needed for packaging
// This hooks into the Android Gradle Plugin's build lifecycle
afterEvaluate {
    tasks.matching {
        it.name.contains("merge") && it.name.contains("JniLibFolders")
    }.configureEach {
        dependsOn(resolveNativeLibraries)
    }
}
