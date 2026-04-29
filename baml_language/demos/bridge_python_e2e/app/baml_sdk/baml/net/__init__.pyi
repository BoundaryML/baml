from __future__ import annotations

import typing
import pydantic


class Socket(pydantic.BaseModel):
    def read(self) -> str: ...
    async def read_async(self) -> str: ...

    def close(self) -> None: ...
    async def close_async(self) -> None: ...


__all__ = [
    "Socket",
]
