import org.gradle.api.file.DuplicatesStrategy
import org.gradle.api.publish.maven.MavenPublication
import org.gradle.api.tasks.bundling.Jar
import org.gradle.api.tasks.bundling.Zip
import org.gradle.plugins.signing.SigningExtension

plugins {
    base
    `maven-publish`
    signing
}

group = "ai.nobodywho"
version = providers.gradleProperty("version").getOrElse("0.0.0-local")

val abiTargets = mapOf(
    "arm64-v8a" to "aarch64-linux-android",
    "x86_64" to "x86_64-linux-android",
)
val commonLibraries = listOf(
    "libggml-base.so",
    "libggml.so",
    "libllama-common.so",
    "libllama.so",
    "libc++_shared.so",
)

data class NativeArtifact(
    val binding: String,
    val integration: String,
    val artifactId: String,
    val mainLibrary: String,
)

val nativeArtifact = when (val binding = providers.gradleProperty("nobodywhoBinding").getOrElse("uniffi")) {
    "flutter" -> NativeArtifact(
        binding = binding,
        integration = "flutter",
        artifactId = "nobodywho-flutter-android",
        mainLibrary = "libnobodywho_flutter.so",
    )
    "react-native" -> NativeArtifact(
        binding = binding,
        integration = "uniffi",
        artifactId = "nobodywho-react-native-android",
        mainLibrary = "libnobodywho_uniffi.so",
    )
    "uniffi" -> NativeArtifact(
        binding = binding,
        integration = "uniffi",
        artifactId = "nobodywho-uniffi-android",
        mainLibrary = "libnobodywho_uniffi.so",
    )
    else -> error("Unknown nobodywhoBinding '$binding'; expected flutter, react-native, or uniffi")
}

// Cargo's Android outputs in the layout build.yml uploads (see README):
//   libnobodywho-<integration>-<target>-release.so
//   nobodywho-runtime-<target>/*.so      common libs, CPU backends, libc++_shared.so
//   libonnxruntime.so                    x86_64 only
val inputDir = providers.gradleProperty("nobodywhoInputDir").map(::file)
    .getOrElse(layout.buildDirectory.dir("inputs/${nativeArtifact.integration}").get().asFile)
val prebuiltAar = providers.gradleProperty("nobodywhoPrebuiltAar").orNull?.let(::file)

fun entryLibrary(target: String) = inputDir.resolve("libnobodywho-${nativeArtifact.integration}-$target-release.so")
fun runtimeDir(target: String) = inputDir.resolve("nobodywho-runtime-$target")
val onnxRuntime = inputDir.resolve("libonnxruntime.so")

fun validateInputs() {
    abiTargets.forEach { (abi, target) ->
        require(entryLibrary(target).isFile) { "${nativeArtifact.artifactId}: missing ${entryLibrary(target)}" }
        val runtime = runtimeDir(target)
        commonLibraries.forEach { library ->
            require(runtime.resolve(library).isFile) { "${nativeArtifact.artifactId}: missing $library in $runtime" }
        }
        require(runtime.listFiles().orEmpty().any { it.name.startsWith("libggml-cpu") && it.extension == "so" }) {
            "${nativeArtifact.artifactId}: no GGML CPU backend for $abi in $runtime"
        }
    }
    require(onnxRuntime.isFile) { "${nativeArtifact.artifactId}: missing $onnxRuntime (x86_64 links ONNX Runtime dynamically)" }
}

val sourceArchive by tasks.registering(Jar::class) {
    archiveClassifier.set("sources")
    from("README.md")
    isPreserveFileTimestamps = false
    isReproducibleFileOrder = true
}

val documentationArchive by tasks.registering(Jar::class) {
    archiveClassifier.set("javadoc")
    from("README.md")
    isPreserveFileTimestamps = false
    isReproducibleFileOrder = true
}

val nativeAar by tasks.registering(Zip::class) {
    archiveFileName.set("${nativeArtifact.artifactId}-${project.version}.aar")
    destinationDirectory.set(layout.buildDirectory.dir("outputs"))
    duplicatesStrategy = DuplicatesStrategy.FAIL
    isPreserveFileTimestamps = false
    isReproducibleFileOrder = true

    doFirst { validateInputs() }
    from("${nativeArtifact.integration}/AndroidManifest.xml") {
        rename { "AndroidManifest.xml" }
    }
    abiTargets.forEach { (abi, target) ->
        into("jni/$abi") {
            from(entryLibrary(target)) { rename { nativeArtifact.mainLibrary } }
            from(runtimeDir(target)) { include("*.so") }
            if (abi == "x86_64") from(onnxRuntime)
        }
    }
}

tasks.assemble {
    dependsOn(nativeAar)
}

tasks.check {
    dependsOn(nativeAar)
}

publishing {
    publications {
        register<MavenPublication>("native") {
            artifactId = nativeArtifact.artifactId
            if (prebuiltAar == null) {
                artifact(nativeAar) {
                    extension = "aar"
                }
            } else {
                require(prebuiltAar.isFile) { "Missing prebuilt AAR: $prebuiltAar" }
                artifact(prebuiltAar) {
                    extension = "aar"
                }
            }
            artifact(sourceArchive)
            artifact(documentationArchive)

            pom {
                name.set(nativeArtifact.artifactId)
                description.set("NobodyWho native Android libraries for ${nativeArtifact.binding}")
                url.set("https://github.com/nobodywho-ooo/nobodywho")
                licenses {
                    license {
                        name.set("EUPL-1.2")
                        url.set("https://joinup.ec.europa.eu/collection/eupl/eupl-text-eupl-12")
                    }
                }
                developers {
                    developer {
                        id.set("nobodywho")
                        name.set("NobodyWho")
                        email.set("services@nobodywho.ooo")
                    }
                }
                scm {
                    connection.set("scm:git:git://github.com/nobodywho-ooo/nobodywho.git")
                    developerConnection.set("scm:git:ssh://github.com/nobodywho-ooo/nobodywho.git")
                    url.set("https://github.com/nobodywho-ooo/nobodywho")
                }
            }
        }
    }

    providers.gradleProperty("candidateRepository").orNull?.let { repositoryPath ->
        repositories.maven {
            name = "Candidate"
            url = uri(repositoryPath)
        }
    }
}

extensions.configure<SigningExtension> {
    val signingKey = providers.environmentVariable("SIGNING_KEY").orNull
    val signingPassword = providers.environmentVariable("SIGNING_PASSWORD").orNull
    if (signingKey != null) {
        requireNotNull(signingPassword) {
            "SIGNING_PASSWORD must be set when SIGNING_KEY is set"
        }
        useInMemoryPgpKeys(signingKey, signingPassword)
        sign(extensions.getByType<org.gradle.api.publish.PublishingExtension>().publications)
    }
}
