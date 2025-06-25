package com.boundaryml.jetbrains_ext

import com.intellij.execution.configurations.GeneralCommandLine
import com.intellij.openapi.project.Project
import com.redhat.devtools.lsp4ij.server.OSProcessStreamConnectionProvider
import java.nio.file.Files
import java.nio.file.Path

class BamlLanguageServer(private val project: Project) : OSProcessStreamConnectionProvider() {

    init {
        // val commandLine = GeneralCommandLine(Path.of(System.getProperty("user.home"), ".baml/jetbrains", "baml-cli-0.89.0-aarch64-apple-darwin", "baml-cli").toString(), "lsp")
        // UNCOMMENT FOR DEBUGGING LOCALLY
//        val commandLine = GeneralCommandLine(
//            Path.of(System.getProperty("user.home"),
//                "/Documents/baml/engine/target/debug", "baml-cli").toString(), "lsp")
//        super.setCommandLine(commandLine)

        val cacheDir = Path.of(System.getProperty("user.home"), ".baml/jetbrains")
        val version  = Files.readString(cacheDir.resolve("baml-cli-installed.txt")).trim()

        val (arch, platform, _) = BamlLanguageServerInstaller.getPlatformTriple()
        val exe = if (platform == "pc-windows-msvc") "baml-cli.exe" else "baml-cli"
        val cli = cacheDir.resolve("baml-cli-$version-$arch-$platform").resolve(exe)

        super.setCommandLine(GeneralCommandLine(cli.toString(), "lsp"))

    }

}
