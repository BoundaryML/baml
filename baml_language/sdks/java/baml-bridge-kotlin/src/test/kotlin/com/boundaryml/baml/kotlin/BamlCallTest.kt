package com.boundaryml.baml.kotlin

import java.util.concurrent.CompletableFuture
import kotlin.test.*
import kotlinx.coroutines.*
import kotlinx.coroutines.test.runTest
import org.junit.jupiter.api.Test

class BamlCallTest {
    @Test fun starts_once_kotlin_only() = runTest {
        var starts = 0
        assertEquals(7, bamlCall { starts++; CompletableFuture.completedFuture(7) })
        assertEquals(1, starts)
    }
    @Test fun pre_cancelled_job_does_not_start_kotlin_only() = runTest {
        var starts = 0
        val job = launch(start = CoroutineStart.LAZY) { bamlCall { starts++; CompletableFuture.completedFuture(7) }; Unit }
        job.cancel(); job.join()
        assertEquals(0, starts)
    }
    @Test fun cancellation_reaches_original_future_kotlin_only() = runTest {
        val original = CompletableFuture<Int>()
        val task = launch(start = CoroutineStart.UNDISPATCHED) { bamlCall<Int> { original }; Unit }
        task.cancelAndJoin()
        assertTrue(original.isCancelled)
    }
    @Test fun start_failure_propagates_kotlin_only() = runTest {
        val error = IllegalArgumentException("start failed")
        val caught = assertFailsWith<IllegalArgumentException> { bamlCall<Int> { throw error } }
        assertSame(error, caught)
    }
}
