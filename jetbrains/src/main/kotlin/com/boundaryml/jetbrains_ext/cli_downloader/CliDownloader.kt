package com.boundaryml.jetbrains_ext.cli_downloader

import com.intellij.openapi.diagnostic.thisLogger
import kotlinx.coroutines.runBlocking
import mu.KotlinLogging
import java.nio.file.Files

private val logger = KotlinLogging.logger {}

/**
 * Main CLI downloader orchestrator that integrates all components.
 */
class CliDownloader(
    private val config: DownloadConfig = DownloadConfig.default(),
    private val pathManager: PathManager = PathManager(config),
    private val httpDownloader: HttpDownloader = HttpDownloader(config),
    private val checksumVerifier: ChecksumVerifier = ChecksumVerifier(),
    private val backoffManager: BackoffManager = BackoffManager(config.backoff)
) {

    suspend fun resolveCliPath(requestedVersion: String): String? {

        logger.info { "Resolving CLI path for version: $requestedVersion" }

        val currentPlatform = PlatformDetector.getCurrentPlatform()
        val currentArch = PlatformDetector.getCurrentArchitecture()
        val cliVersion = CliVersion(requestedVersion, currentArch, currentPlatform)

        // Check if already downloaded
        val expectedDownloadedPath = pathManager.getDownloadedCliPath(cliVersion)
        if (pathManager.checkDownloadedCliExists(cliVersion)) {
            logger.info { "Found existing CLI for version $requestedVersion: $expectedDownloadedPath" }
            return expectedDownloadedPath.toString()
        }

        // Check backoff state
        when (val backoffResult = backoffManager.shouldAttemptDownload(requestedVersion)) {
            is BackoffResult.ShouldWait -> {
                val waitMinutes = backoffResult.waitTimeMs / 1000 / 60
                logger.warn { "Download blocked by backoff for $requestedVersion. Wait $waitMinutes minutes." }
                return null
            }

            is BackoffResult.AlreadyInProgress -> {
                logger.warn { "Download already in progress for $requestedVersion" }
                return null
            }

            BackoffResult.ShouldAttempt -> {
                // Proceed with download
            }
        }

        // Attempt download
        return try {
            backoffManager.markDownloadStarted(requestedVersion)
            downloadCli(cliVersion)
        } catch (e: Exception) {
            logger.error(e) { "Download failed for version $requestedVersion" }
            backoffManager.recordFailure(requestedVersion)
            null
        } finally {
            backoffManager.markDownloadCompleted(requestedVersion)
        }
    }

    suspend fun downloadCli(cliVersion: CliVersion): String? {
        logger.info { "Starting download for BAML CLI v${cliVersion.version}" }

        val installPath = pathManager.ensureInstallPathExists()
        val artifactName = pathManager.getBinaryArtifactName(cliVersion)
        val extension = PlatformMapper.getArchiveExtension(cliVersion.platform)
        val compressedFileName = "$artifactName.$extension"

        val tempFilePath = installPath.resolve("$compressedFileName.tmp")
        val targetFilePath = pathManager.getDownloadedCliPath(cliVersion)

        val binaryUrl = "${config.baseUrl}/${cliVersion.version}/$compressedFileName"
        val checksumUrl = "${config.baseUrl}/${cliVersion.version}/$compressedFileName.sha256"

        logger.info { "Download URLs - Binary: $binaryUrl, Checksum: $checksumUrl" }

        var downloadSucceeded = false

        try {
            // Download binary file
            httpDownloader.downloadFile(binaryUrl, tempFilePath)
            downloadSucceeded = true

            // Download and verify checksum
            val expectedChecksum = checksumVerifier.downloadAndVerifyChecksum(checksumUrl, httpDownloader)
            checksumVerifier.verifyChecksum(tempFilePath, expectedChecksum)

            // Extract archive
            val extractor = ArchiveExtractorFactory.getExtractor(extension)
            extractor.extract(tempFilePath, targetFilePath.fileName.toString(), installPath)

            // Set executable permissions
            if (!pathManager.ensureExecutablePermissions(targetFilePath)) {
                throw DownloadException.ExtractionError("Failed to set executable permissions", null)
            }

            // Success - clear any backoff state
            backoffManager.clearBackoff(cliVersion.version)
            logger.info { "Successfully downloaded BAML CLI v${cliVersion.version} to $targetFilePath" }

            return targetFilePath.toString()

        } finally {
            // Clean up temporary file
            if (downloadSucceeded) {
                try {
                    Files.deleteIfExists(tempFilePath)
                    logger.debug { "Cleaned up temporary file: $tempFilePath" }
                } catch (e: Exception) {
                    logger.warn(e) { "Failed to delete temporary file: $tempFilePath" }
                }
            }
        }
    }

    fun checkDownloadedCliExists(cliVersion: CliVersion): Boolean =
        runBlocking { pathManager.checkDownloadedCliExists(cliVersion) }
}

/**
 * Factory object providing convenience methods for creating CLI downloaders.
 */
object CliDownloaderFactory {

    fun create(config: DownloadConfig = DownloadConfig.default()): CliDownloader =
        CliDownloader(config)

    // Mirror TypeScript exports
    suspend fun downloadCli(version: String, config: DownloadConfig = DownloadConfig.default()): String? {
        val downloader = create(config)
        val currentPlatform = PlatformDetector.getCurrentPlatform()
        val currentArch = PlatformDetector.getCurrentArchitecture()
        val cliVersion = CliVersion(version, currentArch, currentPlatform)
        return downloader.downloadCli(cliVersion)
    }

    suspend fun resolveCliPath(version: String, config: DownloadConfig = DownloadConfig.default()): String? {
        return create(config).resolveCliPath(version)
    }

    fun checkIfDownloadedCliExists(version: String, config: DownloadConfig = DownloadConfig.default()): Boolean {
        val downloader = create(config)
        val currentPlatform = PlatformDetector.getCurrentPlatform()
        val currentArch = PlatformDetector.getCurrentArchitecture()
        val cliVersion = CliVersion(version, currentArch, currentPlatform)
        return downloader.checkDownloadedCliExists(cliVersion)
    }

    fun getReleaseArchitecture(nodeArch: String = PlatformDetector.getCurrentArchitecture()): String =
        PlatformMapper.toGitHubReleaseArchitecture(nodeArch)

    fun getReleasePlatform(platform: String = PlatformDetector.getCurrentPlatform()): String =
        PlatformMapper.toGitHubReleasePlatform(platform)
}