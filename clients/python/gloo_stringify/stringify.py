import abc
import json
import typing
from .errors import StringifyError

T = typing.TypeVar("T")


class StringifyBase(abc.ABC, typing.Generic[T]):
    @property
    def json(self) -> str:
        return self._json_str()

    @abc.abstractmethod
    def _json_str(self) -> str:
        raise NotImplementedError()

    @abc.abstractmethod
    def _parse(self, value: typing.Any) -> T:
        raise NotImplementedError()

    def parse(self, value: typing.Any) -> T:
        try:
            return self._parse(value)
        except StringifyError as e:
            raise e
        except ValueError:
            raise StringifyError(f"Expected {self.json}, got {value}")

    @abc.abstractmethod
    def vars(self) -> typing.Dict[str, str]:
        raise NotImplementedError()


class StringifyRemappedField:
    def __init__(
        self,
        *,
        rename: typing.Optional[str] = None,
        describe: typing.Optional[str] = None,
        skip: bool = False,
    ) -> None:
        self.name = rename
        self.description = describe
        self.skip = skip


class StringifyCtx:
    _context_stack: typing.List[object] = []
    _instances_stack: typing.List[typing.Any] = []

    def __enter__(self) -> "StringifyCtx":
        self.current_context = object()
        StringifyCtx._context_stack.append(self.current_context)
        StringifyCtx._instances_stack.append({})
        return self

    def __exit__(
        self, exc_type: typing.Any, exc_value: typing.Any, traceback: typing.Any
    ) -> None:
        StringifyCtx._context_stack.pop()
        StringifyCtx._instances_stack.pop()

    @staticmethod
    def get_current_context() -> typing.Optional[object]:
        return StringifyCtx._context_stack[-1] if StringifyCtx._context_stack else None

    @staticmethod
    def set_instance_for_current_context(cls: typing.Any, instance: typing.Any) -> None:
        current_context = StringifyCtx.get_current_context()
        if current_context:
            StringifyCtx._instances_stack[-1][cls] = instance

    @staticmethod
    def get_instance_for_current_context(
        cls: typing.Any,
    ) -> typing.Optional[typing.Any]:
        current_context = StringifyCtx.get_current_context()
        if current_context and cls in StringifyCtx._instances_stack[-1]:
            return StringifyCtx._instances_stack[-1][cls]
        return None


def as_singular(value: typing.Any) -> typing.Any:
    try:
        if isinstance(value, str):
            stripped = value.strip()
            if (
                (stripped.startswith("[") and stripped.endswith("]"))
                or (stripped.startswith("{") and stripped.endswith("}"))
                or (stripped.startswith("(") and stripped.endswith(")"))
            ):
                parsed = json.loads(stripped)
                if isinstance(parsed, (list, tuple)):
                    if len(parsed) >= 1:
                        return parsed[0]
                if isinstance(parsed, (set, frozenset)):
                    if len(parsed) >= 1:
                        return next(iter(parsed))
                if isinstance(parsed, dict):
                    if len(parsed) >= 1:
                        return next(iter(parsed.values()))
    except json.JSONDecodeError:
        pass
    if isinstance(value, (list, tuple)):
        if len(value) >= 1:
            return value[0]
    if isinstance(value, (set, frozenset)):
        if len(value) >= 1:
            return next(iter(value))
    if isinstance(value, dict):
        if len(value) >= 1:
            return next(iter(value.values()))
    return value
