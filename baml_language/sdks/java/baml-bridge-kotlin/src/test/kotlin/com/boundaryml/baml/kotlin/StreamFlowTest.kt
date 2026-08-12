package com.boundaryml.baml.kotlin

import baml_sdk.ai.stream.Done
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.test.runTest
import kotlin.test.Test
import kotlin.test.assertEquals

/**
 * [asFlow]'s drain semantics, exercised through the internal [streamFlow] seam
 * with a fake `next` supplier (a real `BamlStream` is engine-backed and `final`,
 * so it cannot be constructed offline; the seam keeps the loop testable).
 */
class StreamFlowTest {

    @Test
    fun drains_partials_and_stops_at_the_sentinel() = runTest {
        val items: List<Any?> = listOf("a", "b", "c", Done())
        val cursor = items.iterator()
        val flow: Flow<String> = streamFlow { cursor.next() }

        assertEquals(listOf("a", "b", "c"), flow.toList())
    }

    @Test
    fun the_sentinel_is_consumed_but_never_emitted() = runTest {
        val items: List<Any?> = listOf(Done())
        val cursor = items.iterator()
        val flow: Flow<String> = streamFlow { cursor.next() }

        assertEquals(emptyList(), flow.toList())
    }

    @Test
    fun null_partials_are_emitted_only_the_sentinel_terminates() = runTest {
        // `null` is a valid partial value (Done, not null, is the
        // terminator), so it must be emitted.
        val items: List<Any?> = listOf("a", null, "b", Done())
        val cursor = items.iterator()
        val flow: Flow<String?> = streamFlow { cursor.next() }

        assertEquals(listOf("a", null, "b"), flow.toList())
    }

    @Test
    fun is_cold_no_draining_until_collected() = runTest {
        var pulls = 0
        val flow: Flow<String> = streamFlow {
            pulls++
            Done()
        }
        // Building the flow drove nothing.
        assertEquals(0, pulls)
        flow.toList()
        assertEquals(1, pulls)
    }
}
