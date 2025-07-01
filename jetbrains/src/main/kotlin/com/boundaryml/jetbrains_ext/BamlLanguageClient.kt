package com.boundaryml.jetbrains_ext

import BamlCustomServerAPI
import PortParams
import com.intellij.openapi.diagnostic.Logger
import com.intellij.openapi.project.Project
import com.redhat.devtools.lsp4ij.client.LanguageClientImpl

class BamlLanguageClient(project: Project) :
    LanguageClientImpl(project), BamlCustomServerAPI {

    private val log = Logger.getInstance(BamlLanguageClient::class.java)

    override fun onPort(params: PortParams) {
        Logger.getInstance(javaClass).warn("Port params: ${params.port}")
        project.getService(BamlGetPortService::class.java)
            .setPort(params.port)
    }
}
