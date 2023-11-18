import abc
import socket
import typing

from pydantic import BaseModel

class Message(BaseModel):
  name: str
  data: BaseModel

class BaseIPCChannel:
  
  @abc.abstractmethod
  def send(self, name: str, data: BaseModel) -> None:
    raise NotImplementedError()

@typing.final
class NoopIPCChannel(BaseIPCChannel):
  def send(self, name: str, data: BaseModel) -> None:
    pass

@typing.final
class IPCChannel(BaseIPCChannel):
  def __init__(self, host: str, port: int) -> None:
    self._socket = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    self._socket.connect((host, port))
  
  def send(self, name: str, data: BaseModel) -> None:
    self._socket.send(Message(name=name, data=data).model_dump_json().encode("utf-8"))
