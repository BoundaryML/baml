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
        // Check for dynamic CLI path from version switching FIRST
        val languageServerService = service<BamlLanguageServerService>()
        val dynamicCliPath = runBlocking {  cliDownloader.resolveCliPath(languageServerService.getCurrentCliVersion()) }

        log.info("creating baml language server at $dynamicCliPath")
        super.setCommandLine(GeneralCommandLine(dynamicCliPath, "lsp"))
        
//        val commandLine = if (dynamicCliPath != null) {
//            // Use dynamic CLI path from version switching
//            log.info("Using dynamic CLI path from version switch: $dynamicCliPath")
//            GeneralCommandLine(dynamicCliPath, "lsp")
//        } else if (BamlIdeConfig.isDebugMode) {
//            // PRESERVE EXISTING DEBUG MODE LOGIC EXACTLY AS-IS
//            // Kill any orphaned baml-cli processes before starting
//            val pkillProcess = Runtime.getRuntime().exec("pkill -f target/debug/baml-cli")
//            pkillProcess.waitFor()
//            log.info("pkill'd the old baml-cli processes")
//
//            // baml-hot-reload is implemented by recording and replaying stdin, but this may be buggy
//            // if that happens, comment this out and just use `baml-cli` directly
//            val hostIdeProjectDir = System.getenv("JETBRAINS_PROJECT_DIR") ?: throw RuntimeException("JETBRAINS_PROJECT_DIR was not set")
//            val workspaceRoot = findBamlWorkspaceRoot(Path.of(hostIdeProjectDir)) ?: throw RuntimeException("BAML workspace root not found")
//            val hotReloadPath = workspaceRoot.resolve("engine/target/debug/language-server-hot-reload")
//            GeneralCommandLine(hotReloadPath.toString(), "lsp")
//                .withEnvironment("RUST_BACKTRACE", "full")
//                .withEnvironment("BAML_INTERNAL_LOG", "debug")
//                .withEnvironment("RUST_LOG", "debug")
//                .withEnvironment("VSCODE_DEBUG_MODE", "true")
//        } else {
//            // REPLACE TODO: Use dynamic path resolution instead of existing production logic
//            // Get the extension's bundled version and resolve CLI path dynamically
//            val extensionVersion = getExtensionVersion() // TODO: implement this to read from plugin metadata
//            val resolvedCliPath = runBlocking {
//                BamlCliPathResolver.resolveCliPath(project, extensionVersion)
//            }
//
//            if (resolvedCliPath != null) {
//                log.info("Using resolved CLI path: $resolvedCliPath")
//                GeneralCommandLine(resolvedCliPath, "lsp")
//            } else {
//                // Fallback to hardcoded debug CLI if resolution fails
//                val fallbackPath = "/Users/sam/baml/engine/target/debug/baml-cli"
//                log.info("CLI resolution failed, using fallback: $fallbackPath")
//                GeneralCommandLine(fallbackPath, "lsp")
//            }
//        }
//        super.setCommandLine(commandLine)
    }
    
    private fun getExtensionVersion(): String {
        // TODO: Get actual extension version from plugin.xml or build.gradle.kts
        // For now, return a placeholder version that matches an available CLI
        return "0.206.1" // Hardcoded for initial implementation
    }

}
