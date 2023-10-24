import abc
import typing


class RawWrapper(metaclass=abc.ABCMeta):
    @abc.abstractmethod
    def as_str(self) -> typing.Optional[str]:
        raise NotImplemented

    @abc.abstractmethod
    def as_int(self) -> typing.Optional[int]:
        raise NotImplemented

    @abc.abstractmethod
    def as_float(self) -> typing.Optional[float]:
        raise NotImplemented

    @abc.abstractmethod
    def as_bool(self) -> typing.Optional[bool]:
        raise NotImplemented

    @abc.abstractmethod
    def as_list(self) -> typing.Iterable["RawWrapper"]:
        raise NotImplemented

    @abc.abstractmethod
    def as_dict(
        self,
    ) -> typing.ItemsView[typing.Optional["RawWrapper"], "RawWrapper"]:
        raise NotImplemented
