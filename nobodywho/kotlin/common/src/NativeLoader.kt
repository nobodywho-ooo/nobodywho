package uniffi.nobodywho

import com.sun.jna.Platform
import java.io.File
import java.net.JarURLConnection
import java.nio.file.Files

internal object NativeLoader {
    private val bundledPath by lazy { findBundledLibrary() }

    fun findLibraryName(componentName: String): String =
        System.getProperty("uniffi.component.$componentName.libraryOverride")
            ?: bundledPath
            ?: "${componentName}_uniffi"

    private fun findBundledLibrary(): String? {
        val prefix = Platform.RESOURCE_PREFIX
        val libraryName = System.mapLibraryName("nobodywho_uniffi")
        val url = NativeLoader::class.java.getResource("/$prefix/$libraryName") ?: return null
        if (url.protocol == "file") return File(url.toURI()).absolutePath

        val connection = url.openConnection() as? JarURLConnection
            ?: error("unsupported native library URL: $url")
        val directory = Files.createTempDirectory("nobodywho-natives").toFile()
        directory.deleteOnExit()
        val entryPrefix = "$prefix/"
        val entries = connection.jarFile.entries()
        while (entries.hasMoreElements()) {
            val entry = entries.nextElement()
            if (entry.isDirectory || !entry.name.startsWith(entryPrefix)) continue
            val name = entry.name.removePrefix(entryPrefix)
            if ('/' in name) continue
            val output = File(directory, name)
            connection.jarFile.getInputStream(entry).use { input ->
                output.outputStream().use(input::copyTo)
            }
            output.deleteOnExit()
        }

        return File(directory, libraryName).also {
            require(it.isFile) { "$libraryName is missing from bundled native libraries" }
        }.absolutePath
    }
}
