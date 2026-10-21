from typing import Callable, Optional, TypeVar, overload
from typing_extensions import ParamSpec

_P = ParamSpec("_P")
_R = TypeVar("_R")

class TraceUsageError(TypeError): ...

_T = TypeVar("_T")

def register_capture(
    value_type: type[_T], handler: Callable[[_T], object]
) -> Callable[[_T], object]: ...
def capture_for(
    value_type: type[_T],
) -> Callable[[Callable[[_T], object]], Callable[[_T], object]]: ...
@overload
def instrument(
    function_or_options: Callable[_P, _R], *, name: Optional[str] = ...
) -> Callable[_P, _R]: ...
@overload
def instrument(
    function_or_options: object = ..., *, name: Optional[str] = ...
) -> Callable[[Callable[_P, _R]], Callable[_P, _R]]: ...
