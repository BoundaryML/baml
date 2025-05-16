package org.jetbrains.plugins.template.baml

import com.intellij.openapi.application.PluginPathManager
import org.jetbrains.plugins.textmate.api.TextMateBundleProvider

class BamlTextMateBundleProvider : TextMateBundleProvider {
    override fun getBundles() =
        PluginPathManager.getPluginResource(javaClass, "baml.tmbundle")
            ?.let { listOf(TextMateBundleProvider.PluginBundle("baml", it.toPath())) }
            ?: emptyList()
}