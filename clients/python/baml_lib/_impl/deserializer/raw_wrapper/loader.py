import json5 as json
import regex as re
import typing
from .raw_wrapper import RawWrapper
from .primitive_wrapper import RawBaseWrapper, RawStringWrapper, RawNoneWrapper
from .list_wrapper import ListRawWrapper
from .dict_wrapper import DictRawWrapper
from ..diagnostics import Diagnostics


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
        str_val = val.strip()

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
            except ValueError:
                try:
                    parsed_list = typing.cast(
                        typing.List[typing.Any],
                        json.loads(make_string_robust_for_json(str_val)),
                    )
                except ValueError:
                    parsed_list = None
            if parsed_list is not None:
                return ListRawWrapper(
                    [
                        __from_value(item, diagnostics=diagnostics)
                        for item in parsed_list
                    ]
                )
        is_dict = str_val.startswith("{") and str_val.endswith("}")
        if is_dict:
            try:
                parsed_obj = typing.cast(
                    typing.Mapping[typing.Any, typing.Any], json.loads(str_val)
                )
            except ValueError:
                try:
                    parsed_obj = typing.cast(
                        typing.Mapping[typing.Any, typing.Any],
                        json.loads(make_string_robust_for_json(str_val)),
                    )
                except ValueError:
                    parsed_obj = None
            if parsed_obj is not None:
                return DictRawWrapper(
                    {
                        __from_value(k, diagnostics=diagnostics): __from_value(
                            v, diagnostics=diagnostics
                        )
                        for k, v in parsed_obj.items()
                    }
                )
        as_inner: typing.Optional[RawWrapper] = None
        if result := re.findall(r"```json\s*([^`]*?)\s*```", str_val, re.DOTALL):
            # if multiple matches, we'll just take the first one
            if len(result) > 1:
                pass
            as_inner = __from_value(result[0], diagnostics=diagnostics)
        as_obj = None
        as_list: typing.Optional[RawWrapper] = None
        if not is_dict:
            if result := re.findall(r"\{(?:[^{}]+|(?R))+\}", str_val):
                # if multiple matches, we'll just take the first one
                if len(result) > 1:
                    as_list = ListRawWrapper(
                        [__from_value(item, diagnostics=diagnostics) for item in result]
                    )
                else:
                    as_obj = __from_value(result[0], diagnostics=diagnostics)
        if not is_list and as_list is None:
            if result := re.findall(r"\[(?:[^\[\]]*|(?R))+\]", str_val):
                # if multiple matches, we'll just take the first one
                as_list = __from_value(result[0], diagnostics=diagnostics)

        return RawStringWrapper(val, as_obj=as_obj, as_list=as_list, as_inner=as_inner)
    if isinstance(val, (list, tuple)):
        return ListRawWrapper(
            [__from_value(item, diagnostics=diagnostics) for item in val]
        )
    if isinstance(val, dict):
        return DictRawWrapper(
            {
                __from_value(key, diagnostics=diagnostics): __from_value(
                    value, diagnostics=diagnostics
                )
                for key, value in val.items()
            }
        )

    diagnostics.push_unknown_error(
        "Unrecognized type: {} in value {}".format(type(val), val)
    )
    diagnostics.to_exception()

    raise Exception("[unreachable] Unsupported type: {}".format(type(val)))


def make_string_robust_for_json(s: str) -> str:
    in_string = False
    escape_count = 0
    result: typing.List[str] = []

    for char in s:
        # Check for the quote character
        if char == '"':
            # If preceded by an odd number of backslashes, it's an escaped quote and doesn't toggle the string state
            if escape_count % 2 == 0:
                in_string = not in_string
                escape_count = (
                    0  # Reset escape sequence counter after a non-escaped quote
                )
            # If it's an escaped quote, just reset the counter but don't add to it
        elif char == "\\":
            # Increment escape sequence counter if we're in a string
            if in_string:
                escape_count += 1
        else:
            # Any other character resets the escape sequence counter
            escape_count = 0

        # When inside a string, escape the newline characters
        if in_string and char == "\n":
            result.append("\\n")
        else:
            result.append(char)

    return "".join(result)
