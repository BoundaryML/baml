package com.boundaryml.jetbrains_ext

import com.intellij.openapi.progress.ProgressIndicator
import com.intellij.openapi.progress.ProgressManager
import com.intellij.util.io.HttpRequests
import com.redhat.devtools.lsp4ij.installation.LanguageServerInstallerBase
import kotlinx.serialization.SerialName
import org.apache.commons.compress.archivers.tar.TarArchiveEntry
import org.apache.commons.compress.archivers.tar.TarArchiveInputStream
import org.apache.commons.compress.compressors.gzip.GzipCompressorInputStream
import java.nio.file.*
import java.nio.file.attribute.PosixFilePermission
import java.security.MessageDigest
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json
import java.util.zip.ZipInputStream

class BamlLanguageServerInstaller : LanguageServerInstallerBase() {

    private val REPO = "BoundaryML/baml"
    private val GH_API_LATEST = "https://api.github.com/repos/$REPO/releases/latest"
    private val GH_RELEASES_BASE = "https://github.com/$REPO/releases/download"

    private val bamlCacheDir: Path = Path.of(System.getProperty("user.home"), ".baml/jetbrains")
    private val breadcrumbFile: Path = bamlCacheDir.resolve("baml-cli-installed.txt")

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

    override fun checkServerInstalled(indicator: ProgressIndicator): Boolean {
        super.progress("Checking if BAML CLI is installed...", indicator)
        ProgressManager.checkCanceled()
        return Files.exists(breadcrumbFile)
    }

    override fun install(indicator: ProgressIndicator) {
        super.progress("Installing BAML CLI...", indicator)
        ProgressManager.checkCanceled()

        val latestVersion = fetchLatestReleaseVersion(indicator)
        val (arch, platform, extension) = getPlatformTriple()

        val artifactName = "baml-cli-$latestVersion-$arch-$platform"
        val destDir = bamlCacheDir.resolve(artifactName)
        val targetExecutable = if (platform == "pc-windows-msvc")
            destDir.resolve("baml-cli.exe")
        else
            destDir.resolve("baml-cli")

        if (Files.exists(targetExecutable)) {
            super.progress("BAML CLI already installed.", indicator)
            return
        }

        val archivePath = downloadFile(artifactName, extension, latestVersion, indicator)
        verifyChecksum(artifactName, extension, latestVersion, archivePath, indicator)

        super.progress("Extracting BAML CLI...", indicator)
        extractArchive(archivePath, extension, destDir)

        Files.deleteIfExists(archivePath)
        setExecutable(targetExecutable)

        Files.createDirectories(bamlCacheDir)
        Files.writeString(breadcrumbFile, latestVersion)

        super.progress("Installation complete!", 1.0, indicator)
    }

    @Serializable
    data class GitHubRelease(
        @SerialName("tag_name")
        val tagName: String
    )

    private fun fetchLatestReleaseVersion(indicator: ProgressIndicator): String {
        super.progress("Fetching latest version info...", indicator)
        ProgressManager.checkCanceled()

        return try {
            val jsonText = HttpRequests.request(GH_API_LATEST).readString()
            val jsonParser = Json {
                ignoreUnknownKeys = true
            }
            val release = jsonParser.decodeFromString<GitHubRelease>(jsonText)

            release.tagName.removePrefix("v")
        } catch (e: Exception) {
            super.progress("GitHub fetch failed, falling back to local cache...", indicator)
            // hardcoded fallback to 0.89
            // TODO: fallback to latest downloaded version
            "0.89.0"
        }
    }

    private fun downloadFile(artifactName: String, extension: String, version: String, indicator: ProgressIndicator): Path {
        val url = "$GH_RELEASES_BASE/$version/$artifactName.$extension"
        val tempFile = Files.createTempFile("baml-cli", ".$extension")

        super.progress("Downloading $artifactName...", indicator)
        ProgressManager.checkCanceled()

        HttpRequests.request(url).connect { request ->
            request.saveToFile(tempFile.toFile(), indicator)
        }

        return tempFile
    }

    private fun verifyChecksum(artifactName: String, extension: String, version: String, archivePath: Path, indicator: ProgressIndicator) {
        super.progress("Verifying checksum...", indicator)
        ProgressManager.checkCanceled()

        val checksumUrl = "$GH_RELEASES_BASE/$version/$artifactName.$extension.sha256"
        val checksumContent = HttpRequests.request(checksumUrl).readString().trim()
        val expectedChecksum = checksumContent.split(Regex("\\s+"))[0]
        val actualChecksum = calculateSha256(archivePath)

        if (!expectedChecksum.equals(actualChecksum, ignoreCase = true)) {
            throw IllegalStateException("Checksum mismatch! Expected $expectedChecksum, got $actualChecksum")
        }
    }

    private fun calculateSha256(file: Path): String {
        val digest = MessageDigest.getInstance("SHA-256")
        Files.newInputStream(file).use { stream ->
            val buffer = ByteArray(8192)
            var read: Int
            while (stream.read(buffer).also { read = it } != -1) {
                digest.update(buffer, 0, read)
            }
        }
        return digest.digest().joinToString("") { "%02x".format(it) }
    }

    private fun extractArchive(archivePath: Path, extension: String, destDir: Path) {
        Files.createDirectories(destDir)

        if (extension == "tar.gz") {
            Files.newInputStream(archivePath).use { fileIn ->
                GzipCompressorInputStream(fileIn).use { gzipIn ->
                    TarArchiveInputStream(gzipIn).use { tarIn ->
                        var entry: TarArchiveEntry? = tarIn.nextTarEntry
                        while (entry != null) {
                            val sanitizedName = sanitizePath(entry.name)
                            val outPath = destDir.resolve(sanitizedName)
                            
                            // Ensure the resolved path is within the destination directory
                            if (!outPath.startsWith(destDir)) {
                                throw SecurityException("Archive entry path would escape destination directory: ${entry.name}")
                            }
                            
                            if (entry.isDirectory) {
                                Files.createDirectories(outPath)
                            } else {
                                Files.createDirectories(outPath.parent)
                                Files.copy(tarIn, outPath, StandardCopyOption.REPLACE_EXISTING)
                            }
                            entry = tarIn.nextTarEntry
                        }
                    }
                }
            }
        } else if (extension == "zip") {
            ZipInputStream(Files.newInputStream(archivePath)).use { zipIn ->
                var entry = zipIn.nextEntry
                while (entry != null) {
                    val sanitizedName = sanitizePath(entry.name)
                    val outPath = destDir.resolve(sanitizedName)
                    
                    // Ensure the resolved path is within the destination directory
                    if (!outPath.startsWith(destDir)) {
                        throw SecurityException("Archive entry path would escape destination directory: ${entry.name}")
                    }
                    
                    if (entry.isDirectory) {
                        Files.createDirectories(outPath)
                    } else {
                        Files.createDirectories(outPath.parent)
                        Files.copy(zipIn, outPath, StandardCopyOption.REPLACE_EXISTING)
                    }
                    entry = zipIn.nextEntry
                }
            }
        } else {
            throw IllegalArgumentException("Unsupported archive extension: $extension")
        }
    }

    private fun sanitizePath(path: String): String {
        // Normalize the path and remove any ".." components
        return Path.of(path).normalize().toString().replace("..", "")
    }

    private fun setExecutable(file: Path) {
        if (!Files.exists(file)) return
        try {
            val perms = Files.getPosixFilePermissions(file)
            val updatedPerms = perms + setOf(
                PosixFilePermission.OWNER_EXECUTE,
            )
            Files.setPosixFilePermissions(file, updatedPerms)
        } catch (e: UnsupportedOperationException) {
            // Windows doesn't support POSIX permissions — safely ignore
        }
    }
}
