package ai.nobodywho

fun main() {
    val sim = cosineSimilarity(listOf(1.0f, 0.0f, 0.0f), listOf(0.0f, 1.0f, 0.0f))
    check(sim > -1.01f && sim < 1.01f) { "unexpected cosineSimilarity=$sim" }
    println("nobodywho JVM smoke OK (cosineSimilarity=$sim)")
}
