from typing import Any, Dict

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

class BamlRuntimeFfi:

    @staticmethod
    def from_directory(directory: str) -> BamlRuntimeFfi: ...

    async def call_function(
        self,
        function_name: str,
        args: Dict[str, Any],
        ctx: Dict[str, Any],
    ) -> FunctionResult: ...
