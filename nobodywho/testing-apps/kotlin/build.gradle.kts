plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
}

android {
    namespace = "ai.nobodywho.testapp"
    compileSdk = 35

    defaultConfig {
        applicationId = "ai.nobodywho.testapp"
        minSdk = 26
        targetSdk = 35
        versionCode = 1
        versionName = "0.1.0"
        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
        ndk {
            abiFilters += listOf("arm64-v8a", "x86_64")
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_11
        targetCompatibility = JavaVersion.VERSION_11
    }

    kotlinOptions {
        jvmTarget = "11"
    }
}

dependencies {
    // The default composite build substitutes the local `:android` project and
    // ignores this version; -PnobodywhoVersion=<v> resolves the released artifact
    // from Maven Central instead (see settings.gradle.kts). The sentinel is
    // unresolvable on purpose — if substitution ever breaks, this fails loudly
    // rather than silently testing a stale release.
    implementation(
        "ai.nobodywho:nobodywho-android:" +
            providers.gradleProperty("nobodywhoVersion").getOrElse("COMPOSITE-BUILD-ONLY")
    )

    // On-device instrumentation tests (run on real hardware via Firebase Test Lab)
    androidTestImplementation("androidx.test.ext:junit:1.2.1")
    androidTestImplementation("androidx.test:runner:1.6.2")
}
