import abc
import typing


class RawWrapper(metaclass=abc.ABCMeta):
    @abc.abstractmethod
    def as_str(self, inner: bool) -> typing.Optional[str]:
        raise NotImplementedError

    @abc.abstractmethod
    def as_int(self) -> typing.Optional[int]:
        raise NotImplementedError

    @abc.abstractmethod
    def as_float(self) -> typing.Optional[float]:
        raise NotImplementedError

    @abc.abstractmethod
    def as_bool(self) -> typing.Optional[bool]:
        raise NotImplementedError

    @abc.abstractmethod
    def as_list(self) -> typing.Iterable["RawWrapper"]:
        raise NotImplementedError

    @abc.abstractmethod
    def as_dict(
        self,
    ) -> typing.ItemsView[typing.Optional["RawWrapper"], "RawWrapper"]:
        raise NotImplementedError
