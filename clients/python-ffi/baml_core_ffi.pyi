from typing import Any, Dict

class RenderData_Client: ...

class RenderData_Context: ...

class RenderData:
    def __new__(cls,
                args: Dict[str, Any],
                ctx: RenderData_Context,
                template_string_vars: Dict[str, str]
                ) -> "RenderData": ...

    @staticmethod
    def ctx(client: RenderData_Client,
            output_schema: str,
            env: Dict[str, str]
            ) -> RenderData_Context: ...

    @staticmethod
    def client(name: str, provider: str) -> RenderData_Client: ...

def render_prompt(prompt_template: str, render_context: RenderData) -> str: ...
