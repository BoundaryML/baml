package com.boundaryml.jetbrains_ext

import com.intellij.openapi.project.Project
import com.redhat.devtools.lsp4ij.LanguageServerFactory
import com.redhat.devtools.lsp4ij.client.features.LSPClientFeatures
import com.redhat.devtools.lsp4ij.server.StreamConnectionProvider

class BamlLanguageServerFactory : LanguageServerFactory {

    override fun createConnectionProvider(project: Project): StreamConnectionProvider {
        return BamlLanguageServer(project)
    }

    override fun createClientFeatures(): LSPClientFeatures {
        val features = LSPClientFeatures()
        features.setServerInstaller(BamlLanguageServerInstaller()) // customize language server installer
        return features
    }

    override fun createLanguageClient(project: Project) =
        BamlLanguageClient(project)      // our custom client

    // If you need to expose a custom server API
//    override fun getServerInterface(): Class<out LanguageServer> {
//        return BamlCustomServerAPI.kt::class.java
//    }
}
