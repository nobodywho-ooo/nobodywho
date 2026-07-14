package ai.nobodywho

import com.sun.jna.Platform
import java.io.File
import java.net.JarURLConnection

/**
 * Stages the bundled native libraries so JNA can load the full dynamic-link graph.
 *
 * Under dynamic-link the binding lib (`nobodywho_uniffi`) depends on co-located
 * `libggml*`/`libllama*` siblings via its own `$ORIGIN`/`@loader_path` rpath, but JNA
 * extracts only the *named* library from JAR resources to a private temp dir — leaving
 * the siblings undiscoverable (`UnsatisfiedLinkError` at first use). [ensureLoaded] copies
 * the whole resource dir (binding + siblings) into one temp dir and prepends it to
 * `jna.library.path`, so the rpath finds the siblings alongside the binding.
 *
 * No-op when the libs aren't JAR resources: Android (OS loader resolves from
 * `jniLibs/<abi>`) and tests (point `jna.library.path` at the cargo build dir).
 */
internal object NativeLoader {
    @Volatile private var done = false

    @Synchronized
    fun ensureLoaded() {
        if (done) return
        try {
            stageBundledLibs()
        } catch (e: Exception) {
            // Propagate the real cause rather than swallow: staging is what makes a
            // JAR-packaged dynamic-link binding loadable, so a failure here would only
            // resurface as an opaque UnsatisfiedLinkError at first FFI call.
            throw RuntimeException("nobodywho: failed to stage bundled native libraries", e)
        }
        done = true // only after success, so a transient failure can retry on the next call
    }

    private fun stageBundledLibs() {
        val prefix = Platform.RESOURCE_PREFIX
        val bindingResource = "/$prefix/" + System.mapLibraryName("nobodywho_uniffi")
        val url = NativeLoader::class.java.getResource(bindingResource) ?: return // not JAR-packaged

        val dir = File.createTempFile("nobodywho-natives", "").apply { delete(); mkdirs() }
        dir.deleteOnExit()

        val conn = url.openConnection()
        if (conn is JarURLConnection) {
            // Packaged in a JAR: copy every lib directly under <prefix>/ out to `dir`.
            // The JarFile is classloader-cached, so we read from it but must not close it.
            val jar = conn.jarFile
            val dirPrefix = "$prefix/"
            val entries = jar.entries()
            while (entries.hasMoreElements()) {
                val entry = entries.nextElement()
                val name = entry.name
                if (entry.isDirectory || !name.startsWith(dirPrefix)) continue
                if (name.indexOf('/', dirPrefix.length) >= 0) continue // direct children only
                val out = File(dir, name.substring(dirPrefix.length))
                jar.getInputStream(entry).use { input -> out.outputStream().use { output -> input.copyTo(output) } }
                out.deleteOnExit()
            }
        } else {
            // Exploded on disk (e.g. `gradle run`): copy the siblings next to the binding.
            val srcDir = File(url.toURI()).parentFile ?: return
            srcDir.listFiles()?.forEach { f ->
                if (!f.isFile) return@forEach
                val out = File(dir, f.name)
                f.copyTo(out, overwrite = true)
                out.deleteOnExit()
            }
        }

        val existing = System.getProperty("jna.library.path")
        System.setProperty(
            "jna.library.path",
            if (existing.isNullOrEmpty()) dir.absolutePath
            else dir.absolutePath + File.pathSeparator + existing
        )
    }
}
