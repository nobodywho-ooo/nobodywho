// Root build file — shared configuration for all subprojects
allprojects {
    group = "ai.nobodywho"
    version = "0.1.0"
}

// Shared POM metadata and signing for publishable subprojects
subprojects {
    afterEvaluate {
        extensions.findByType<PublishingExtension>()?.apply {
            publications.withType<MavenPublication>().configureEach {
                pom {
                    name.set("NobodyWho")
                    description.set("Local LLM inference for Kotlin/Android — chat, tool calling, vision, and embeddings powered by llama.cpp")
                    url.set("https://github.com/nobodywho-ooo/nobodywho")
                    licenses {
                        license {
                            name.set("EUPL-1.2")
                            url.set("https://joinup.ec.europa.eu/collection/eupl/eupl-text-eupl-12")
                        }
                    }
                    developers {
                        developer {
                            id.set("nobodywho")
                            name.set("NobodyWho")
                            email.set("services@nobodywho.ooo")
                        }
                    }
                    scm {
                        connection.set("scm:git:git://github.com/nobodywho-ooo/nobodywho.git")
                        developerConnection.set("scm:git:ssh://github.com/nobodywho-ooo/nobodywho.git")
                        url.set("https://github.com/nobodywho-ooo/nobodywho")
                    }
                }
            }
        }

        // Sign all publications if a signing key is available
        plugins.withId("signing") {
            extensions.findByType<SigningExtension>()?.apply {
                val signingKey = providers.environmentVariable("SIGNING_KEY").orNull
                val signingPassword = providers.environmentVariable("SIGNING_PASSWORD").orNull
                if (signingKey != null) {
                    useInMemoryPgpKeys(signingKey, signingPassword ?: "")
                    sign(extensions.getByType<PublishingExtension>().publications)
                }
            }
        }
    }
}
