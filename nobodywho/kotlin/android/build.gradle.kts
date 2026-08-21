plugins {
    id("com.android.library")
    id("org.jetbrains.kotlin.android")
    id("maven-publish")
    signing
}

// Pinned independently of this module's own version — bump deliberately to adopt
// a newer nobodywho-android-vX.Y.Z release.
val nobodywhoNativeVersion = "2.5.0"
val localNativeAar = providers.gradleProperty("nobodywhoNativeAar").orNull
    ?: System.getenv("NOBODYWHO_UNIFFI_ANDROID_AAR")
val localNativeRoot = layout.buildDirectory.dir("localNativeAar")
val extractLocalNativeAar = localNativeAar?.let { path ->
    tasks.register<Sync>("extractLocalNativeAar") {
        val aar = file(path)
        require(aar.isFile) {
            "NOBODYWHO_UNIFFI_ANDROID_AAR does not exist: ${aar.absolutePath}"
        }
        from(zipTree(aar)) {
            include("jni/**/*.so")
        }
        into(localNativeRoot)
    }
}

android {
    namespace = "ai.nobodywho"
    compileSdk = 35

    defaultConfig {
        minSdk = 26
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_11
        targetCompatibility = JavaVersion.VERSION_11
    }

    kotlinOptions {
        jvmTarget = "11"
    }

    sourceSets {
        getByName("main") {
            // Only libc++_shared.so lives here. The NobodyWho libraries come
            // from the shared UniFFI AAR dependency below.
            jniLibs.srcDirs(layout.buildDirectory.dir("jniLibs").get().asFile)
            if (extractLocalNativeAar != null) {
                jniLibs.srcDir(localNativeRoot.get().dir("jni").asFile)
            }
        }
    }
}

dependencies {
    // Exclude the JNA JAR from :common — Android needs the AAR variant instead
    api(project(":nobodywho-core")) {
        exclude(group = "net.java.dev.jna")
    }
    // JNA AAR for Android runtime
    implementation("net.java.dev.jna:jna:5.14.0@aar")
    // Android-specific coroutines dispatcher
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-android:1.8.1")

    if (localNativeAar == null) {
        implementation("ai.nobodywho:nobodywho-uniffi-android:$nobodywhoNativeVersion")
    }
}

if (extractLocalNativeAar != null) {
    tasks.named("preBuild") {
        dependsOn(extractLocalNativeAar)
    }
}

publishing {
    publications {
        register<MavenPublication>("release") {
            groupId = "ai.nobodywho"
            artifactId = "nobodywho-android"
            version = project.version.toString()

            afterEvaluate {
                from(components["release"])
            }
        }
    }
}
