package com.boundaryml.jetbrains_ext

import com.boundaryml.jetbrains_ext.cli_downloader.CliDownloader
import com.boundaryml.jetbrains_ext.cli_downloader.CliVersion
import com.intellij.openapi.components.service
import com.intellij.openapi.diagnostic.Logger
import com.intellij.openapi.progress.ProgressIndicator
import com.intellij.openapi.project.Project
import com.redhat.devtools.lsp4ij.LanguageServerFactory
import com.redhat.devtools.lsp4ij.client.features.LSPClientFeatures
import com.redhat.devtools.lsp4ij.installation.LanguageServerInstallerBase
import com.redhat.devtools.lsp4ij.server.StreamConnectionProvider
import kotlinx.coroutines.runBlocking

class BamlLanguageServerFactory : LanguageServerFactory {

    private val log = Logger.getInstance(javaClass)

    override fun createConnectionProvider(project: Project): StreamConnectionProvider {
        log.info("Creating connection provider")
        return BamlLanguageServer(project)
    }

    override fun createClientFeatures(): LSPClientFeatures {
        val features = object : LSPClientFeatures() {
            override fun initializeParams(params: org.eclipse.lsp4j.InitializeParams) {
                // Add initialization options for BAML settings
                params.initializationOptions = mapOf(
                    "settings" to mapOf(
                        "featureFlags" to listOf("beta"),
                        "generateCodeOnSave" to "always",
                        "lspMethodsToForwardToWebview" to listOf(
                            "runtime_updated",
                            "baml_settings_updated",
                            "workspace/executeCommand",
                        )
                    )
                )
            }
        }
        
        features.setServerInstaller(BamlLanguageServerInstaller()) // customize language server installer
        return features
    }

    override fun createLanguageClient(project: Project) =
        BamlLanguageClient(project)      // our custom client
}


class BamlLanguageServerInstaller : LanguageServerInstallerBase() {

    private val cliDownloader = CliDownloader()
    private val log = Logger.getInstance(javaClass)

    override fun checkServerInstalled(indicator: ProgressIndicator): Boolean {
        log.info("checkServerInstalled")
        super.progress("Checking if BAML CLI is installed...", indicator)
        val newCliVersion = service<BamlLanguageServerService>().getCurrentCliVersion()
        return cliDownloader.checkDownloadedCliExists(CliVersion.fromVersionString(newCliVersion))
    }

    override fun install(indicator: ProgressIndicator) {
        log.info("install")
        try {
            super.progress("Installing BAML CLI...", indicator)

            val newCliVersion = service<BamlLanguageServerService>().getCurrentCliVersion()
            val download = runBlocking { cliDownloader.resolveCliPath(newCliVersion) }

            super.progress("Installation complete!", 1.0, indicator)
        } finally {
            service<BamlLanguageServerService>().setRestartingFlag(false)
        }
    }
}