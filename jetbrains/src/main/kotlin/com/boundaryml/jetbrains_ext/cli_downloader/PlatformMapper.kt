package com.boundaryml.jetbrains_ext.cli_downloader

import mu.KotlinLogging
import java.nio.file.Files
import java.nio.file.Paths
import java.util.concurrent.TimeUnit

private val logger = KotlinLogging.logger {}

/**
 * Maps platform/architecture combinations to GitHub release naming conventions.
 */
object PlatformMapper {

    fun toGitHubReleaseArchitecture(arch: String): String = when (arch) {
        "x64" -> "x86_64"
        "arm64" -> "aarch64"
        else -> {
            logger.warn { "Unknown architecture for GitHub releases: $arch. Using as-is." }
            arch
        }
    }

    fun toGitHubReleasePlatform(platform: String): String = when (platform) {
        "win32" -> "pc-windows-msvc"
        "darwin" -> "apple-darwin"
        "linux" -> detectLinuxLibc()
        else -> {
            logger.warn { "Unknown platform for GitHub releases: $platform. Using as-is." }
            platform
        }
    }

    fun getExecutableName(platform: String = PlatformDetector.getCurrentPlatform()): String =
        if (platform == "win32") "baml-cli.exe" else "baml-cli"

    fun getArchiveExtension(platform: String = PlatformDetector.getCurrentPlatform()): String =
        when (toGitHubReleasePlatform(platform)) {
            "pc-windows-msvc" -> "zip"
            "apple-darwin", "unknown-linux-gnu", "unknown-linux-musl" -> "tar.gz"
            else -> {
                logger.warn { "Unknown archive extension for platform $platform, defaulting to zip" }
                "zip"
            }
        }

    private fun detectLinuxLibc(): String {
        return try {
            // Check for Alpine Linux (musl)
            if (Files.exists(Paths.get("/etc/alpine-release"))) {
                return "unknown-linux-musl"
            }

            // Try ldd --version to detect libc type
            val process = ProcessBuilder("ldd", "--version")
                .redirectErrorStream(true)
                .start()

            val output = process.inputStream.bufferedReader().use { it.readText() }
            process.waitFor(5, TimeUnit.SECONDS)

            when {
                output.lowercase().contains("musl") -> "unknown-linux-musl"
                else -> "unknown-linux-gnu"
            }
        } catch (e: Exception) {
            logger.warn(e) { "Failed to detect libc type, defaulting to gnu" }
            "unknown-linux-gnu"
        }
    }
}