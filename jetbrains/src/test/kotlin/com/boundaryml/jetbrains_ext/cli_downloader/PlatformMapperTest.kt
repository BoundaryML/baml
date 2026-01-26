package com.boundaryml.jetbrains_ext.cli_downloader

import org.junit.Test
import org.junit.Assert.*

class PlatformMapperTest {

    @Test
    fun `toGitHubReleaseArchitecture should map architectures correctly`() {
        assertEquals("x86_64", PlatformMapper.toGitHubReleaseArchitecture("x64"))
        assertEquals("aarch64", PlatformMapper.toGitHubReleaseArchitecture("arm64"))
    }

    @Test
    fun `toGitHubReleasePlatform should map platforms correctly`() {
        assertEquals("pc-windows-msvc", PlatformMapper.toGitHubReleasePlatform("win32"))
        assertEquals("apple-darwin", PlatformMapper.toGitHubReleasePlatform("darwin"))
        
        val linuxPlatform = PlatformMapper.toGitHubReleasePlatform("linux")
        assertTrue("Linux platform should be gnu or musl",
            linuxPlatform == "unknown-linux-gnu" || linuxPlatform == "unknown-linux-musl")
    }

    @Test
    fun `getExecutableName should handle Windows vs Unix`() {
        assertEquals("baml-cli.exe", PlatformMapper.getExecutableName("win32"))
        assertEquals("baml-cli", PlatformMapper.getExecutableName("darwin"))
        assertEquals("baml-cli", PlatformMapper.getExecutableName("linux"))
    }

    @Test
    fun `getArchiveExtension should return correct format`() {
        assertEquals("zip", PlatformMapper.getArchiveExtension("win32"))
        assertEquals("tar.gz", PlatformMapper.getArchiveExtension("darwin"))
        assertEquals("tar.gz", PlatformMapper.getArchiveExtension("linux"))
    }
}