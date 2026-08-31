pluginManagement {
    repositories {
        google()
        mavenCentral()
        gradlePluginPortal()
    }
    plugins {
        id("com.android.application") version "8.7.3"
        id("org.jetbrains.kotlin.android") version "2.0.21"
    }
}

dependencyResolutionManagement {
    repositories {
        providers.gradleProperty("nobodywhoRepository").orNull?.let { repositoryPath ->
            // Source device tests put release-shaped Maven artifacts here.
            // It is a runner-local directory, never a package registry.
            maven { url = uri(repositoryPath) }
        }
        google()
        mavenCentral()
    }
}

rootProject.name = "nobodywho-android-testapp"

// Two ways to build this app:
//
//  * default — consume the bindings in this repo via a composite build, so
//    changes on a branch are exercised without publishing anything. The
//    bindings' own settings/pluginManagement stay free of app concerns.
//
//  * -PnobodywhoVersion=<version> — skip the composite build and resolve a
//    Maven artifact. By default that is Maven Central; source device tests also
//    pass -PnobodywhoRepository=<local directory> for their staged candidate.
//
// dependencySubstitution below always overrides the coordinate, so the composite
// build has to be skipped entirely for the released artifact to be used.
if (providers.gradleProperty("nobodywhoVersion").orNull == null) {
    includeBuild("../../kotlin") {
        dependencySubstitution {
            substitute(module("ai.nobodywho:nobodywho-android")).using(project(":android"))
        }
    }
}
