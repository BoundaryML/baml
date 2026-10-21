package com.boundaryml.baml.kotlin

import baml_bridge.InvocationCapture
import baml_bridge.internal.InvocationFrames
import java.util.concurrent.CompletableFuture
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.ThreadContextElement
import kotlinx.coroutines.asContextElement
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.future.await
import kotlinx.coroutines.future.future
import kotlin.coroutines.coroutineContext

/** Starts once after the cancellation check and awaits the original cancellable future. */
public suspend fun <T> bamlCall(start: () -> CompletableFuture<T>): T {
    coroutineContext.ensureActive()
    return start().await()
}

/** Explicit inheritance for coroutine continuations, restored on every suspension. */
public fun InvocationCapture.asContextElement(): ThreadContextElement<InvocationFrames.Frame?> =
    InvocationFrames.CURRENT.asContextElement(bridgeFrame())

/** Used inside a generated Java host callback to adapt a suspend body. */
public fun <T> suspendCallback(scope: CoroutineScope, body: suspend () -> T): CompletableFuture<T> {
    val frame = InvocationFrames.CURRENT.get()
    return scope.future(InvocationFrames.CURRENT.asContextElement(frame)) { body() }
}
