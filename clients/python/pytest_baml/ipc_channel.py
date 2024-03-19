import abc
import socket
import time
import typing
import json

from pydantic import BaseModel
from baml_core.stream.baml_stream import _PartialDict

T = typing.TypeVar("T", BaseModel, _PartialDict)


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


def connect_to_server(
    host: str, port: int, retries: int = 5, delay: float = 1
) -> socket.socket:
    attempt = 0
    while attempt < retries:
        try:
            s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
            s.connect((host, port))
            return s  # Return the connected socket
        except socket.error as e:
            print(f"Connection attempt {attempt + 1} failed: {e}")
            time.sleep(delay)  # Wait before retrying
            attempt += 1
    raise ConnectionError(f"Could not connect to the server after {retries} attempts")


def custom_serializer(obj: typing.Any) -> typing.Any:
    if isinstance(obj, BaseModel):
        return obj.model_dump(by_alias=True)
    raise TypeError(f"Object of type {obj.__class__.__name__} is not JSON serializable")


@typing.final
class IPCChannel(BaseIPCChannel):
    def __init__(self, host: str, port: int) -> None:
        self._host = host
        self._port = port
        self._socket = connect_to_server(host, port)

    def send(self, name: str, data: T) -> None:
        message = (
            json.dumps({"name": name, "data": data}, default=custom_serializer).encode(
                "utf-8"
            )
            + b"<BAML_END_MSG>\n"
        )
        self._socket.sendall(message)
