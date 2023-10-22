import inspect
import types
import typing

from typeguard import typechecked

import pytest
from _pytest.fixtures import FixtureRequest


T = typing.TypeVar("T")


def __parse_arg(arg: typing.Any, t: typing.Type[T], _default: T) -> T:
    if arg is None:
        return _default
    if isinstance(arg, t):
        return arg
    try:
        return t(arg)  # type: ignore
    except (ValueError, TypeError):
        return _default


RET = typing.TypeVar("RET", covariant=True)


class CB(typing.Generic[RET], typing.Protocol):
    def __call__(
        self, *args: typing.Any, **kwargs: typing.Any
    ) -> typing.Awaitable[RET]:
        ...


class BAMLImpl(typing.Generic[RET]):
    __cb: CB[RET]

    def __init__(self, cb: CB[RET]) -> None:
        self.__cb = cb

    async def run(self, **kwargs: typing.Any) -> RET:
        return await self.__cb(**kwargs)


class BaseBAMLFunction(typing.Generic[RET]):
    __impls: typing.Dict[str, BAMLImpl[RET]]

    def __init__(self, name: str, interface: typing.Any) -> None:
        self.__impls = {}
        self.__name = name
        self.__interface = interface

    def register_impl(self, name: str) -> typing.Callable[[CB[RET]], None]:
        assert (
            name not in self.__impls
        ), f"Already called: register_impl for {self.__name}:{name}"

        def decorator(cb: CB[RET]) -> None:
            # Runtime check
            sig = inspect.signature(cb)
            expected_sig = inspect.signature(self.__interface.__call__)
            assert (
                sig == expected_sig
            ), f"{self._name} {sig} does not match expected signature {expected_sig}"
            self.__impls[name] = BAMLImpl(cb)

        return decorator

    def get_impl(self, name: str) -> BAMLImpl[RET]:
        assert (
            name in self.__impls
        ), f"Never called register_impl for {self.__name}:{name}"
        return self.__impls[name]

    @property
    def _name(self) -> str:
        return self.__name

    @property
    def _impls(self) -> typing.Dict[str, BAMLImpl[RET]]:
        return self.__impls

    def __parametrize_test_methods(
        self,
        test_class: T,
        excluded_impls: typing.Optional[typing.Iterable[str]] = None,
    ) -> T:
        """
        Applies pytest.mark.parametrize to each test method in the test class.
        """
        selected_impls = filter(
            lambda k: k not in (excluded_impls or []), self.__impls.keys()
        )
        decorator = self.__test_wrapper(selected_impls)

        for attr_name, attr_value in vars(test_class).items():
            if isinstance(attr_value, types.FunctionType) and attr_name.startswith(
                "test_"
            ):
                setattr(
                    test_class,
                    attr_name,
                    decorator(attr_value),
                )
        return test_class

    def __test_wrapper(self, impls: typing.Iterable[str]) -> pytest.MarkDecorator:
        return pytest.mark.parametrize(f"{self._name}TestHandler", impls, indirect=True)

    def test(self, *args: typing.Any, **kwargs: typing.Any) -> typing.Any:
        if len(args) == 1:
            if len(kwargs) > 0:
                raise ValueError("To specify parameters, use keyword arguments.")

            if callable(args[0]) and inspect.isclass(args[0]):
                return self.__parametrize_test_methods(args[0])
            elif callable(args[0]):
                return self.__test_wrapper(self.__impls.keys())(args[0])
        if len(args) != 0:
            raise ValueError(
                "Only keyword arguments are supported. Otherwise use without ()."
            )

        excluded_impls = __parse_arg(
            kwargs.get("exclude_impl"),
            typing.cast(typing.Type[typing.Iterable[str]], set),
            set(),
        )
        selected_impls = filter(lambda k: k not in excluded_impls, self.__impls.keys())
        return self.__test_wrapper(selected_impls)
