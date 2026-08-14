package com.boundaryml.baml.kotlin

import baml_bridge.BamlCallContext
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.awaitCancellation
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.runTest
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertTrue

/**
 * [withBamlContext] semantics — offline (no active engine calls are ever bound
 * to the context, so `abort()` never dispatches a native cancel; it only latches
 * the aborted flag, which is what this asserts).
 */
class WithBamlContextTest {

    @Test
    fun returns_block_result_without_aborting_on_normal_completion() = runTest {
        var captured: BamlCallContext? = null
        val result = withBamlContext { ctx ->
            captured = ctx
            "ok"
        }
        assertEquals("ok", result)
        assertFalse(captured!!.aborted(), "normal completion must not abort the context")
    }

    @Test
    fun does_not_abort_on_a_non_cancellation_exception() = runTest {
        var captured: BamlCallContext? = null
        val thrown = runCatching {
            withBamlContext { ctx ->
                captured = ctx
                throw IllegalStateException("boom")
            }
        }
        assertTrue(thrown.exceptionOrNull() is IllegalStateException)
        assertFalse(captured!!.aborted(), "a plain failure must not abort the context")
    }

    @Test
    fun aborts_the_context_when_the_coroutine_is_cancelled() = runTest {
        val captured = CompletableDeferred<BamlCallContext>()
        val job = launch {
            withBamlContext { ctx ->
                captured.complete(ctx)
                awaitCancellation()
            }
        }
        val ctx = captured.await()
        assertFalse(ctx.aborted(), "not aborted before cancellation")

        job.cancelAndJoin()
        assertTrue(ctx.aborted(), "coroutine cancellation must abort the BAML context")
    }
}
