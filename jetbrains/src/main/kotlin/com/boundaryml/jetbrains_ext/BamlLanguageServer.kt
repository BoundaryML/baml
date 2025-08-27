package com.boundaryml.jetbrains_ext

import com.intellij.execution.configurations.GeneralCommandLine
import com.intellij.openapi.project.Project
import com.redhat.devtools.lsp4ij.server.OSProcessStreamConnectionProvider
import java.nio.file.Files
import java.nio.file.Path

class BamlLanguageServer(private val project: Project) : OSProcessStreamConnectionProvider() {

    init {
        val commandLine = if (BamlIdeConfig.isDebugMode) {
            // Kill any orphaned baml-cli processes before starting
            val pkillProcess = Runtime.getRuntime().exec("pkill -f target/debug/baml-cli")
            pkillProcess.waitFor()
            println("pkill'd the old baml-cli processes")

            // baml-hot-reload is implemented by recording and replaying stdin, but this may be buggy
            // if that happens, comment this out and just use `baml-cli` directly
            GeneralCommandLine("/Users/sam/baml4/engine/target/debug/baml-hot-reload", "lsp")
                .withEnvironment("RUST_BACKTRACE", "full")
                .withEnvironment("VSCODE_DEBUG_MODE", "true")
            // Commented debug option:
            // GeneralCommandLine("/Users/sam/baml4/engine/target/debug/baml-cli", "lsp")
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
