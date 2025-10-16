package com.boundaryml.jetbrains_ext.cli_downloader

/**
 * Represents a specific version of the CLI with platform and architecture information.
 */
data class CliVersion(
    val version: String,
    val architecture: String,
    val platform: String
) {
    fun toArtifactName(): String = "baml-cli-$version-${architecture}-${platform}"

    companion object {

        fun fromVersionString(version: String): CliVersion {
            val currentPlatform = PlatformDetector.getCurrentPlatform()
            val currentArch = PlatformDetector.getCurrentArchitecture()

            return CliVersion(version, currentArch, currentPlatform)
        }
    }
}

/**
 * Tracks failure state for exponential backoff management.
 */
data class BackoffState(
    val failureCount: Int,
    val lastAttemptTimestamp: Long
)

/**
 * Result of a download operation.
 */
sealed class DownloadResult {
    data class Success(val filePath: String) : DownloadResult()
    data class Failure(val error: DownloadException) : DownloadResult()
}

/**
 * Hierarchy of download-related exceptions with specific error types.
 */
sealed class DownloadException(message: String, cause: Throwable? = null) : Exception(message, cause) {
    class NetworkError(message: String, cause: Throwable?) : DownloadException(message, cause)
    class ChecksumMismatch(expected: String, actual: String) :
        DownloadException("Checksum mismatch: expected $expected, got $actual")

    class ExtractionError(message: String, cause: Throwable?) : DownloadException(message, cause)
    class BackoffActive(waitTimeMs: Long) :
        DownloadException("Download blocked by backoff, wait ${waitTimeMs}ms")

    class FileNotFound(path: String) : DownloadException("File not found: $path")
}