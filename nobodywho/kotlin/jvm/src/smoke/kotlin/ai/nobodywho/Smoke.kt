package ai.nobodywho

/**
 * Entry point for the JVM packaging smoke test (`jvmSmoke` task in build.gradle.kts).
 *
 * Exercises one pure-FFI call (no model, GPU, or network) off the published JAR with no
 * `jna.library.path` shim, so [NativeLoader] must stage the ggml/llama siblings itself —
 * a broken bundle fails CI here instead of at a downstream consumer. The co-located unit
 * tests miss this because they point `jna.library.path` at a dir holding all libs.
 */
fun main() {
    val sim = cosineSimilarity(listOf(1.0f, 0.0f, 0.0f), listOf(0.0f, 1.0f, 0.0f))
    check(sim > -1.01f && sim < 1.01f) { "unexpected cosineSimilarity=$sim" }
    println("nobodywho JVM smoke OK (cosineSimilarity=$sim)")
}
