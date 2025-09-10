package com.boundaryml.jetbrains_ext.cli_downloader

import mu.KotlinLogging
import java.util.concurrent.ConcurrentHashMap

private val logger = KotlinLogging.logger {}

/**
 * Manages exponential backoff for download failures with concurrent protection.
 */
class BackoffManager(private val config: BackoffConfig) {

    private val backoffState = ConcurrentHashMap<String, BackoffState>()
    private val downloadsInProgress = ConcurrentHashMap.newKeySet<String>()

    fun shouldAttemptDownload(version: String): BackoffResult {
        // Check if download is already in progress
        if (downloadsInProgress.contains(version)) {
            return BackoffResult.AlreadyInProgress(version)
        }

        // Check backoff state
        val state = backoffState[version] ?: return BackoffResult.ShouldAttempt

        val backoffDelay = calculateBackoffDelay(state.failureCount)
        val nextAttemptTime = state.lastAttemptTimestamp + backoffDelay
        val currentTime = System.currentTimeMillis()

        if (currentTime < nextAttemptTime) {
            val waitTimeMs = nextAttemptTime - currentTime
            return BackoffResult.ShouldWait(waitTimeMs, "Exponential backoff active")
        }

        return BackoffResult.ShouldAttempt
    }

    fun markDownloadStarted(version: String) {
        downloadsInProgress.add(version)
        logger.debug { "Marked download started for version $version" }
    }

    fun markDownloadCompleted(version: String) {
        downloadsInProgress.remove(version)
        logger.debug { "Marked download completed for version $version" }
    }

    fun recordFailure(version: String) {
        val currentTime = System.currentTimeMillis()
        val currentState = backoffState[version]

        val newState = if (currentState == null) {
            BackoffState(failureCount = 1, lastAttemptTimestamp = currentTime)
        } else {
            val newCount = if (currentState.failureCount >= config.maxFailureCountBeforeReset) {
                logger.warn { "Resetting failure count for $version after reaching max failures" }
                1
            } else {
                currentState.failureCount + 1
            }

            BackoffState(failureCount = newCount, lastAttemptTimestamp = currentTime)
        }

        backoffState[version] = newState

        val nextDelay = calculateBackoffDelay(newState.failureCount)
        logger.warn {
            "Recorded download failure for $version (count: ${newState.failureCount}). " +
                    "Next attempt allowed in ${nextDelay / 1000 / 60} minutes"
        }
    }

    fun clearBackoff(version: String) {
        backoffState.remove(version)
        logger.info { "Cleared backoff state for version $version" }
    }

    private fun calculateBackoffDelay(failureCount: Int): Long {
        val exponentialDelay = config.initialDelayMs * (1L shl (failureCount - 1))
        return minOf(exponentialDelay, config.maxDelayMs)
    }
}

/**
 * Result of checking whether a download should be attempted.
 */
sealed class BackoffResult {
    object ShouldAttempt : BackoffResult()
    data class ShouldWait(val waitTimeMs: Long, val reason: String) : BackoffResult()
    data class AlreadyInProgress(val version: String) : BackoffResult()
}