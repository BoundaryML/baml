package com.boundaryml.jetbrains_ext.cli_downloader

import org.junit.Test
import org.junit.Assert.*

class DownloadConfigTest {

    @Test
    fun `DownloadConfig should have sensible defaults`() {
        val config = DownloadConfig.default()
        
        assertEquals("https://github.com/BoundaryML/baml/releases/download", config.baseUrl)
        assertTrue(config.installPath.endsWith("/.baml"))
        assertEquals(TimeoutConfig.default(), config.timeouts)
        assertEquals(BackoffConfig.default(), config.backoff)
    }

    @Test
    fun `TimeoutConfig should have reasonable defaults`() {
        val config = TimeoutConfig.default()
        
        assertEquals(60_000L, config.binaryDownloadMs)
        assertEquals(10_000L, config.checksumDownloadMs)
        assertEquals(30_000L, config.extractionTimeoutMs)
    }

    @Test
    fun `BackoffConfig should have exponential backoff defaults`() {
        val config = BackoffConfig.default()
        
        assertEquals(10 * 60 * 1000L, config.initialDelayMs) // 10 minutes
        assertEquals(60 * 60 * 1000L, config.maxDelayMs)     // 1 hour
        assertEquals(5, config.maxFailureCountBeforeReset)
    }
}