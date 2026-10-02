"""Run a host callback on the originating application's asyncio loop.

Rust schedules _start_dispatch with a fresh copy of the entry's Context. The
task owns its callable and arguments until actual execution completes, even
when the BAML caller stops waiting. Registration stores no loop or Context.
"""

import asyncio
from contextvars import ContextVar

from .baml_py import (
    _complete_host_call_error,
    _complete_host_call_success,
    _invoke_host_callable,
    _register_host_call_execution,
)


_active_dispatches = {}
_current_invocation = ContextVar("baml_invocation", default=None)


def _invoke_with_frame(callback, args, frame):
    from ._invocation import _Invocation

    frame = _Invocation(frame)
    token = _current_invocation.set(frame)
    try:
        result = _invoke_host_callable(callback, args)
    finally:
        _current_invocation.reset(token)
    if asyncio.iscoroutine(result):

        async def run():
            token = _current_invocation.set(frame)
            try:
                return await result
            finally:
                _current_invocation.reset(token)

        return run()
    return result


def _start_dispatch(callback, call_id, args, execution):
    loop = asyncio.get_running_loop()
    if not _register_host_call_execution(call_id, loop):
        execution.finish()
        return  # Cancelled before the scheduled callback could start.
    coroutine = _dispatch(callback, call_id, args, execution)
    try:
        task = loop.create_task(coroutine)
    except BaseException as error:
        coroutine.close()
        try:
            _complete_host_call_error(call_id, error)
        finally:
            execution.finish()
        return
    # asyncio keeps only weak references to tasks. Hold pending callbacks until
    # they finish, including callbacks whose BAML caller has stopped waiting.
    _active_dispatches[call_id] = task
    task.add_done_callback(lambda done: _dispatch_finished(call_id, done, execution))


def _dispatch_finished(call_id, task, execution):
    if _active_dispatches.get(call_id) is task:
        del _active_dispatches[call_id]
    try:
        if task.cancelled():
            # Cancellation before first execution never enters _dispatch's try.
            _complete_host_call_error(call_id, asyncio.CancelledError())
    finally:
        # A cancellation request is not exit. This callback runs only after the
        # task has actually finished unwinding, including its finally blocks.
        execution.finish()


def _cancel_dispatch(call_id):
    task = _active_dispatches.get(call_id)
    if task is not None and not task.done():
        task.cancel()


async def _dispatch(callback, call_id, args, execution):
    if not execution.start():
        return
    try:
        result = _invoke_with_frame(callback, args, execution.frame())
        if asyncio.iscoroutine(result):
            result = await result
    except BaseException as error:
        _complete_host_call_error(call_id, error)
    else:
        _complete_host_call_success(call_id, result)


def _run_sync_coroutine(call_id, coroutine):
    # Called on a bridge-owned worker with a copy of the synchronous caller's
    # Context. Never poll or replace a blocked application's event loop.
    async def run():
        loop = asyncio.get_running_loop()
        if not _register_host_call_execution(call_id, loop):
            coroutine.close()
            raise asyncio.CancelledError()
        task = asyncio.current_task()
        _active_dispatches[call_id] = task
        try:
            return await coroutine
        finally:
            if _active_dispatches.get(call_id) is task:
                del _active_dispatches[call_id]

    return asyncio.run(run())
