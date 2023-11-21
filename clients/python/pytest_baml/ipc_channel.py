import abc
import socket
import typing

from pydantic import BaseModel

T = typing.TypeVar("T", bound=BaseModel)


class Message(BaseModel, typing.Generic[T]):
    name: str
    data: T


class BaseIPCChannel:
    @abc.abstractmethod
    def send(self, name: str, data: T) -> None:
        raise NotImplementedError()


@typing.final
class NoopIPCChannel(BaseIPCChannel):
    def send(self, name: str, data: T) -> None:
        pass


@typing.final
class IPCChannel(BaseIPCChannel):
    def __init__(self, host: str, port: int) -> None:
        self._socket = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        self._socket.connect((host, port))

    def send(self, name: str, data: T) -> None:
        self._socket.sendall(
            Message(name=name, data=data).model_dump_json(by_alias=True).encode("utf-8")
        )
        self._socket.sendall(b"<END_MSG>\n")
