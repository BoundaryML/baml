package com.boundaryml.jetbrains_ext.cli_downloader

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import mu.KotlinLogging
import org.apache.commons.compress.archivers.tar.TarArchiveEntry
import org.apache.commons.compress.archivers.tar.TarArchiveInputStream
import org.apache.commons.compress.archivers.zip.ZipArchiveEntry
import org.apache.commons.compress.archivers.zip.ZipArchiveInputStream
import org.apache.commons.compress.compressors.gzip.GzipCompressorInputStream
import java.nio.file.Files
import java.nio.file.Path

private val logger = KotlinLogging.logger {}

/**
 * Interface for extracting archives (ZIP and TAR.GZ).
 */
interface ArchiveExtractor {
    suspend fun extract(archivePath: Path, targetFileName: String, installDir: Path)
}

object ArchiveExtractorFactory {
    fun getExtractor(extension: String): ArchiveExtractor = when (extension.lowercase()) {
        "zip" -> ZipExtractor()
        "tar.gz" -> TarGzExtractor()
        else -> throw IllegalArgumentException("Unsupported archive format: $extension")
    }
}

/**
 * ZIP archive extractor with security validation.
 */
class ZipExtractor : ArchiveExtractor {

    override suspend fun extract(archivePath: Path, targetFileName: String, installDir: Path) =
        withContext(Dispatchers.IO) {
            logger.info { "Extracting ZIP archive $archivePath to $installDir/$targetFileName" }

            Files.newInputStream(archivePath).use { inputStream ->
                ZipArchiveInputStream(inputStream).use { zipStream ->
                    var entry: ZipArchiveEntry?
                    val executableName = PlatformMapper.getExecutableName()

                    while (zipStream.nextZipEntry.also { entry = it } != null) {
                        val currentEntry = entry ?: continue

                        if (currentEntry.name == executableName) {
                            val targetPath = installDir.resolve(targetFileName)

                            // Validate path for security (ZIP slip protection)
                            validateExtractPath(targetPath, installDir)

                            Files.newOutputStream(targetPath).use { outputStream ->
                                zipStream.copyTo(outputStream)
                            }

                            logger.info { "Extracted $executableName to $targetPath" }
                            return@withContext
                        }
                    }

                    throw DownloadException.ExtractionError(
                        "Required entry '$executableName' not found in ZIP archive",
                        null
                    )
                }
            }
        }

    private fun validateExtractPath(targetPath: Path, baseDir: Path) {
        val normalizedTarget = targetPath.normalize()
        val normalizedBase = baseDir.normalize()

        if (!normalizedTarget.startsWith(normalizedBase)) {
            throw DownloadException.ExtractionError(
                "Archive entry would extract outside target directory (ZIP slip attack): $targetPath",
                null
            )
        }
    }
}

/**
 * TAR.GZ archive extractor with compression support.
 */
class TarGzExtractor : ArchiveExtractor {

    override suspend fun extract(archivePath: Path, targetFileName: String, installDir: Path) =
        withContext(Dispatchers.IO) {
            logger.info { "Extracting TAR.GZ archive $archivePath to $installDir/$targetFileName" }

            Files.newInputStream(archivePath).use { fileInputStream ->
                GzipCompressorInputStream(fileInputStream).use { gzipStream ->
                    TarArchiveInputStream(gzipStream).use { tarStream ->
                        var entry: TarArchiveEntry?
                        val executableName = "./baml-cli"

                        while (tarStream.nextTarEntry.also { entry = it } != null) {
                            val currentEntry = entry ?: continue

                            if (currentEntry.name == executableName || currentEntry.name == "baml-cli") {
                                val targetPath = installDir.resolve(targetFileName)

                                // Validate path for security
                                validateExtractPath(targetPath, installDir)

                                Files.newOutputStream(targetPath).use { outputStream ->
                                    tarStream.copyTo(outputStream)
                                }

                                logger.info { "Extracted ${currentEntry.name} to $targetPath" }
                                return@withContext
                            }
                        }

                        throw DownloadException.ExtractionError(
                            "Required entry '$executableName' not found in TAR.GZ archive",
                            null
                        )
                    }
                }
            }
        }

    private fun validateExtractPath(targetPath: Path, baseDir: Path) {
        val normalizedTarget = targetPath.normalize()
        val normalizedBase = baseDir.normalize()

        if (!normalizedTarget.startsWith(normalizedBase)) {
            throw DownloadException.ExtractionError(
                "Archive entry would extract outside target directory (TAR slip attack): $targetPath",
                null
            )
        }
    }
}