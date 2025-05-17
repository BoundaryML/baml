package com.boundaryml.jetbrains_ext

import com.intellij.openapi.application.PathManager
import com.intellij.openapi.application.PluginPathManager
import org.jetbrains.plugins.textmate.api.TextMateBundleProvider
import org.jetbrains.plugins.textmate.api.TextMateBundleProvider.PluginBundle
import java.io.IOException
import java.net.URL
import java.nio.file.Files
import java.nio.file.Path


class BamlTextMateBundleProvider : TextMateBundleProvider {
    private val files = listOf(
        "package.json",
        "language-configuration.json",
        "syntaxes/baml.tmLanguage.json",
        "syntaxes/jinja.tmLanguage.json"
    )

    // This strategy for loading the textmate bundle is taken from permify. Although it seems ridiculous to have to
    // unpack the resources one-by-one and then synthesize them into a directory, it makes sense - resources in Java are
    // bundled individually into the JAR, there's no magic dir that they're unzipped into, so we have to do this by hand.
    //
    // Original implementation:
    // https://github.com/mallowigi/permify-jetbrains/blob/master/src/main/kotlin/com/mallowigi/permify/PermifyTextMateBundleProvider.kt
    //
    // jetbrains-plugin-fss somehow has a magic dir approach:
    //
    //    final File directory = PluginPathManager.getPluginResource(this.getClass(), "fsh/fsh.tmbundle");
    //    if (directory == null) {
    //        LOG.warn("Could not find the FSH TextMate bundle");
    //        return List.of();
    //    }
    //    return List.of(new PluginBundle("FSH", directory.toPath()));
    //
    // but I couldn't figure out how to make that approach work. I suspect there's (1) magic in how tmbundle is loaded
    // and (2) build system shenanigans in jetbrains-plugin-fss for packaging the tmbundle.
    override fun getBundles(): List<PluginBundle> {
        try {
            val tmpDir: Path = Files.createTempDirectory(Path.of(PathManager.getTempPath()), "textmate-baml")

            files.forEach { fileToCopy ->
                val resource: URL? = javaClass.classLoader.getResource("textmate/$fileToCopy")

                resource?.openStream().use { resourceStream ->
                    if (resourceStream != null) {
                        val target: Path = tmpDir.resolve(fileToCopy)
                        Files.createDirectories(target.parent)
                        Files.copy(resourceStream, target)
                    }
                }
            }

            val bundle = PluginBundle("baml", tmpDir)
            return listOf(bundle)
        } catch (e: IOException) {
            throw RuntimeException(e)
        }
    }
}