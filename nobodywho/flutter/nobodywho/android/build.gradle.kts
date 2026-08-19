import java.io.ByteArrayOutputStream
import org.gradle.process.ExecOperations
import org.gradle.kotlin.dsl.support.serviceOf

plugins {
    id("com.android.library")
}

group = "ooo.nobodywho.nobodywho"
version = "1.0"

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

    sourceSets {
        getByName("main") {
            jniLibs.srcDirs(layout.buildDirectory.dir("jniLibs"))
        }
    }
}

val resolveNativeLibraries by tasks.registering {
    description = "Resolves NobodyWho native libraries using the Dart resolution script"

    val jniLibsDir = layout.buildDirectory.dir("jniLibs")
    val cacheDir = layout.buildDirectory.dir("nobodywho-cache")

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

        // libnobodywho_flutter.so dynamically needs libc++_shared.so at runtime
        // (android-static-stdcxx doesn't fully embed it - confirmed via `objdump -p`).
        fun copyLibcxxShared(abi: String, abiOutputDir: File) {
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
                throw GradleException("libc++_shared.so not found at: ${libcxxShared.absolutePath}")
            }
        }

        targetAbis.forEach { abi ->
            val abiOutputDir = jniLibsDir.get().dir(abi).asFile
            // Wipe first: a stale .so from an earlier runtime set would otherwise linger
            // in jniLibs and get packaged alongside the current one.
            abiOutputDir.deleteRecursively()
            abiOutputDir.mkdirs()

            // resolve_binary.dart already yields a file named libnobodywho_flutter.so,
            // so no rename is needed here.
            val resolvedLibPath = resolveLibrary(abi, "main")
            logger.lifecycle("[$abi] Resolved library: $resolvedLibPath")
            copy {
                from(resolvedLibPath)
                into(abiOutputDir)
            }

            copyLibcxxShared(abi, abiOutputDir)

            // The dlopen'd ggml/llama libs ship in a nobodywho-runtime/ subdir next to the
            // binding; flatten them into jniLibs/<abi>, where the Android loader resolves
            // NEEDED libs.
            copy {
                from(File(File(resolvedLibPath).parentFile, "nobodywho-runtime")) {
                    include("*.so")
                }
                from(File(resolvedLibPath).parentFile) {
                    include("libonnxruntime.so")
                }
                into(abiOutputDir)
            }

            // Only x86_64 needs onnxruntime as a separate .so (Microsoft ships
            // no static build for it); arm64 statically embeds it (see objdump -p).
            // Runs after the copy above: this one comes from the Maven AAR (a separate
            // cache dir, not the resolved binding's dir), so it is authoritative.
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

tasks.configureEach {
    if (name.contains("merge") && name.contains("JniLibFolders")) {
        dependsOn(resolveNativeLibraries)
    }
}
