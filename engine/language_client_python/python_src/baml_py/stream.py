from __future__ import annotations
from .baml_py import (
    FunctionResult,
    FunctionResultStream,
    SyncFunctionResultStream,
    RuntimeContextManager,
)
from typing import Callable, Generic, Optional, TypeVar
import threading
import asyncio
import concurrent.futures

import queue

PartialOutputType = TypeVar("PartialOutputType")
FinalOutputType = TypeVar("FinalOutputType")


class BamlStream(Generic[PartialOutputType, FinalOutputType]):
    __ffi_stream: FunctionResultStream
    __partial_coerce: Callable[[FunctionResult], PartialOutputType]
    __final_coerce: Callable[[FunctionResult], FinalOutputType]
    __ctx_manager: RuntimeContextManager
    __task: Optional[threading.Thread]
    __event_queue: queue.Queue[Optional[FunctionResult]]
    __future: concurrent.futures.Future[FunctionResult]
    __is_done: bool

    def __init__(
        self,
        ffi_stream: FunctionResultStream,
        partial_coerce: Callable[[FunctionResult], PartialOutputType],
        final_coerce: Callable[[FunctionResult], FinalOutputType],
        ctx_manager: RuntimeContextManager,
    ):
        self.__ffi_stream = ffi_stream.on_event(self.__enqueue)
        self.__partial_coerce = partial_coerce
        self.__final_coerce = final_coerce
        self.__ctx_manager = ctx_manager
        self.__task = None
        self.__event_queue = queue.Queue()
        self.__future = concurrent.futures.Future()  # Initialize the future here
        self.__is_done = False

    def __enqueue(self, data: FunctionResult) -> None:
        self.__event_queue.put_nowait(data)

    async def __drive_to_completion(self) -> FunctionResult:
        try:
            retval = await self.__ffi_stream.done(self.__ctx_manager)

            self.__future.set_result(retval)
           
            return retval
        except Exception as e:
            self.__future.set_exception(e)
            raise
        finally:
            # Remove the callback, so that the ffi_stream can be GC'd
            # If we don't do this, the ffi_stream *and* this BamlStream object
            # will never get collected since they circularly reference each other.
            self.__ffi_stream.on_event(None)  # type: ignore
            self.__is_done = True
            self.__event_queue.put_nowait(None)

    def __drive_to_completion_in_bg(self) -> concurrent.futures.Future[FunctionResult]:
        if self.__task is None and not self.__is_done:
            self.__task = threading.Thread(target=self.threading_target, daemon=True)
            self.__task.start()
        return self.__future

    def threading_target(self):
        asyncio.run(self.__drive_to_completion(), debug=True)

    async def __aiter__(self):
        # TODO: This is deliberately __aiter__ and not __iter__ because we want to
        # ensure that the caller is using an async for loop.
        # Eventually we do not want to create a new thread for each stream.
        try:
            self.__drive_to_completion_in_bg()
            while True:
                try:
                    event = self.__event_queue.get_nowait()
                    if event is None:
                        break
                    if event.is_ok():
                        yield self.__partial_coerce(event)
                except queue.Empty:
                    await asyncio.sleep(0.050)
        except Exception as e:
            raise e
        finally:
            if self.__task and self.__task.is_alive():
                self.__task.join(timeout=5.0)

    async def get_final_response(self):
        final = self.__drive_to_completion_in_bg()
        return self.__final_coerce((await asyncio.wrap_future(final)))


class BamlSyncStream(Generic[PartialOutputType, FinalOutputType]):
    __ffi_stream: SyncFunctionResultStream
    __partial_coerce: Callable[[FunctionResult], PartialOutputType]
    __final_coerce: Callable[[FunctionResult], FinalOutputType]
    __ctx_manager: RuntimeContextManager
    __task: Optional[threading.Thread]
    __event_queue: queue.Queue[Optional[FunctionResult]]
    __result: Optional[FunctionResult]
    __exception: Optional[Exception]

    def __init__(
        self,
        ffi_stream: SyncFunctionResultStream,
        partial_coerce: Callable[[FunctionResult], PartialOutputType],
        final_coerce: Callable[[FunctionResult], FinalOutputType],
        ctx_manager: RuntimeContextManager,
    ):
        self.__ffi_stream = ffi_stream.on_event(self.__enqueue)
        self.__partial_coerce = partial_coerce
        self.__final_coerce = final_coerce
        self.__ctx_manager = ctx_manager
        self.__task = None
        self.__event_queue = queue.Queue()
        self.__result = None
        self.__exception = None
        self.__is_done = False

    def __enqueue(self, data: FunctionResult) -> None:
        self.__event_queue.put_nowait(data)

    def __drive_to_completion(self) -> FunctionResult:
        try:
            retval = self.__ffi_stream.done(self.__ctx_manager)
            self.__result = retval
            # Remove the callback, so that the ffi_stream can be GC'd
            # If we don't do this, the ffi_stream *and* this BamlStream object
            # will never get collected since they circularly reference each other.
            self.__ffi_stream.on_event(None)  # type: ignore

            return retval
        except Exception as e:
            self.__exception = e
            raise e
        finally:
            self.__is_done = True
            self.__event_queue.put_nowait(None)

    def __drive_to_completion_in_bg(self):
        if self.__task is None and not self.__is_done:
            self.__task = threading.Thread(target=self.__threading_target, daemon=True)
            self.__task.start()

    def __threading_target(self):
        self.__drive_to_completion()

    def __iter__(self):
        # TODO: This is deliberately __iter__ and not __aiter__ because we want to
        # ensure that the caller is NOT using an async for loop.
        self.__drive_to_completion_in_bg()

        try:
            while True:
                event = self.__event_queue.get()
                if event is None:
                    break
                if event.is_ok():
                    yield self.__partial_coerce(event)
        except Exception as e:
            raise e
        finally:
            if self.__task and self.__task.is_alive():
                self.__task.join(timeout=5.0)

    def get_final_response(self):
        self.__drive_to_completion_in_bg()
        if self.__task is not None:
            self.__task.join()

        if self.__exception is not None:
            raise self.__exception

        if self.__result is None:
            raise Exception(
                "BAML Internal error: Stream did not complete successfully. Please report this issue."
            )

        return self.__final_coerce(self.__result)
