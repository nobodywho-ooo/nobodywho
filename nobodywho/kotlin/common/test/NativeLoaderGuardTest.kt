package ai.nobodywho

import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * Enforces the invariant that the generated UniFFI loader stages the bundled native libs
 * (see [NativeLoader]) *before* JNA registers the binding. The hook lives inside the
 * generated `nobodywho.kt`, at both `Native.register` sites, and is re-injected on every
 * regeneration by the inject-native-loader.py script in the justfile / regen_checks.yml.
 * This test fails loudly
 * if a regeneration ever drops it — which would silently reintroduce the packaged-JAR
 * UnsatisfiedLinkError this guards against. See kotlin/DEVELOPMENT.md.
 */
class NativeLoaderGuardTest {
    private val generated = File("generated/uniffi/nobodywho/nobodywho.kt")

    @Test
    fun `ensureLoaded runs before every Native_register`() {
        assertTrue(
            "generated bindings not found at ${generated.absolutePath} — run from the module dir",
            generated.isFile
        )
        val text = generated.readText()
        val registers = Regex("""Native\.register\(\w+::class\.java""").findAll(text).toList()
        assertTrue(
            "expected exactly 2 Native.register sites, found ${registers.size}",
            registers.size == 2
        )
        for (m in registers) {
            val register = m.range.first
            val initStart = text.lastIndexOf("init {", register)
            val hook = text.lastIndexOf("NativeLoader.ensureLoaded()", register)
            assertTrue(
                "NativeLoader.ensureLoaded() must precede `${m.value}` inside its init block; " +
                    "regeneration must re-inject it (see kotlin/DEVELOPMENT.md)",
                initStart >= 0 && hook > initStart && hook < register
            )
        }
    }
}
