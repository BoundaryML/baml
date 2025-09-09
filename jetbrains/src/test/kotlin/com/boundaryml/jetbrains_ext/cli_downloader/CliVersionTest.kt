package com.boundaryml.jetbrains_ext.cli_downloader

import org.junit.Test
import org.junit.Assert.*

class CliVersionTest {

    @Test
    fun `CliVersion should create correct artifact name`() {
        val cliVersion = CliVersion("1.0.0", "x64", "darwin")
        assertEquals("baml-cli-1.0.0-x64-darwin", cliVersion.toArtifactName())
    }

    @Test
    fun `DownloadResult Success should contain file path`() {
        val result = DownloadResult.Success("/path/to/cli")
        assertTrue(result is DownloadResult.Success)
        assertEquals("/path/to/cli", result.filePath)
    }

    @Test
    fun `ChecksumMismatch should format message correctly`() {
        val exception = DownloadException.ChecksumMismatch("expected123", "actual456")
        assertEquals("Checksum mismatch: expected expected123, got actual456", exception.message)
    }
}