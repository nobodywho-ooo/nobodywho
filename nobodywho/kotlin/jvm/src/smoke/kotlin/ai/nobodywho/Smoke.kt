package ai.nobodywho

/**
 * Entry point for the JVM packaging smoke test (see the `jvmSmoke` task in build.gradle.kts).
 *
 * Not part of the public API. It exercises a single pure-FFI call — one that needs no
 * model file, GPU, or network — so that a broken native-lib bundling in the published JAR
 * (e.g. the binding lib loading but its ggml/llama siblings not resolving) fails CI here
 * instead of at a downstream consumer. The co-located unit tests can't catch that because
 * they point `jna.library.path` at a directory holding all libs; this runs off the JAR
 * with no such shim, so [NativeLoader] must stage the siblings itself.
 */
fun main() {
    val sim = cosineSimilarity(listOf(1.0f, 0.0f, 0.0f), listOf(0.0f, 1.0f, 0.0f))
    check(sim > -1.01f && sim < 1.01f) { "unexpected cosineSimilarity=$sim" }
    println("nobodywho JVM smoke OK (cosineSimilarity=$sim)")
}
