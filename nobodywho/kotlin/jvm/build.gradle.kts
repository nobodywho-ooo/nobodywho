plugins {
    id("org.jetbrains.kotlin.jvm")
    id("maven-publish")
}

kotlin {
    jvmToolchain(17)
    compilerOptions {
        jvmTarget.set(org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_11)
    }
}

dependencies {
    api(project(":common"))
}

// Native libs are placed in src/main/resources/ following JNA's expected layout:
//   linux-x86-64/libnobodywho_uniffi.so
//   linux-aarch64/libnobodywho_uniffi.so
//   darwin-x86-64/libnobodywho_uniffi.dylib
//   darwin-aarch64/libnobodywho_uniffi.dylib
//   win32-x86-64/nobodywho_uniffi.dll
// JNA automatically extracts and loads the correct one at runtime.

publishing {
    publications {
        register<MavenPublication>("release") {
            groupId = "ai.nobodywho"
            artifactId = "nobodywho"
            version = project.version.toString()

            from(components["java"])
        }
    }
}
