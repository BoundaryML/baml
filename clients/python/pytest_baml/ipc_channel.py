import abc
import socket
import time
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


def connect_to_server(host: str, port: int, retries=5, delay=1):
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


@typing.final
class IPCChannel(BaseIPCChannel):
    def __init__(self, host: str, port: int) -> None:
        self._host = host
        self._port = port
        self._socket = connect_to_server(host, port)

    def send(self, name: str, data: T) -> None:
        message = (
            Message(name=name, data=data).model_dump_json(by_alias=True) + "<END_MSG>\n"
        ).encode("utf-8")
        connect_to_server(self._host, self._port).sendall(message)
