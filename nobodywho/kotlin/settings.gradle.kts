pluginManagement {
    repositories {
        google()
        mavenCentral()
        gradlePluginPortal()
    }
    plugins {
        id("com.android.library") version "8.7.3"
        id("org.jetbrains.kotlin.android") version "2.0.21"
        id("org.jetbrains.kotlin.jvm") version "2.0.21"
    }
}

plugins {
    id("com.gradleup.nmcp.settings") version "1.4.4"
}

nmcpSettings {
    centralPortal {
        username = System.getenv("MAVEN_CENTRAL_USERNAME")
        password = System.getenv("MAVEN_CENTRAL_PASSWORD")
        publishingType = "AUTOMATIC"
    }
}

dependencyResolutionManagement {
    repositories {
        System.getenv("NOBODYWHO_CANDIDATE_MAVEN_REPO")?.let { repositoryPath ->
            // Source device tests resolve their runner-local native candidate
            // through the same Maven coordinate used by the release POM.
            maven { url = uri(repositoryPath) }
        }
        google()
        mavenCentral()
    }
}

rootProject.name = "nobodywho-kotlin"
include(":common", ":android", ":jvm")
project(":common").name = "nobodywho-core"
