import pytest

from baml_core import render_prompt, RenderData


def test_success() -> None:
    ctx = RenderData.ctx(
        client=RenderData.client(name="gpt4", provider="openai"),
        output_schema="",
        env={"LANG": "en_US.UTF-8"},
    )
    rendered = render_prompt(
        "{{ ctx.env.LANG }}: Hello {{name}} -{{ ctx.client.provider }}",
        RenderData(
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
                            "c": "d",
                            "x": None,
                        },
                        5,
                        6,
                        7,
                    ],
                },
            },
            ctx=ctx,
            template_string_macros=[
                RenderData.template_string_macro(
                    name="latin",
                    args=[],
                    template="lorem ipsum dolor",
                )
            ],
        ),
    )
    assert rendered == ("completion", "en_US.UTF-8: Hello world -openai")


def test_bad_params() -> None:
    with pytest.raises(RuntimeError) as e:
        render_prompt(
            "Hello {{name}",
            RenderData(
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
                    env={},
                ),
                template_string_macros=[],
            ),
        )
    assert (
        "Error occurred while rendering prompt: syntax error: unexpected `}}`, expected end of variable block (in prompt:1)"
        in str(e.value)
    )


def test_bad_template() -> None:
    with pytest.raises(TypeError) as e:
        render_prompt(
            "Hello {{name}}",
            RenderData(
                args={
                    "name": "world",
                    "foo": {
                        "bar": "baz",
                        "buzz": [
                            0,
                            1,
                            2,
                            {
                                "a": "b",
                                "y": object(),
                                "c": "d",
                                "x": None,
                            },
                            4,
                            object(),
                            6,
                        ],
                    },
                },
                ctx=RenderData.ctx(
                    client=RenderData.client(name="gpt4", provider="openai"),
                    output_schema="",
                    env={},
                ),
                template_string_macros=[],
            ),
        )
    assert "args.foo.buzz.3.y: unsupported type" in str(e.value)
    assert "args.foo.buzz.5: unsupported type" in str(e.value)
