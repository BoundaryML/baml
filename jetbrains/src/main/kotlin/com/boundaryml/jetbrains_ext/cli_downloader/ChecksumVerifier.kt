package com.boundaryml.jetbrains_ext.cli_downloader

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import mu.KotlinLogging
import java.nio.file.Files
import java.nio.file.Path
import java.security.MessageDigest

private val logger = KotlinLogging.logger {}

/**
 * SHA256 checksum calculation and verification for downloaded files.
 */
class ChecksumVerifier {

    suspend fun calculateSha256(filePath: Path): String = withContext(Dispatchers.IO) {
        logger.debug { "Calculating SHA256 for $filePath" }

        val digest = MessageDigest.getInstance("SHA-256")

        Files.newInputStream(filePath).use { inputStream ->
            val buffer = ByteArray(8192)
            var bytesRead: Int

            while (inputStream.read(buffer).also { bytesRead = it } != -1) {
                digest.update(buffer, 0, bytesRead)
            }
        }

        val hash = digest.digest().joinToString("") { "%02x".format(it) }
        logger.debug { "Calculated SHA256: $hash" }
        hash
    }

    suspend fun verifyChecksum(filePath: Path, expectedChecksum: String): Boolean {
        val actualChecksum = calculateSha256(filePath)
        val isValid = actualChecksum.equals(expectedChecksum, ignoreCase = true)

        if (!isValid) {
            logger.error { "Checksum mismatch for $filePath: expected $expectedChecksum, got $actualChecksum" }
            throw DownloadException.ChecksumMismatch(expectedChecksum, actualChecksum)
        }

        logger.info { "Checksum verification successful for $filePath" }
        return true
    }

    suspend fun downloadAndVerifyChecksum(url: String, httpDownloader: HttpDownloader): String {
        logger.info { "Downloading checksum from $url" }

        val checksumContent = try {
            httpDownloader.downloadText(url)
        } catch (e: DownloadException.NetworkError) {
            if (e.message?.contains("404") == true) {
                throw DownloadException.FileNotFound("Checksum file not found at $url")
            }
            throw e
        }

        val expectedChecksum = checksumContent.trim().split(Regex("\\s+")).first()

        if (!isValidSha256(expectedChecksum)) {
            throw DownloadException.NetworkError("Invalid checksum format: $expectedChecksum", null)
        }

        logger.debug { "Downloaded checksum: $expectedChecksum" }
        return expectedChecksum
    }

    private fun isValidSha256(checksum: String): Boolean =
        checksum.matches(Regex("[a-fA-F0-9]{64}"))
}