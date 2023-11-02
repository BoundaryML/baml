import json5 as json  # type: ignore
import re
import typing
from .raw_wrapper import RawWrapper
from .primitive_wrapper import RawBaseWrapper, RawStringWrapper, RawNoneWrapper
from .list_wrapper import ListRawWrapper
from .dict_wrapper import DictRawWrapper
from ..diagnostics import Diagnostics, DeserializerError

def from_string(val: str, diagnostics: Diagnostics) -> RawWrapper:
    return __from_value(val, diagnostics)


def __from_value(val: typing.Any, diagnostics: Diagnostics) -> RawWrapper:
    if val is None:
        return RawNoneWrapper()
    if isinstance(val, bool):
        return RawBaseWrapper(val)
    if isinstance(val, int):
        return RawBaseWrapper(val)
    if isinstance(val, float):
        return RawBaseWrapper(val)
    if isinstance(val, str):
        # First remove any whitespace
        str_val = val.strip()
        # Remove any starting and ending quotes
        if str_val.startswith('"') and str_val.endswith('"'):
            str_val = str_val[1:-1]
        # Remove any starting and ending quotes
        if str_val.startswith("'") and str_val.endswith("'"):
            str_val = str_val[1:-1]

        if str_val.lower() == "true":
            return RawBaseWrapper(True)
        if str_val.lower() == "false":
            return RawBaseWrapper(False)

        is_number = re.match(r"^(\+|-)?\d+(\.\d+)?$", str_val)
        if is_number:
            if "." in str_val:
                return RawBaseWrapper(float(str_val))
            return RawBaseWrapper(int(str_val))

        is_list = str_val.startswith("[") and str_val.endswith("]")
        if is_list:
            try:
                parsed_list = typing.cast(typing.List[typing.Any], json.loads(str_val))
            except:
                parsed_list = None
            if parsed_list:
                return ListRawWrapper([__from_value(item, diagnostics=diagnostics) for item in parsed_list])
        is_dict = str_val.startswith("{") and str_val.endswith("}")
        if is_dict:
            try:
                parsed_obj = typing.cast(
                    typing.Mapping[typing.Any, typing.Any], json.loads(str_val)
                )
            except:
                parsed_obj = None
            if parsed_obj:
                return DictRawWrapper(
                    {__from_value(k, diagnostics=diagnostics): __from_value(v, diagnostics=diagnostics) for k, v in parsed_obj.items()}
                )

        return RawStringWrapper(str_val)
    if isinstance(val, (list, tuple)):
        return ListRawWrapper([__from_value(item, diagnostics=diagnostics) for item in val])
    if isinstance(val, dict):
        return DictRawWrapper(
            {__from_value(key, diagnostics=diagnostics): __from_value(value, diagnostics=diagnostics) for key, value in val.items()}
        )
    
    diagnostics.push_error(DeserializerError("Unrecognized type: {} in value {}".format(type(val), val)))
    diagnostics.to_exception()

    raise Exception("[unreachable] Unsupported type: {}".format(type(val))) 
