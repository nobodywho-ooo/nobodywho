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

sourceSets {
    main {
        kotlin.srcDirs("src", "generated")
    }
    test {
        kotlin.srcDirs("test")
    }
}

dependencies {
    // JNA is required by UniFFI-generated Kotlin bindings
    implementation("net.java.dev.jna:jna:5.14.0")
    // Coroutines for suspend functions and Flow
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-core:1.8.1")
    // Reflection for Tool function introspection
    implementation(kotlin("reflect"))
    // JSON library
    implementation("org.json:json:20240303")

    // Tests
    testImplementation("junit:junit:4.13.2")
}

tasks.withType<Test> {
    val libDir = System.getenv("NOBODYWHO_LIB_DIR") ?: ""
    systemProperty("java.library.path", libDir)
    systemProperty("jna.library.path", libDir)
}

publishing {
    publications {
        create<MavenPublication>("release") {
            groupId = "ai.nobodywho"
            artifactId = "nobodywho-core"
            version = project.version.toString()
            from(components["java"])
        }
    }
}
