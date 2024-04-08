from typing import Any, Dict, List, Tuple

class RenderData_Client: ...

class RenderData_Context: ...

class TemplateStringMacro: ...

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

    @staticmethod
    def template_string_macro(name: str, args: List[Tuple[str, str]], template: str) -> TemplateStringMacro: ...

def render_prompt(prompt_template: str, render_context: RenderData) -> str: ...
