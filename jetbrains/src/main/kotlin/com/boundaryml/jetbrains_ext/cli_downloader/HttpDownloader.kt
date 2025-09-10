package com.boundaryml.jetbrains_ext.cli_downloader

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import mu.KotlinLogging
import okhttp3.OkHttpClient
import okhttp3.Request
import java.io.IOException
import java.nio.file.Files
import java.nio.file.Path
import java.util.concurrent.TimeUnit

private val logger = KotlinLogging.logger {}

/**
 * HTTP client wrapper with timeout and error handling for downloading CLI binaries and checksums.
 */
class HttpDownloader(
    private val config: DownloadConfig,
    private val client: OkHttpClient = createDefaultClient(config)
) {
    companion object {
        private fun createDefaultClient(config: DownloadConfig): OkHttpClient =
            OkHttpClient.Builder()
                .connectTimeout(30, TimeUnit.SECONDS)
                .readTimeout(config.timeouts.binaryDownloadMs, TimeUnit.MILLISECONDS)
                .writeTimeout(config.timeouts.binaryDownloadMs, TimeUnit.MILLISECONDS)
                .build()
    }

    suspend fun downloadFile(url: String, targetPath: Path): Unit = withContext(Dispatchers.IO) {
        logger.info { "Downloading file from $url to $targetPath" }

        val request = Request.Builder()
            .url(url)
            .build()

        try {
            client.newCall(request).execute().use { response ->
                if (!response.isSuccessful) {
                    throw DownloadException.NetworkError(
                        "HTTP ${response.code}: ${response.message}",
                        null
                    )
                }

                response.body?.byteStream()?.use { inputStream ->
                    Files.newOutputStream(targetPath).use { outputStream ->
                        inputStream.copyTo(outputStream)
                    }
                } ?: throw DownloadException.NetworkError("Empty response body", null)
            }
            logger.info { "Successfully downloaded file to $targetPath" }
        } catch (e: Exception) {
            when (e) {
                is DownloadException -> throw e
                is IOException -> throw DownloadException.NetworkError("Network error: ${e.message}", e)
                else -> throw DownloadException.NetworkError("Unexpected error: ${e.message}", e)
            }
        }
    }

    suspend fun downloadText(url: String): String = withContext(Dispatchers.IO) {
        logger.debug { "Downloading text from $url" }

        val request = Request.Builder()
            .url(url)
            .build()

        val client = this@HttpDownloader.client.newBuilder()
            .readTimeout(config.timeouts.checksumDownloadMs, TimeUnit.MILLISECONDS)
            .build()

        try {
            client.newCall(request).execute().use { response ->
                if (!response.isSuccessful) {
                    throw DownloadException.NetworkError(
                        "HTTP ${response.code}: ${response.message}",
                        null
                    )
                }
                response.body?.string() ?: throw DownloadException.NetworkError("Empty response body", null)
            }
        } catch (e: Exception) {
            when (e) {
                is DownloadException -> throw e
                is IOException -> throw DownloadException.NetworkError("Network error: ${e.message}", e)
                else -> throw DownloadException.NetworkError("Unexpected error: ${e.message}", e)
            }
        }
    }
}