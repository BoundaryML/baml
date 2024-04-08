from typing import Literal, List, Tuple, Union
from baml_core_ffi import (
    render_prompt as render_prompt_ffi,
    RenderData,
    RenderData_Client,
    RenderData_Context,
    RenderedChatMessage as ChatMessage,
    TemplateStringMacro,
)


def render_prompt(
    prompt_template: str, render_context: RenderData
) -> Union[
    Tuple[Literal["completion"], str], Tuple[Literal["chat"], List[ChatMessage]]
]:
    return render_prompt_ffi(prompt_template, render_context)


__all__ = [
    "render_prompt",
    "RenderData",
    "RenderData_Client",
    "RenderData_Context",
    "TemplateStringMacro",
]


if __name__ == "__main__":
    print("Demonstrating render_prompt")
    args = RenderData(
        args={
            "name": "world",
            "foo": {
                "bar": "baz",
                "buzz": [
                    1,
                    2,
                    3,
                    {
                        "a": "b",
                        # "y": PromptClient(),
                        "c": "d",
                        "x": None,
                    },
                    5,
                    6,
                    7,
                ],
            },
        },
        ctx=RenderData.ctx(
            client=RenderData.client(name="gpt4", provider="openai"),
            output_schema="",
            env={"LANG": "en_US.UTF-8"},
        ),
        template_string_macros=[
            RenderData.template_string_macro(
                name="foo",
                args=[
                    ("bar", "string"),
                ],
                template="foo {{bar}}",
            )
        ],
    )
    rendered = render_prompt(
        "{{ctx.env.LANG}}: Hello {{name}}, it's a good day today!", args
    )
    print("Rendered", rendered)
