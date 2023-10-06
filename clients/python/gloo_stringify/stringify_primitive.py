import json
import re
import typing
from .stringify import StringifyBase, as_singular
from .errors import StringifyError

T = typing.TypeVar("T", str, int, float, bool, None)


class StringifyPrimitive(StringifyBase[T]):
    pass


class StringifyString(StringifyPrimitive[str]):
    def _json_str(self) -> str:
        return "string"

    def _parse(self, value: typing.Any) -> str:
        value = as_singular(value)
        if isinstance(value, str):
            stripped = value.strip()
            if stripped.startswith('"""') and stripped.endswith('"""'):
                return stripped[3:-3]
            if stripped.startswith("'''") and stripped.endswith("'''"):
                return stripped[3:-3]
            if stripped.startswith('"') and stripped.endswith('"'):
                return stripped[1:-1]
            if stripped.startswith("'") and stripped.endswith("'"):
                return stripped[1:-1]
            return stripped
        return str(value)

    def vars(self) -> typing.Dict[str, str]:
        return {}


class StringifyChar(StringifyPrimitive[str]):
    def _json_str(self) -> str:
        return "char"

    def _parse(self, value: typing.Any) -> str:
        value = as_singular(value)

        if isinstance(value, str):
            stripped = value.strip()
            if stripped.startswith('"""') and stripped.endswith('"""'):
                stripped = stripped[3:-3]
            elif stripped.startswith("'''") and stripped.endswith("'''"):
                stripped = stripped[3:-3]
            elif stripped.startswith('"') and stripped.endswith('"'):
                stripped = stripped[1:-1]
            elif stripped.startswith("'") and stripped.endswith("'"):
                stripped = stripped[1:-1]
            elif len(stripped) == 1:
                stripped = stripped

            cleaned = stripped.strip()
            # Log warning if string is longer than 1 char
            if len(cleaned) == 0:
                raise StringifyError(f"Expected char, got {stripped}")
            return cleaned[0]
        val = str(value)
        if len(val) == 0:
            raise StringifyError(f"Expected char, got {value}")
        return val[0]

    def vars(self) -> typing.Dict[str, str]:
        return {}


class StringifyFloat(StringifyPrimitive[float]):
    def _json_str(self) -> str:
        return "float"

    def _parse(self, value: typing.Any) -> float:
        value = as_singular(value)
        if isinstance(value, str):
            cleaned = value.strip().lower()
            # Validate string only has digits and or a single decimal point.
            # A starting negative sign is allowed, and starting digit is not required.
            # Commas are allowed, but only between digits before the decimal point.
            if re.match(r"^-?(\d+,?)*\.?\d+$", cleaned):
                # Remove commas
                cleaned = cleaned.replace(",", "")
                return float(cleaned)
            else:
                try:
                    return float(json.loads(value.lower()))
                except TypeError:
                    raise StringifyError(f"Expected float, got string: {value}")
                except json.JSONDecodeError:
                    raise StringifyError(f"Expected float, got string: {value}")
        try:
            return float(value)
        except TypeError:
            raise StringifyError(f"Expected float, got {value}")
        except ValueError:
            raise StringifyError(f"Expected float, got {value}")

    def vars(self) -> typing.Dict[str, str]:
        return {}


class StringifyInt(StringifyPrimitive[int]):
    def _json_str(self) -> str:
        return "int"

    def _parse(self, value: typing.Any) -> int:
        value = as_singular(value)
        if isinstance(value, str):
            cleaned = value.strip().lower()
            # Validate string only has digits.
            # A starting negative sign is allowed, and starting digit is not required.
            # Commas are allowed, but only between digits.
            if re.match(r"^-?(\d+,?)*\d+$", cleaned):
                # Remove commas
                cleaned = cleaned.replace(",", "")
                return int(cleaned)
            else:
                try:
                    return int(json.loads(cleaned))
                except json.JSONDecodeError:
                    raise StringifyError(f"Expected int, got string: {value}")
                except TypeError:
                    raise StringifyError(f"Expected int, got string: {value}")
        try:
            return int(value)
        except TypeError:
            raise StringifyError(f"Expected int, got {value}")
        except ValueError:
            raise StringifyError(f"Expected int, got {value}")

    def vars(self) -> typing.Dict[str, str]:
        return {}


class StringifyBool(StringifyPrimitive[bool]):
    def _json_str(self) -> str:
        return "bool"

    def _parse(self, value: typing.Any) -> bool:
        value = as_singular(value)
        print(value, type(value))
        if isinstance(value, str):
            cleaned = value.strip().lower()
            if cleaned == "true":
                return True
            elif cleaned == "false":
                return False
            else:
                try:
                    return bool(as_singular(cleaned))
                except json.JSONDecodeError:
                    raise StringifyError(f"Expected bool, got string: {value}")
                except TypeError:
                    raise StringifyError(f"Expected bool, got string: {value}")
        try:
            return bool(value)
        except ValueError:
            raise StringifyError(f"Expected bool, got {value}")

    def vars(self) -> typing.Dict[str, str]:
        return {}


class StringifyNone(StringifyPrimitive[None]):
    def _json_str(self) -> str:
        return "null"

    def _parse(self, value: typing.Any) -> None:
        return None

    def vars(self) -> typing.Dict[str, str]:
        return {}
