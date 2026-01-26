package com.boundaryml.jetbrains_ext.cli_downloader

import mu.KotlinLogging

private val logger = KotlinLogging.logger {}

/**
 * Detects current platform and architecture using JVM system properties.
 */
object PlatformDetector {

    fun getCurrentPlatform(): String = System.getProperty("os.name").lowercase().let { osName ->
        when {
            osName.contains("windows") -> "win32"
            osName.contains("mac") || osName.contains("darwin") -> "darwin"
            osName.contains("linux") -> "linux"
            else -> {
                logger.warn { "Unknown platform: $osName. Using as-is." }
                osName
            }
        }
    }

    fun getCurrentArchitecture(): String = System.getProperty("os.arch").lowercase().let { arch ->
        when (arch) {
            "amd64", "x86_64" -> "x64"
            "aarch64", "arm64" -> "arm64"
            else -> {
                logger.warn { "Unknown architecture: $arch. Using as-is." }
                arch
            }
        }
    }

    fun getCurrentCliVersion(): CliVersion = CliVersion(
        version = "latest", // Will be overridden by caller
        architecture = getCurrentArchitecture(),
        platform = getCurrentPlatform()
    )
}