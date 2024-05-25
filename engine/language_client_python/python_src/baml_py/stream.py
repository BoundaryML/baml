from .baml_py import FunctionResultPy, FunctionResultStreamPy, RuntimeContextManagerPy
from typing import Callable, Generic, Optional, TypeVar
import asyncio

PartialOutputType = TypeVar("PartialOutputType")
FinalOutputType = TypeVar("FinalOutputType")


class BamlStream(Generic[PartialOutputType, FinalOutputType]):
    __ffi_stream: FunctionResultStreamPy
    __partial_coerce: Callable[[FunctionResultPy], PartialOutputType]
    __final_coerce: Callable[[FunctionResultPy], FinalOutputType]
    __ctx_manager: RuntimeContextManagerPy

    __task: Optional[asyncio.Task[FunctionResultPy]] = None
    __event_queue: asyncio.Queue[Optional[FunctionResultPy]] = asyncio.Queue()

    def __init__(
        self,
        ffi_stream: FunctionResultStreamPy,
        partial_coerce: Callable[[FunctionResultPy], PartialOutputType],
        final_coerce: Callable[[FunctionResultPy], FinalOutputType],
        ctx_manager: RuntimeContextManagerPy,
    ):
        self.__ffi_stream = ffi_stream.on_event(self.__enqueue)
        self.__partial_coerce = partial_coerce
        self.__final_coerce = final_coerce
        self.__ctx_manager = ctx_manager

    def __enqueue(self, data: FunctionResultPy) -> None:
        self.__event_queue.put_nowait(data)

    async def __drive_to_completion(self) -> FunctionResultPy:
        try:
            retval = await self.__ffi_stream.done(self.__ctx_manager)
            return retval
        finally:
            self.__event_queue.put_nowait(None)

    def __drive_to_completion_in_bg(self) -> asyncio.Task[FunctionResultPy]:
        # Doing this without using a compare-and-swap or lock is safe,
        # because we don't cross an await point during it
        if self.__task is None:
            self.__task = asyncio.create_task(self.__drive_to_completion())

        return self.__task

    async def __aiter__(self):
        self.__drive_to_completion_in_bg()
        while True:
            event = await self.__event_queue.get()
            if event is None:
                break
            yield self.__partial_coerce(event.parsed())

    async def done(self):
        final = await self.__drive_to_completion_in_bg()
        return self.__final_coerce(final.parsed())
