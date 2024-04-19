from baml_core_ffi import (
    render_prompt,
    RenderData,
    RenderData_Client,
    RenderData_Context,
    RenderedChatMessage,
    TemplateStringMacro,
)


__all__ = [
    "render_prompt",
    "RenderData",
    "RenderData_Client",
    "RenderData_Context",
    "RenderedChatMessage",
    "TemplateStringMacro",
]


if __name__ == "__main__":
    print("Demonstrating render_prompt")

    # With pydantic objects
    from pydantic import BaseModel
    from enum import Enum
    import typing

    class MyEnum(str, Enum):
        FOO = "foo"
        BAR = "bar"

    class Foo(BaseModel):
        bar: str
        em2: MyEnum = MyEnum.FOO

    foo = Foo(bar="baz")

    class Bar(BaseModel):
        foo: typing.List[Foo]
        en: MyEnum = MyEnum.BAR

    bar = Bar(foo=[foo])

    args = RenderData(
        args={
            "name": "world",
            "bar": [bar],
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
            output_format="",
            env={"LANG": "en_US.UTF-8"},
        ),
        template_string_macros=[
            RenderData.template_string_macro(
                name="farewell",
                args=[
                    ("name", "string"),
                ],
                template="goodbye {{name}}",
            )
        ],
    )
    rendered = render_prompt(
        '{{ _.chat("system") }} {{ctx.env.LANG}}: Hello {{name}}, it\'s a good day today!\n\n{{farewell(name)}}\n\n{{ bar }}',
        args,
    )
    print("Rendered", rendered)
