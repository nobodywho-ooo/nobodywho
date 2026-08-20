// NobodyWho Flutter Plugin - Android Build Configuration
//
// This build script resolves prebuilt native libraries using a Dart script
// that supports multiple resolution strategies:
// 1. Environment variable override (NOBODYWHO_FLUTTER_LIB_PATH)
// 2. Local cargo build detection
// 3. Cached download
// 4. Download from GitHub releases

import java.io.ByteArrayOutputStream
import org.gradle.process.ExecOperations
import org.gradle.kotlin.dsl.support.serviceOf

plugins {
    id("com.android.library")
}

group = "ooo.nobodywho.nobodywho"
version = "1.0"

// Supported ABIs (32-bit not supported due to llama.cpp build issues)
val supportedAbis = listOf("arm64-v8a", "x86_64")

// `flutter build apk --target-platform android-arm64` passes
// -Ptarget-platform=android-arm64 (comma-separated for multiple) on every
// Gradle invocation it drives — restrict to just the ABIs actually requested
// instead of always resolving all of them, e.g. so a CI job building only
// arm64-v8a locally never falls through to downloading x86_64.
private val flutterPlatformToAbi = mapOf(
    "android-arm64" to "arm64-v8a",
    "android-x64" to "x86_64",
)
val targetAbis = (findProperty("target-platform") as String?)
    ?.split(",")
    ?.mapNotNull { flutterPlatformToAbi[it.trim()] }
    ?.filter { it in supportedAbis }
    ?.takeIf { it.isNotEmpty() }
    ?: supportedAbis

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

    // Declare inputs so Gradle re-runs this task when the version or resolve logic changes.
    // Without these, Gradle considers the task UP-TO-DATE after the first run, causing stale
    // .so files from a previous plugin version to persist across upgrades.
    inputs.file("${projectDir}/../pubspec.yaml")
    inputs.file("${projectDir}/../tool/resolve_binary.dart")
    outputs.dir(jniLibsDir)

    // Capture the ExecOperations service at configuration time. Project.exec() was
    // deprecated in Gradle 8.x and removed in Gradle 9.0, so we inject the service
    // instead of calling project.exec { } during task execution (see issue #624).
    val execOperations = serviceOf<ExecOperations>()

    doLast {
        val toolDir = file("${projectDir}/../tool")
        val workingDir = file("${projectDir}/..")

        // Runs resolve_binary.dart for the given ABI/component and returns the resolved path.
        fun resolveLibrary(abi: String, component: String): String {
            val stdout = ByteArrayOutputStream()
            val stderr = ByteArrayOutputStream()

            val execResult = execOperations.exec {
                commandLine(
                    "dart", "run", "${toolDir}/resolve_binary.dart",
                    "--platform=android",
                    "--arch=$abi",
                    "--build-type=release",
                    "--cache-dir=${cacheDir.get().asFile.absolutePath}",
                    "--component=$component"
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
                throw GradleException("Failed to resolve $component library for $abi:\n$stderrText")
            }

            return stdout.toString().trim()
        }

        targetAbis.forEach { abi ->
            val abiOutputDir = jniLibsDir.get().dir(abi).asFile
            abiOutputDir.mkdirs()

            // Copy the resolved library to jniLibs
            val resolvedLibPath = resolveLibrary(abi, "main")
            logger.lifecycle("[$abi] Resolved library: $resolvedLibPath")
            copy {
                from(resolvedLibPath)
                into(abiOutputDir)
                rename { "libnobodywho_flutter.so" }
            }

            // Only x86_64 needs onnxruntime as a separate .so (Microsoft ships
            // no static build for it); arm64 statically embeds it (see objdump -p).
            if (abi == "x86_64") {
                val resolvedOrtPath = resolveLibrary(abi, "onnxruntime")
                logger.lifecycle("[$abi] Resolved onnxruntime library: $resolvedOrtPath")
                copy {
                    from(resolvedOrtPath)
                    into(abiOutputDir)
                    rename { "libonnxruntime.so" }
                }
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
