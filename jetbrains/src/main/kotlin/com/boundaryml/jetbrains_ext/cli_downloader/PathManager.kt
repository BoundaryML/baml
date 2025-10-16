package com.boundaryml.jetbrains_ext.cli_downloader

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import mu.KotlinLogging
import java.nio.file.Files
import java.nio.file.Path
import java.nio.file.Paths
import java.nio.file.attribute.PosixFilePermissions

private val logger = KotlinLogging.logger {}

/**
 * Manages installation paths and file operations for CLI binaries.
 */
class PathManager(private val config: DownloadConfig) {

    fun getInstallPath(): Path = Paths.get(config.installPath)

    fun getDownloadedCliPath(cliVersion: CliVersion): Path {
        val releaseArch = PlatformMapper.toGitHubReleaseArchitecture(cliVersion.architecture)
        val releasePlatform = PlatformMapper.toGitHubReleasePlatform(cliVersion.platform)
        val executableName = PlatformMapper.getExecutableName(cliVersion.platform)

        val uniqueFileName = "baml-cli-${cliVersion.version}-${releaseArch}-${releasePlatform}-${executableName}"
        return getInstallPath().resolve(uniqueFileName)
    }

    fun getBinaryArtifactName(cliVersion: CliVersion): String {
        val releaseArch = PlatformMapper.toGitHubReleaseArchitecture(cliVersion.architecture)
        val releasePlatform = PlatformMapper.toGitHubReleasePlatform(cliVersion.platform)
        return "baml-cli-${cliVersion.version}-${releaseArch}-${releasePlatform}"
    }

    suspend fun ensureInstallPathExists(): Path {
        val installPath = getInstallPath()
        withContext(Dispatchers.IO) {
            if (!Files.exists(installPath)) {
                logger.info { "Creating CLI install directory: $installPath" }
                Files.createDirectories(installPath)
            }
        }
        return installPath
    }

    suspend fun checkDownloadedCliExists(cliVersion: CliVersion): Boolean {
        val expectedPath = getDownloadedCliPath(cliVersion)
        return withContext(Dispatchers.IO) {
            Files.exists(expectedPath) && Files.isExecutable(expectedPath)
        }
    }

    suspend fun ensureExecutablePermissions(filePath: Path): Boolean = withContext(Dispatchers.IO) {
        try {
            if (PlatformDetector.getCurrentPlatform() != "win32") {
                val permissions = PosixFilePermissions.fromString("rwxr-xr-x")
                Files.setPosixFilePermissions(filePath, permissions)
                logger.debug { "Set executable permissions for: $filePath" }
            }
            true
        } catch (e: Exception) {
            logger.error(e) { "Failed to set executable permissions for $filePath" }
            false
        }
    }
}