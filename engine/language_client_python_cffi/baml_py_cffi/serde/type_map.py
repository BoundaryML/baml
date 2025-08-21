from typing import Dict, Tuple, Type, Callable, Any, TypeVar, TYPE_CHECKING

if TYPE_CHECKING:
    from . import cffi_pb2

T = TypeVar('T')

# Type aliases matching Go's approach
EncodeFunc = Callable[[Any], 'cffi_pb2.CFFIValueHolder']
DecodeFunc = Callable[['cffi_pb2.CFFIValueHolder'], Any]
TypeEntry = Tuple[Type[T], EncodeFunc, DecodeFunc]

# TypeMap equivalent to Go's map[string]reflect.Type
TypeMap = Dict[str, TypeEntry]