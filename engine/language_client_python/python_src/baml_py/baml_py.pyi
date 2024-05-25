from typing import Any, Callable, Dict, Optional

class FunctionResultPy:
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

class FunctionResultStreamPy:
    """The result of a BAML function stream.

    Provides a callback interface to receive events from a BAML result stream.

    Use `on_event` to set the callback, and `done` to drive the stream to completion.
    """

    def __str__(self) -> str: ...
    def on_event(
        self, on_event: Callable[[FunctionResultPy], None]
    ) -> FunctionResultStreamPy: ...
    async def done(self, ctx: RuntimeContextManagerPy) -> FunctionResultPy: ...

class BamlImagePy:
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

class RuntimeContextManagerPy:
    def upsert_tags(self, tags: Dict[str, Any]) -> None: ...
    def deep_clone(self) -> RuntimeContextManagerPy: ...

class BamlRuntimePy:
    @staticmethod
    def from_directory(directory: str, env_vars: Dict[str, str]) -> BamlRuntimePy: ...
    async def call_function(
        self,
        function_name: str,
        args: Dict[str, Any],
        ctx: RuntimeContextManagerPy,
    ) -> FunctionResultPy: ...
    def stream_function(
        self,
        function_name: str,
        args: Dict[str, Any],
        on_event: Optional[Callable[[FunctionResultPy], None]],
        ctx: RuntimeContextManagerPy,
    ) -> FunctionResultStreamPy: ...
    def create_context_manager(self) -> RuntimeContextManagerPy: ...
    def flush(self) -> None: ...

class BamlSpanPy:
    @staticmethod
    def new(
        runtime: BamlRuntimePy,
        function_name: str,
        args: Dict[str, Any],
        ctx: RuntimeContextManagerPy,
    ) -> BamlSpanPy: ...
    async def finish(self, result: Any, ctx: RuntimeContextManagerPy) -> str | None: ...

def invoke_runtime_cli() -> None: ...
