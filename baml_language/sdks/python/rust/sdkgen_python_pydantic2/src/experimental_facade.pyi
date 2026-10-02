from typing import Any

from .._invocation_types import BamlOptions, _CallableHandle


def invoke(target: str | _CallableHandle, arguments: dict[str, Any], *, _types: dict[str, Any] | None = None, _baml: BamlOptions | None = None) -> Any: ...
async def invoke_async(target: str | _CallableHandle, arguments: dict[str, Any], *, _types: dict[str, Any] | None = None, _baml: BamlOptions | None = None) -> Any: ...
