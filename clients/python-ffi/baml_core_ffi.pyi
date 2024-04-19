from typing import Any, Dict, Literal, List, Tuple, Union

class RenderData_Client: ...

class RenderData_Context: ...

class TemplateStringMacro: ...

class RenderData:
    def __new__(cls,
                args: Dict[str, Any],
                ctx: RenderData_Context,
                template_string_macros: List[TemplateStringMacro]
                ) -> "RenderData": ...

    @staticmethod
    def ctx(client: RenderData_Client,
            output_format: str,
            env: Dict[str, str]
            ) -> RenderData_Context: ...

    @staticmethod
    def client(name: str, provider: str) -> RenderData_Client: ...

    @staticmethod
    def template_string_macro(name: str, args: List[Tuple[str, str]], template: str) -> TemplateStringMacro: ...

class RenderedChatMessage:
    @property
    def role(self) -> str: ...

    @property
    def message(self) -> str: ...

def render_prompt(prompt_template: str, render_context: RenderData) -> Union[
        Tuple[Literal["completion"], str],
        Tuple[Literal["chat"], List[RenderedChatMessage]]
    ]: ...
