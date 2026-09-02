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

// Real releases pass -Pversion= (derived from the nobodywho-android-vX.Y.Z tag).
// Local/CI dev builds fall back to this — never a version other tooling depends on.
version = providers.gradleProperty("version").getOrElse("0.0.0-local")

val supportedAbis = listOf("arm64-v8a", "x86_64")
val commonLibraries = listOf(
    "libggml-base.so",
    "libggml.so",
    "libllama-common.so",
    "libllama.so",
)

data class NativeArtifact(
    val taskPrefix: String,
    val artifactId: String,
    val mainLibrary: String,
)

val nativeArtifacts = listOf(
    NativeArtifact(
        taskPrefix = "flutter",
        artifactId = "nobodywho-flutter-android",
        mainLibrary = "libnobodywho_flutter.so",
    ),
    NativeArtifact(
        taskPrefix = "uniffi",
        artifactId = "nobodywho-uniffi-android",
        mainLibrary = "libnobodywho_uniffi.so",
    ),
)
val prebuiltAarDirectory = providers.gradleProperty("nobodywhoPrebuiltAarDir")
    .orNull
    ?.let(::file)

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

val aarTasks = nativeArtifacts.associateWith { artifact ->
    val jniRoot = layout.buildDirectory.dir("${artifact.taskPrefix}/jniLibs")
    tasks.register<Zip>("${artifact.taskPrefix}Aar") {
        archiveFileName.set("${artifact.artifactId}-${project.version}.aar")
        destinationDirectory.set(layout.buildDirectory.dir("outputs"))
        duplicatesStrategy = DuplicatesStrategy.FAIL
        isPreserveFileTimestamps = false
        isReproducibleFileOrder = true

        doFirst {
            validateNativeLibraries(artifact, jniRoot.get().asFile)
        }
        from("${artifact.taskPrefix}/AndroidManifest.xml") {
            rename { "AndroidManifest.xml" }
        }
        into("jni") {
            from(jniRoot)
            include(supportedAbis.map { "$it/**" })
        }
    }
}

tasks.assemble {
    dependsOn(aarTasks.values)
}

tasks.check {
    dependsOn(aarTasks.values)
}

publishing {
    publications {
        nativeArtifacts.forEach { artifact ->
            register<MavenPublication>(artifact.taskPrefix) {
                artifactId = artifact.artifactId
                val prebuiltAar = prebuiltAarDirectory?.resolve(
                    "${artifact.artifactId}-${project.version}.aar",
                )
                if (prebuiltAar == null) {
                    artifact(aarTasks.getValue(artifact)) {
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
                    name.set(artifact.artifactId)
                    description.set("NobodyWho native Android libraries for ${artifact.taskPrefix}")
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
