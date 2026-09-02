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

val supportedAbis = listOf("arm64-v8a", "x86_64")
val commonLibraries = listOf(
    "libggml-base.so",
    "libggml.so",
    "libllama-common.so",
    "libllama.so",
)

data class NativeArtifact(
    val binding: String,
    val jniDirectory: String,
    val artifactId: String,
    val mainLibrary: String,
)

val nativeArtifact = when (val binding = providers.gradleProperty("nobodywhoBinding").getOrElse("uniffi")) {
    "flutter" -> NativeArtifact(
        binding = binding,
        jniDirectory = "flutter",
        artifactId = "nobodywho-flutter-android",
        mainLibrary = "libnobodywho_flutter.so",
    )
    "react-native" -> NativeArtifact(
        binding = binding,
        jniDirectory = "uniffi",
        artifactId = "nobodywho-react-native-android",
        mainLibrary = "libnobodywho_uniffi.so",
    )
    "uniffi" -> NativeArtifact(
        binding = binding,
        jniDirectory = "uniffi",
        artifactId = "nobodywho-uniffi-android",
        mainLibrary = "libnobodywho_uniffi.so",
    )
    else -> error("Unknown nobodywhoBinding '$binding'; expected flutter, react-native, or uniffi")
}
val prebuiltAar = providers.gradleProperty("nobodywhoPrebuiltAar").orNull?.let(::file)

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

fun validateNativeLibraries(artifact: NativeArtifact, jniRoot: File) {
    supportedAbis.forEach { abi ->
        val abiDir = jniRoot.resolve(abi)
        require(abiDir.isDirectory) { "Missing $abiDir" }

        val required = commonLibraries + artifact.mainLibrary + "libc++_shared.so" +
            if (abi == "x86_64") listOf("libonnxruntime.so") else emptyList()
        required.forEach { library ->
            require(abiDir.resolve(library).isFile) {
                "${artifact.artifactId} is missing $abi/$library"
            }
        }

        require(abiDir.listFiles().orEmpty().any {
            it.name.startsWith("libggml-cpu") && it.extension == "so"
        }) {
            "${artifact.artifactId} has no CPU backend for $abi"
        }
    }
}

val jniRoot = layout.buildDirectory.dir("${nativeArtifact.jniDirectory}/jniLibs")
val nativeAar by tasks.registering(Zip::class) {
    archiveFileName.set("${nativeArtifact.artifactId}-${project.version}.aar")
    destinationDirectory.set(layout.buildDirectory.dir("outputs"))
    duplicatesStrategy = DuplicatesStrategy.FAIL
    isPreserveFileTimestamps = false
    isReproducibleFileOrder = true

    doFirst {
        validateNativeLibraries(nativeArtifact, jniRoot.get().asFile)
    }
    from("${nativeArtifact.jniDirectory}/AndroidManifest.xml") {
        rename { "AndroidManifest.xml" }
    }
    into("jni") {
        from(jniRoot)
        include(supportedAbis.map { "$it/**" })
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
