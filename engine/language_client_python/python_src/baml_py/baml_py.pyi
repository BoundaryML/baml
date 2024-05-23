from typing import Any, Callable, Dict, Optional

class FunctionResult:
    """The result of a BAML function call.

    Represents any of:

        - a successful LLM call, with a successful type parse
        - a successful LLM call, with a failed type parse
        - a failed LLM call, due to a provider outage or other network error
        - a failed LLM call, due to an inability to build the request
        - or any other outcome, really

    We only expose the parsed result to Python right now.
    """

    def __str__(self) -> str: ...
    def parsed(self) -> Any: ...

class FunctionResultStream:
    """The result of a BAML function stream.

    Provides a callback interface to receive events from a BAML result stream.

    Use `on_event` to set the callback, and `done` to drive the stream to completion.
    """

    def __str__(self) -> str: ...
    def on_event(
        self, on_event: Callable[[FunctionResult], None]
    ) -> FunctionResultStream: ...
    async def done(self) -> Any: ...

class Image:
    def __init__(
        self, url: Optional[str] = None, base64: Optional[str] = None
    ) -> None: ...
    @property
    def url(self) -> Optional[str]: ...
    @url.setter
    def url(self, value: Optional[str]) -> None: ...
    @property
    def base64(self) -> Optional[str]: ...
    @base64.setter
    def base64(self, value: Optional[str]) -> None: ...

class BamlRuntimeFfi:
    @staticmethod
    def from_directory(directory: str, ctx: Dict[str, Any] = {}) -> BamlRuntimeFfi: ...
    @staticmethod
    def from_encoded(encoded: str) -> BamlRuntimeFfi: ...
    async def call_function(
        self,
        function_name: str,
        args: Dict[str, Any],
        ctx: Dict[str, Any],
    ) -> FunctionResult: ...
    def stream_function(
        self,
        function_name: str,
        args: Dict[str, Any],
        ctx: Dict[str, Any],
        on_event: Optional[Callable[[FunctionResult], None]] = None,
    ) -> FunctionResultStream: ...

class BamlSpan:
    @staticmethod
    def new(
        runtime: BamlRuntimeFfi,
        function_name: str,
        ctx: Dict[str, Any],
        args: Dict[str, Any],
    ) -> BamlSpan: ...
    async def finish(self, result: Any) -> str | None: ...

def invoke_runtime_cli() -> None: ...
