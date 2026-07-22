package com.boundaryml.baml.kotlin

import baml_bridge.BamlCallContext
import kotlinx.coroutines.CancellationException

/**
 * Runs [block] with a fresh [BamlCallContext] whose lifetime is tied to the
 * surrounding coroutine: if the coroutine is cancelled while [block] suspends,
 * the resulting [CancellationException] triggers [BamlCallContext.abort],
 * cancelling every in-flight BAML call bound to the context before the exception
 * propagates.
 *
 * Pass the received context to any generated binding's trailing-`ctx` overload
 * (`Fns.myFunc(args, ctx)` / `Fns.myFunc_async(args, ctx).await()`), or to a
 * streaming call, so coroutine cancellation reaches the engine. This is the
 * coroutine-native counterpart of the Java surface, where you hold a
 * `BamlCallContext` yourself and call [BamlCallContext.abort] explicitly.
 *
 * Semantics:
 * - Normal completion of [block] returns its value and does NOT abort (the calls
 *   already finished).
 * - A non-cancellation exception from [block] propagates unchanged, WITHOUT an
 *   abort (those calls already failed on their own).
 * - Coroutine cancellation (a [CancellationException] crossing this frame)
 *   aborts the context, then rethrows the same exception — structured-
 *   concurrency-correct, never swallowing cancellation. [BamlCallContext.abort]
 *   is a fast, non-suspending native cancel, so it runs correctly even though
 *   the coroutine is already in its cancelled state.
 */
public suspend fun <T> withBamlContext(block: suspend (BamlCallContext) -> T): T {
    val ctx = BamlCallContext()
    try {
        return block(ctx)
    } catch (cancellation: CancellationException) {
        ctx.abort()
        throw cancellation
    }
}
