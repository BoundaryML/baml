from typing import Any, Dict

class FunctionResult: ...

class BamlRuntimeFfi:
    @staticmethod
    def from_directory(directory: str) -> BamlRuntimeFfi: ...

    def call_function(
        self,
        function_name: str,
        args: Dict[str, Any],
        ctx: Dict[str, Any],
    ) -> FunctionResult: ...

    async def call_async(
        self,
        function_name: str,
        args: Dict[str, Any],
        ctx: Dict[str, Any],
    ) -> FunctionResult: ...


def rust_sleep() -> None: ...