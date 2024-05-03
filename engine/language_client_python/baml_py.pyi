from typing import Any, Dict, Optional

class FunctionResult:
    """The result of a BAML function call.

    Represents any of:

        - a successful LLM call, with a successful type parse
        - a successful LLM call, with a failed type parse
        - a failed LLM call, due to a provider outage or other network error
        - a failed LLM call, due to an inability to build the request
        - or any other outcome, really

    We only expose the parsed result to Python right now.
    """

    def __str__(self) -> str: ...
    def parsed(self) -> Any: ...

class Image:
    def __init__(
        self, url: Optional[str] = None, base64: Optional[str] = None
    ) -> None: ...

    @property
    def url(self) -> Optional[str]: ...

    @url.setter
    def url(self, value: Optional[str]) -> None: ...

    @property
    def base64(self) -> Optional[str]: ...

    @base64.setter
    def base64(self, value: Optional[str]) -> None: ...


class BamlRuntimeFfi:
    @staticmethod
    def from_directory(directory: str) -> BamlRuntimeFfi: ...

    @staticmethod
    def from_encoded(encoded: str) -> BamlRuntimeFfi: ...

    async def call_function(
        self,
        function_name: str,
        args: Dict[str, Any],
        ctx: Dict[str, Any],
    ) -> FunctionResult: ...
