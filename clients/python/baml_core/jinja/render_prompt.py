from typing import List, Tuple
from baml_core_ffi import RenderData
import baml_core_ffi


def render_prompt(prompt_template: str, args: RenderData) -> str:
    return baml_core_ffi.render_prompt(prompt_template, args)


__all__ = ["render_prompt", "RenderData"]


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
