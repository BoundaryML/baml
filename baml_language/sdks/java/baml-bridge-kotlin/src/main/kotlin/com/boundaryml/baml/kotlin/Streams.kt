package com.boundaryml.baml.kotlin

import baml_bridge.BamlStream
import baml_sdk.ai.stream.Done
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.future.await

/**
 * A cold [Flow] of this stream's partial values. Collecting drives the stream:
 * each step awaits `next_async()` and emits the partial, stopping — WITHOUT
 * emitting — when the engine returns the [Done] sentinel. Cold: no
 * work happens until the flow is collected, and cancelling the collector stops
 * driving the stream (the suspended `await()` is cancellable and cancels the
 * underlying engine call).
 *
 * A `BamlStream` is single-pass (the engine advances an underlying handle), so
 * collect the returned flow at most once — re-collecting would continue from
 * wherever the stream currently is, not restart it.
 *
 * @see awaitFinal for the completed value.
 */
public fun <P, F> BamlStream<P, F>.asFlow(): Flow<P> = streamFlow { next_async().await() }

/**
 * Suspends until the stream's final (completed) value is available —
 * `get_final_async().await()`, the coroutine-friendly form of
 * [BamlStream.get_final].
 */
public suspend fun <P, F> BamlStream<P, F>.awaitFinal(): F = get_final_async().await()

/**
 * The drain loop behind [asFlow], factored out as an internal seam so it can be
 * unit-tested offline against a fake `next` supplier (a real `BamlStream` is
 * engine-backed and `final`, hence unmockable). Emits every value `next` yields
 * until it yields the [Done] sentinel, which is consumed but never
 * emitted; a `null` partial is a legitimate value and IS emitted (only the
 * sentinel terminates).
 */
internal fun <P> streamFlow(next: suspend () -> Any?): Flow<P> = flow {
    while (true) {
        val value = next()
        if (value is Done) {
            break
        }
        @Suppress("UNCHECKED_CAST")
        emit(value as P)
    }
}
