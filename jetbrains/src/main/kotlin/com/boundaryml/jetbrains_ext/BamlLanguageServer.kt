package com.boundaryml.jetbrains_ext

import com.boundaryml.jetbrains_ext.cli_downloader.CliDownloader
import com.intellij.execution.configurations.GeneralCommandLine
import com.intellij.openapi.components.service
import com.intellij.openapi.diagnostic.thisLogger
import com.intellij.openapi.project.Project
import com.redhat.devtools.lsp4ij.server.OSProcessStreamConnectionProvider
import kotlinx.coroutines.runBlocking
import java.nio.file.Files
import java.nio.file.Path

class BamlLanguageServer(private val project: Project) : OSProcessStreamConnectionProvider() {

    private val log = thisLogger();
    private val cliDownloader = CliDownloader()

    private fun findBamlWorkspaceRoot(startPath: Path): Path? {
        var current = startPath
        while (current.parent != null) {
            if (Files.exists(current.resolve("engine/Cargo.toml"))) {
                return current
            }
            current = current.parent
        }
        return null
    }

    init {
        if (BamlIdeConfig.shouldUseLocalLanguageServerBuild()) {
            // Kill any orphaned baml-cli processes before starting
            val pkillProcess = Runtime.getRuntime().exec("pkill -f target/debug/baml-cli")
            pkillProcess.waitFor()
            log.info("pkill'd the old baml-cli processes")

            // baml-hot-reload is implemented by recording and replaying stdin, but this may be buggy
            // if that happens, comment this out and just use `baml-cli` directly
            val hostIdeProjectDir =
                System.getenv("JETBRAINS_PROJECT_DIR") ?: throw RuntimeException("JETBRAINS_PROJECT_DIR was not set")
            val workspaceRoot = findBamlWorkspaceRoot(Path.of(hostIdeProjectDir))
                ?: throw RuntimeException("BAML workspace root not found")
            val hotReloadPath = workspaceRoot.resolve("engine/target/debug/language-server-hot-reload")
            val commandLine = GeneralCommandLine(hotReloadPath.toString(), "lsp")
                .withEnvironment("RUST_BACKTRACE", "full")
                .withEnvironment("BAML_INTERNAL_LOG", "debug")
                .withEnvironment("RUST_LOG", "debug")
                .withEnvironment("VSCODE_DEBUG_MODE", "true")
            super.setCommandLine(commandLine)
        } else {
            // Check for dynamic CLI path from version switching FIRST
            val languageServerService = service<BamlLanguageServerService>()
            val dynamicCliPath =
                runBlocking { cliDownloader.resolveCliPath(languageServerService.getCurrentCliVersion()) }

            log.info("creating baml language server at $dynamicCliPath")
            super.setCommandLine(GeneralCommandLine(dynamicCliPath, "lsp")
                .withEnvironment("BAML_INTERNAL_LOG", "debug")
                .withEnvironment("RUST_LOG", "debug")
            )
        }

    }
}
