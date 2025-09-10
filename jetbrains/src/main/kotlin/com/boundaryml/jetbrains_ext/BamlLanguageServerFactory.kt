package com.boundaryml.jetbrains_ext

import com.intellij.openapi.diagnostic.Logger
import com.intellij.openapi.project.Project
import com.redhat.devtools.lsp4ij.LanguageServerFactory
import com.redhat.devtools.lsp4ij.client.features.LSPClientFeatures
import com.redhat.devtools.lsp4ij.server.StreamConnectionProvider

class BamlLanguageServerFactory : LanguageServerFactory {

    private val log = Logger.getInstance(javaClass)

    override fun createConnectionProvider(project: Project): StreamConnectionProvider {
        log.info("Creating connection provider")
        return BamlLanguageServer(project)
    }

    override fun createClientFeatures(): LSPClientFeatures {
        val features = LSPClientFeatures()
        features.setServerInstaller(BamlLanguageServerInstaller()) // customize language server installer
        return features
    }

    override fun createLanguageClient(project: Project) =
        BamlLanguageClient(project)      // our custom client
}
