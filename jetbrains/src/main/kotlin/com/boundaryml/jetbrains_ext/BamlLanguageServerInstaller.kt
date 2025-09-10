package com.boundaryml.jetbrains_ext

import com.boundaryml.jetbrains_ext.cli_downloader.CliDownloader
import com.boundaryml.jetbrains_ext.cli_downloader.CliVersion
import com.intellij.openapi.components.service
import com.intellij.openapi.diagnostic.Logger
import com.intellij.openapi.progress.ProgressIndicator
import com.intellij.openapi.progress.ProgressManager
import com.intellij.util.io.HttpRequests
import com.redhat.devtools.lsp4ij.installation.LanguageServerInstallerBase
import kotlinx.coroutines.runBlocking
import kotlinx.serialization.SerialName
import org.apache.commons.compress.archivers.tar.TarArchiveEntry
import org.apache.commons.compress.archivers.tar.TarArchiveInputStream
import org.apache.commons.compress.compressors.gzip.GzipCompressorInputStream
import java.nio.file.*
import java.nio.file.attribute.PosixFilePermission
import java.security.MessageDigest
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json
import java.lang.RuntimeException
import java.util.zip.ZipInputStream

class BamlLanguageServerInstaller : LanguageServerInstallerBase() {

    private val cliDownloader = CliDownloader()

    companion object {
        /** arch, platform, extension (zip|tar.gz) */
        @JvmStatic
        fun getPlatformTriple(): Triple<String, String, String> {
            val os   = System.getProperty("os.name").lowercase()
            val arch = System.getProperty("os.arch").lowercase()

            val releaseArch = when {
                arch.contains("aarch64") || arch.contains("arm64") -> "aarch64"
                arch.contains("x86_64") || arch.contains("amd64")  -> "x86_64"
                else -> error("Unsupported arch: $arch")
            }
            val releasePlatform = when {
                os.contains("mac")   -> "apple-darwin"
                os.contains("win")   -> "pc-windows-msvc"
                os.contains("linux") -> "unknown-linux-gnu"
                else -> error("Unsupported OS: $os")
            }
            val ext = if (releasePlatform == "pc-windows-msvc") "zip" else "tar.gz"
            return Triple(releaseArch, releasePlatform, ext)
        }
    }
    private val log = Logger.getInstance(javaClass)

    override fun checkServerInstalled(indicator: ProgressIndicator): Boolean {
        log.info("checkServerInstalled")
        super.progress("Checking if BAML CLI is installed...", indicator)
        val newCliVersion = service<BamlLanguageServerService>().getCurrentCliVersion()
        return cliDownloader.checkDownloadedCliExists(CliVersion.fromVersionString(newCliVersion))
    }

    override fun install(indicator: ProgressIndicator) {
        log.info("install")
        super.progress("Installing BAML CLI...", indicator)

        val newCliVersion = service<BamlLanguageServerService>().getCurrentCliVersion()
        val download = runBlocking { cliDownloader.resolveCliPath(newCliVersion) }
        
        super.progress("Installation complete!", 1.0, indicator)
    }
}
