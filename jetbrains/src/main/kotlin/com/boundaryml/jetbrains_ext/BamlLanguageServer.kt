package com.boundaryml.jetbrains_ext

import com.intellij.execution.configurations.GeneralCommandLine
import com.intellij.openapi.project.Project
import com.redhat.devtools.lsp4ij.server.OSProcessStreamConnectionProvider
import java.nio.file.Files
import java.nio.file.Path

class BamlLanguageServer(private val project: Project) : OSProcessStreamConnectionProvider() {

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
        val commandLine = if (BamlIdeConfig.isDebugMode) {
            // Kill any orphaned baml-cli processes before starting
            val pkillProcess = Runtime.getRuntime().exec("pkill -f target/debug/baml-cli")
            pkillProcess.waitFor()
            println("pkill'd the old baml-cli processes")

            // baml-hot-reload is implemented by recording and replaying stdin, but this may be buggy
            // if that happens, comment this out and just use `baml-cli` directly
            val hostIdeProjectDir = System.getenv("JETBRAINS_PROJECT_DIR") ?: throw RuntimeException("JETBRAINS_PROJECT_DIR was not set")
            val workspaceRoot = findBamlWorkspaceRoot(Path.of(hostIdeProjectDir)) ?: throw RuntimeException("BAML workspace root not found")
            val hotReloadPath = workspaceRoot.resolve("engine/target/debug/language-server-hot-reload")
            GeneralCommandLine(hotReloadPath.toString(), "lsp")
                .withEnvironment("RUST_BACKTRACE", "full")
                .withEnvironment("BAML_INTERNAL_LOG", "debug")
                .withEnvironment("RUST_LOG", "debug")
                .withEnvironment("VSCODE_DEBUG_MODE", "true")
        } else {
            // Production mode - use installed CLI from cache
            val cacheDir = Path.of(System.getProperty("user.home"), ".baml/jetbrains")
            val version = Files.readString(cacheDir.resolve("baml-cli-installed.txt")).trim()
            val (arch, platform, _) = BamlLanguageServerInstaller.getPlatformTriple()
            val exe = if (platform == "pc-windows-msvc") "baml-cli.exe" else "baml-cli"
            val cli = cacheDir.resolve("baml-cli-$version-$arch-$platform").resolve(exe)
            GeneralCommandLine(cli.toString(), "lsp")
        }
        super.setCommandLine(commandLine)
    }

}
