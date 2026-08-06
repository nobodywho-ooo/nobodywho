pluginManagement {
    repositories {
        google()
        mavenCentral()
        gradlePluginPortal()
    }
    plugins {
        id("com.android.application") version "8.7.3"
        id("org.jetbrains.kotlin.android") version "2.0.21"
        id("org.jetbrains.kotlin.plugin.compose") version "2.0.21"
    }
}

dependencyResolutionManagement {
    repositories {
        google()
        mavenCentral()
    }
}

rootProject.name = "nobodywho-android-testapp"

// Consume the Kotlin bindings from the sibling build without merging into it —
// the bindings' own settings/pluginManagement stay free of app + compose
// concerns. `:android` (which carries the arm64 .so) is substituted in for the
// `ai.nobodywho:nobodywho-android` coordinate the app depends on.
includeBuild("../../kotlin") {
    dependencySubstitution {
        substitute(module("ai.nobodywho:nobodywho-android")).using(project(":android"))
    }
}
