from baml_pyo3 import RenderData
import baml_pyo3


def render_prompt(prompt_template: str, args: RenderData) -> str:
    return baml_pyo3.render_prompt(prompt_template, args)


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
        template_string_vars={},
    )
    rendered = render_prompt(
        "{{ctx.env.LANG}}: Hello {{name}}, it's a good day today!", args
    )
    print("Rendered", rendered)
