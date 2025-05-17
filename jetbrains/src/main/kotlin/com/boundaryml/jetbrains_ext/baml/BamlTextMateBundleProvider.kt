package com.boundaryml.jetbrains_ext.baml

import com.intellij.openapi.application.PathManager
import org.jetbrains.plugins.textmate.api.TextMateBundleProvider
import org.jetbrains.plugins.textmate.api.TextMateBundleProvider.PluginBundle
import java.io.IOException
import java.net.URL
import java.nio.file.Files
import java.nio.file.Path


class BamlTextMateBundleProvider : TextMateBundleProvider {
//    override fun getBundles() =
////        PluginPathManager.getPluginResource(javaClass, "textmate")
//////            ?.let { listOf(TextMateBundleProvider.PluginBundle("baml", it.toPath())) }
////            ?: emptyList()
//            listOf(TextMateBundleProvider.PluginBundle("baml", Path("/Users/sam/baml3/jetbrains/src/main/resources/textmate")))

    private val files = listOf(
        "package.json",
        "language-configuration.json",
        "syntaxes/baml.tmLanguage.json",
        "syntaxes/jinja.tmLanguage.json"
    )

    override fun getBundles(): List<TextMateBundleProvider.PluginBundle> {
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