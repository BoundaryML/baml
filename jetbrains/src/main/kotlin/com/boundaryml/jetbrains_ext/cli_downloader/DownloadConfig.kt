package com.boundaryml.jetbrains_ext.cli_downloader

/**
 * Main configuration for the CLI downloader with sensible defaults.
 */
data class DownloadConfig(
    val baseUrl: String = "https://github.com/BoundaryML/baml/releases/download",
    val installPath: String = "${System.getProperty("user.home")}/.baml",
    val timeouts: TimeoutConfig = TimeoutConfig.default(),
    val backoff: BackoffConfig = BackoffConfig.default()
) {
    companion object {
        fun default() = DownloadConfig()
    }
}

/**
 * Timeout configuration for various download operations.
 */
data class TimeoutConfig(
    val binaryDownloadMs: Long = 60_000,
    val checksumDownloadMs: Long = 10_000,
    val extractionTimeoutMs: Long = 30_000
) {
    companion object {
        fun default() = TimeoutConfig()
    }
}

/**
 * Exponential backoff configuration to prevent excessive retry attempts.
 */
data class BackoffConfig(
    val initialDelayMs: Long = 10 * 60 * 1000, // 10 minutes
    val maxDelayMs: Long = 60 * 60 * 1000,     // 1 hour
    val maxFailureCountBeforeReset: Int = 5
) {
    companion object {
        fun default() = BackoffConfig()
    }
}