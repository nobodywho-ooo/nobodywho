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
    val includeCxxRuntime: Boolean,
)

val nativeArtifacts = listOf(
    NativeArtifact(
        taskPrefix = "flutter",
        artifactId = "nobodywho-flutter-android",
        mainLibrary = "libnobodywho_flutter.so",
        includeCxxRuntime = true,
    ),
    NativeArtifact(
        taskPrefix = "uniffi",
        artifactId = "nobodywho-uniffi-android",
        mainLibrary = "libnobodywho_uniffi.so",
        includeCxxRuntime = false,
    ),
)

fun String.capitalized() = replaceFirstChar { it.uppercase() }

val sourceArchive by tasks.registering(Jar::class) {
    archiveClassifier.set("sources")
    from("README.md", "version.txt")
    isPreserveFileTimestamps = false
    isReproducibleFileOrder = true
}

val documentationArchive by tasks.registering(Jar::class) {
    archiveClassifier.set("javadoc")
    from("README.md")
    isPreserveFileTimestamps = false
    isReproducibleFileOrder = true
}

val validationTasks = nativeArtifacts.associateWith { artifact ->
    tasks.register("validate${artifact.taskPrefix.capitalized()}NativeLibraries") {
        val jniRoot = layout.buildDirectory.dir("${artifact.taskPrefix}/jniLibs")
        inputs.dir(jniRoot)

        doLast {
            supportedAbis.forEach { abi ->
                val abiDir = jniRoot.get().dir(abi).asFile
                require(abiDir.isDirectory) { "Missing $abiDir" }

                val required = buildList {
                    add(artifact.mainLibrary)
                    addAll(commonLibraries)
                    if (artifact.includeCxxRuntime) add("libc++_shared.so")
                    if (abi == "x86_64") add("libonnxruntime.so")
                }
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

                if (!artifact.includeCxxRuntime) {
                    require(!abiDir.resolve("libc++_shared.so").exists()) {
                        "${artifact.artifactId} must use the consumer's libc++_shared.so"
                    }
                }
            }
        }
    }
}

val aarTasks = nativeArtifacts.associateWith { artifact ->
    tasks.register<Zip>("${artifact.taskPrefix}Aar") {
        dependsOn(validationTasks.getValue(artifact))
        archiveFileName.set("${artifact.artifactId}-${project.version}.aar")
        destinationDirectory.set(layout.buildDirectory.dir("outputs"))
        duplicatesStrategy = DuplicatesStrategy.FAIL
        isPreserveFileTimestamps = false
        isReproducibleFileOrder = true

        from("${artifact.taskPrefix}/AndroidManifest.xml") {
            rename { "AndroidManifest.xml" }
        }
        into("jni") {
            from(layout.buildDirectory.dir("${artifact.taskPrefix}/jniLibs"))
        }
    }
}

tasks.assemble {
    dependsOn(aarTasks.values)
}

tasks.check {
    dependsOn(validationTasks.values)
}

publishing {
    publications {
        nativeArtifacts.forEach { artifact ->
            register<MavenPublication>(artifact.taskPrefix) {
                artifactId = artifact.artifactId
                artifact(aarTasks.getValue(artifact)) {
                    extension = "aar"
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
        useInMemoryPgpKeys(signingKey, signingPassword.orEmpty())
        sign(extensions.getByType<org.gradle.api.publish.PublishingExtension>().publications)
    }
}
