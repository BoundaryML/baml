package com.boundaryml.jetbrains_ext

import com.intellij.openapi.diagnostic.Logger
import com.intellij.openapi.project.Project
import com.redhat.devtools.lsp4ij.client.LanguageClientImpl
import org.eclipse.lsp4j.jsonrpc.services.JsonNotification

// Existing data class (keep as-is)
data class PortParams(val port: Int)

// New data class for version switching
data class GeneratorVersionPayload(
    val version: String,
    val root_path: String
)

class BamlLanguageClient(project: Project) :
    LanguageClientImpl(project) {

    private val log = Logger.getInstance(javaClass)
    private val languageServerService = project.getService(BamlLanguageServerService::class.java)

    // Existing port notification (keep exactly as-is but use new service)
    @JsonNotification("baml/port")
    fun onPort(params: PortParams) {
        log.warn("Port params: ${params.port}")

        log.warn("Setting port to ${params.port}")
        languageServerService.setPort(params.port)
        log.warn("Set port to ${params.port}")
    }

    // New version switching notification - DETECTION ONLY for Phase 1
    @JsonNotification("baml_src_generator_version")
    fun generatorVersionNotification(payload: GeneratorVersionPayload) {
        log.warn("✅ DETECTED generator version notification: ${payload.version} for ${payload.root_path}")
        
        // Phase 1: Just log that we received it - no processing yet
        println("📋 Version notification received: ${payload.version} (processing not yet implemented)")
        
        // Validate we can access our service
        log.warn("Service state - current version: ${languageServerService.getCurrentCliVersion()}")
        log.warn("Service state - is restarting: ${languageServerService.isCurrentlyRestarting()}")
    }
}
