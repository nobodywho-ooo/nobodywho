package ai.nobodywho

import com.sun.jna.Platform
import java.io.File
import java.net.JarURLConnection

/**
 * Stages the bundled native libraries so JNA can load the full dynamic-link graph.
 *
 * Under the dynamic-link build the binding lib (`nobodywho_uniffi`) is no longer
 * self-contained: it depends on co-located `libggml*`/`libllama*` siblings resolved
 * through its own `$ORIGIN`/`@loader_path` rpath. JNA's default resource loading
 * extracts only the *named* library to a private temp dir, leaving the siblings
 * undiscoverable — an `UnsatisfiedLinkError` at first use.
 *
 * [ensureLoaded] copies every lib packaged under the JNA resource prefix (binding +
 * siblings) into a single temp directory and prepends it to `jna.library.path`, so
 * JNA loads the binding from there and its rpath finds the siblings alongside it.
 *
 * It is a no-op when the libs are not packaged as JAR resources:
 *  - Android, where the OS loader resolves NEEDED libs from `jniLibs/<abi>`;
 *  - tests, which point `jna.library.path` at the co-located cargo build dir.
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
            // Exploded on disk (e.g. `gradle run` off a classes dir): copy the siblings
            // sitting next to the binding lib in the resource directory.
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
