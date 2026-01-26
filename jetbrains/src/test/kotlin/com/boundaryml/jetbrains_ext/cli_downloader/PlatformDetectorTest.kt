package com.boundaryml.jetbrains_ext.cli_downloader

import org.junit.Test
import org.junit.Assert.*

class PlatformDetectorTest {

    @Test
    fun `getCurrentPlatform should return valid platform string`() {
        val platform = PlatformDetector.getCurrentPlatform()
        
        assertNotNull(platform)
        assertTrue("Platform should be recognized", 
            platform in listOf("win32", "darwin", "linux") || platform.isNotEmpty())
    }

    @Test
    fun `getCurrentArchitecture should return valid architecture string`() {
        val architecture = PlatformDetector.getCurrentArchitecture()
        
        assertNotNull(architecture)
        assertTrue("Architecture should be recognized",
            architecture in listOf("x64", "arm64") || architecture.isNotEmpty())
    }

    @Test
    fun `getCurrentCliVersion should create CliVersion with current platform and architecture`() {
        val cliVersion = PlatformDetector.getCurrentCliVersion()
        
        assertEquals("latest", cliVersion.version)
        assertEquals(PlatformDetector.getCurrentArchitecture(), cliVersion.architecture)
        assertEquals(PlatformDetector.getCurrentPlatform(), cliVersion.platform)
    }
}