plugins {
    id("com.android.application")
    id("kotlin-android")
    // The Flutter Gradle Plugin must be applied after the Android and Kotlin Gradle plugins.
    id("dev.flutter.flutter-gradle-plugin")
}

android {
    namespace = "ooo.nobodywho.nobodywho_testapp"
    compileSdk = flutter.compileSdkVersion
    ndkVersion = flutter.ndkVersion

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    kotlinOptions {
        jvmTarget = JavaVersion.VERSION_17.toString()
    }

    defaultConfig {
        applicationId = "ooo.nobodywho.nobodywho_testapp"
        minSdk = flutter.minSdkVersion
        targetSdk = flutter.targetSdkVersion
        versionCode = flutter.versionCode
        versionName = flutter.versionName
        // Required so `assembleAndroidTest` produces the instrumentation APK
        // that Firebase Test Lab runs the integration_test suite from.
        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
    }

    buildTypes {
        release {
            // Signing with the debug keys, so `flutter run --release` works.
            signingConfig = signingConfigs.getByName("debug")
        }
    }
}

// No androidTest dependencies are declared on purpose. The integration_test
// plugin exposes androidx.test runner/rules/espresso as `api` deps pinned to
// `1.2+`, which Gradle treats as a 1.2.x prefix match — declaring a newer
// version here fails to resolve against AGP's consistent-resolution constraint,
// and pinning an exact 1.2.x would break whenever that range resolves elsewhere.

flutter {
    source = "../.."
}
