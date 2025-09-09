package com.boundaryml.jetbrains_ext

import com.intellij.openapi.diagnostic.Logger
import com.intellij.openapi.project.Project
import com.redhat.devtools.lsp4ij.client.LanguageClientImpl
import org.eclipse.lsp4j.jsonrpc.services.JsonNotification
import org.eclipse.lsp4j.services.LanguageClient

data class PortParams(val port: Int)

class BamlLanguageClient(project: Project) :
    LanguageClientImpl(project) {

    private val log = Logger.getInstance(BamlLanguageClient::class.java)

    @JsonNotification("baml/port")
    fun onPort(params: PortParams) {
        Logger.getInstance(javaClass).warn("Port params: ${params.port}")

        println("Setting port to ${params.port}")
        project.getService(BamlGetPortService::class.java)
            .setPort(params.port)
        println("Set port to ${params.port}")
    }
}
