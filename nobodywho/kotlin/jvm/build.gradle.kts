plugins {
    id("org.jetbrains.kotlin.jvm")
    id("maven-publish")
    signing
}

java {
    sourceCompatibility = JavaVersion.VERSION_11
    targetCompatibility = JavaVersion.VERSION_11
}

kotlin {
    jvmToolchain(17)
    compilerOptions {
        jvmTarget.set(org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_11)
    }
}

dependencies {
    api(project(":nobodywho-core"))
}

// Packaging smoke test: a pure-FFI call off the built JAR with libs resolved as JAR
// resources and NO jna.library.path shim — the way a real consumer loads it. Unlike the
// co-located unit tests, this forces NativeLoader to stage the siblings itself, so a broken
// bundle fails CI instead of a downstream release. The entry point sits in its own source
// set (not published), and the classpath excludes :jvm's exploded main output so
// getResource can only resolve the native libs from the JAR.
val smoke = sourceSets.create("smoke") {
    compileClasspath += configurations.runtimeClasspath.get()
    runtimeClasspath += output + compileClasspath
}

val jvmSmoke by tasks.registering(JavaExec::class) {
    dependsOn(tasks.named("jar"))
    mainClass.set("ai.nobodywho.SmokeKt")
    classpath = smoke.output +
        files(tasks.named<Jar>("jar").flatMap { it.archiveFile }) +
        configurations.runtimeClasspath.get()
}

// Native libs are placed in src/main/resources/ following JNA's expected layout:
//   linux-x86-64/libnobodywho_uniffi.so   (+ co-located libggml*/libllama* siblings)
//   linux-aarch64/libnobodywho_uniffi.so
//   darwin-x86-64/libnobodywho_uniffi.dylib
//   darwin-aarch64/libnobodywho_uniffi.dylib
//   win32-x86-64/nobodywho_uniffi.dll
// JNA won't extract the ggml/llama siblings on its own, so the `common` NativeLoader
// stages the whole resource dir to a temp dir and points jna.library.path at it. See NativeLoader.kt.

val sourcesJar by tasks.registering(Jar::class) {
    archiveClassifier.set("sources")
}

val javadocJar by tasks.registering(Jar::class) {
    archiveClassifier.set("javadoc")
}

publishing {
    publications {
        register<MavenPublication>("release") {
            groupId = "ai.nobodywho"
            artifactId = "nobodywho"
            version = project.version.toString()

            from(components["java"])
            artifact(sourcesJar)
            artifact(javadocJar)
        }
    }
}
