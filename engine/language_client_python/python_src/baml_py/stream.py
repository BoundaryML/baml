from .baml_py import FunctionResult, FunctionResultStream
from typing import Callable, Generic, Optional, TypeVar
import asyncio


class CallbackOnTimer:
    __callback: Callable[[int], None]

    def __init__(self, cb: Callable[[int], None]):
        self.__callback = cb

    async def done(self) -> str:
        for i in range(3):
            self.__callback(i)
            await asyncio.sleep(1)
        return "final message"


PartialOutputType = TypeVar("PartialOutputType")
FinalOutputType = TypeVar("FinalOutputType")


class BamlStream(Generic[PartialOutputType, FinalOutputType]):
    __ffi_stream: FunctionResultStream
    __partial_coerce: Callable[[FunctionResult], PartialOutputType]
    __final_coerce: Callable[[FunctionResult], FinalOutputType]

    __started: bool = False
    __event_queue: asyncio.Queue[Optional[FunctionResult]] = asyncio.Queue()
    __done_queue: asyncio.Queue[FunctionResult] = asyncio.Queue(1)

    def __init__(
        self,
        ffi_stream: FunctionResultStream,
        partial_coerce: Callable[[FunctionResult], PartialOutputType],
        final_coerce: Callable[[FunctionResult], FinalOutputType],
    ):
        self.__ffi_stream = ffi_stream.on_event(self.__enqueue)
        self.__partial_coerce = partial_coerce
        self.__final_coerce = final_coerce

    def __enqueue(self, data: FunctionResult) -> None:
        self.__event_queue.put_nowait(data)

    async def __drive_to_completion(self) -> None:
        retval = await self.__ffi_stream.done()
        self.__event_queue.put_nowait(None)
        self.__done_queue.put_nowait(retval)
      
    def __drive_to_completion_in_bg(self) -> None:
        # Doing this without using a compare-and-swap or lock is safe,
        # because we don't cross an await point during it
        if self.__started:
            return
        self.__started = True

        asyncio.create_task(self.__drive_to_completion())

    async def __aiter__(self):
        self.__drive_to_completion_in_bg()
        while True:
            event = await self.__event_queue.get()
            if event is None:
                break
            yield self.__partial_coerce(event.parsed())

    async def done(self):
        self.__drive_to_completion_in_bg()
        return self.__final_coerce((await self.__done_queue.get()).parsed())