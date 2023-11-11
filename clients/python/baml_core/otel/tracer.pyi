from typing import Any, Callable, TypeVar
from typing import overload

F = TypeVar("F", bound=Callable[..., Any])  # Function type

@overload
def trace(func: F) -> F: ...
@overload
def trace(*, name: str) -> Callable[[F], F]: ...
def _trace_internal(func: F, **kwargs: Any) -> F: ...
