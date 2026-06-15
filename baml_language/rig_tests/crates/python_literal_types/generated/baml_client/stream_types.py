import typing
import typing_extensions
from pydantic import BaseModel, ConfigDict, Field

import baml_py

from . import types

StreamStateValueT = typing.TypeVar('StreamStateValueT')
class StreamState(BaseModel, typing.Generic[StreamStateValueT]):
    value: StreamStateValueT
    state: typing_extensions.Literal["Pending", "Incomplete", "Complete"]