from typing import Any, Awaitable, Callable, Dict, List, Literal, Optional, Sequence

# ---------------------------------------------------------------------------
# PyO3 runtime surface
# ---------------------------------------------------------------------------


class AbortController:
    def __init__(self) -> None: ...
    def abort(self) -> None: ...


class BamlHandle:
    def __init__(self, key: int, handle_type: int) -> None: ...
    @property
    def key(self) -> int: ...
    @property
    def handle_type(self) -> int: ...


class BamlPyHandle:
    def handle_type(self) -> int: ...


class UnknownHandle:
    def __init__(self, handle: BamlHandle) -> None: ...
    @property
    def key(self) -> int: ...
    @property
    def handle_type(self) -> int: ...


class HostSpanManager:
    def __init__(self) -> None: ...


class FunctionResult:
    def __init__(self, value: Any) -> None: ...
    def result(self) -> Any: ...


class Timing: ...
class Usage: ...
class LLMCall: ...


class FunctionLog:
    @property
    def id(self) -> str: ...
    @property
    def function_name(self) -> str: ...
    @property
    def timing(self) -> Timing: ...
    @property
    def usage(self) -> Usage: ...
    @property
    def calls(self) -> List["FunctionLog"]: ...
    @property
    def tags(self) -> Dict[str, str]: ...
    @property
    def result(self) -> Any: ...


class Collector:
    def __init__(self, name: Optional[str] = ...) -> None: ...
    @property
    def logs(self) -> List[FunctionLog]: ...
    @property
    def last(self) -> Optional[FunctionLog]: ...
    def id(self, function_log_id: str) -> Optional[FunctionLog]: ...


class BamlRuntime:
    @staticmethod
    def initialize_runtime(
        root_path: str,
        files: Dict[str, str],
        *,
        sdk_root: str,
    ) -> "BamlRuntime": ...
    def call_function(
        self,
        function_name: str,
        args_proto: bytes,
        ctx: Optional[HostSpanManager] = ...,
        collectors: Optional[List[Collector]] = ...,
        abort_controller: Optional[AbortController] = ...,
    ) -> Awaitable[bytes]: ...
    def call_function_sync(
        self,
        function_name: str,
        args_proto: bytes,
        ctx: Optional[HostSpanManager] = ...,
        collectors: Optional[List[Collector]] = ...,
        abort_controller: Optional[AbortController] = ...,
    ) -> bytes: ...
    @property
    def _sdk_root(self) -> str: ...


# ---------------------------------------------------------------------------
# Exceptions
# ---------------------------------------------------------------------------


class BamlError(Exception): ...
class BamlInvalidArgumentError(BamlError): ...
class BamlClientError(BamlError): ...
class BamlCancelledError(BamlError): ...


# ---------------------------------------------------------------------------
# Module-level functions
# ---------------------------------------------------------------------------


def get_runtime() -> BamlRuntime: ...
def get_version() -> str: ...
def flush_events() -> None: ...


def encode_call_args(kwargs: Dict[str, Any]) -> bytes: ...
def decode_call_result(data: bytes) -> Any: ...


def call_function_sync(
    rt: BamlRuntime,
    function_name: str,
    kwargs: Dict[str, Any],
    ctx: Optional[HostSpanManager] = ...,
    collectors: Optional[List[Collector]] = ...,
    abort_controller: Optional[AbortController] = ...,
) -> FunctionResult: ...


async def call_function(
    rt: BamlRuntime,
    function_name: str,
    kwargs: Dict[str, Any],
    ctx: Optional[HostSpanManager] = ...,
    collectors: Optional[List[Collector]] = ...,
    abort_controller: Optional[AbortController] = ...,
) -> FunctionResult: ...


def define_function(
    baml_fqn: str,
    mode: Literal["sync", "async"],
    param_names: List[str],
) -> Callable[..., Any]: ...


def define_static_method(
    baml_fqn: str,
    mode: Literal["sync", "async"],
    param_names: List[str],
) -> Callable[..., Any]: ...


def define_instance_method(
    baml_fqn: str,
    mode: Literal["sync", "async"],
    param_names: List[str],
) -> Callable[..., Any]: ...


# ---------------------------------------------------------------------------
# Trace context manager
# ---------------------------------------------------------------------------


class BamlCtxManager:
    def __init__(self, rt: BamlRuntime) -> None: ...
    def trace_fn(self, func: Callable[..., Any]) -> Callable[..., Any]: ...
    def upsert_tags(self, **tags: str) -> None: ...
    def flush(self) -> None: ...
