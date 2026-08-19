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

// Load from the built JAR, not Gradle's exploded classes and resources.
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
