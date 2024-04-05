import pytest

from baml_core import (
    render_prompt,
    PromptClient,
    PromptContext,
)


def test_success() -> None:
    rendered = render_prompt(
        "Hello {{name}} -{{ ctx.client.provider }}",
        {
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
        PromptContext(client=PromptClient(name="gpt4", provider="openai")),
    )
    assert rendered == "Hello world -openai"


def test_bad_params() -> None:
    with pytest.raises(RuntimeError) as e:
        render_prompt(
            "Hello {{name}",
            {
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
            PromptContext(),
        )
    assert (
        "Error occurred while rendering prompt: syntax error: unexpected `}}`, expected end of variable block (in prompt:1)"
        in str(e.value)
    )


def test_bad_template() -> None:
    with pytest.raises(TypeError) as e:
        render_prompt(
            "Hello {{name}}",
            {
                "name": "world",
                "foo": {
                    "bar": "baz",
                    "buzz": [
                        1,
                        2,
                        3,
                        {
                            "a": "b",
                            "y": PromptClient(),
                            "c": "d",
                            "x": None,
                        },
                        5,
                        PromptClient(),
                        7,
                    ],
                },
            },
            PromptContext(),
        )
    assert "params.foo.buzz.3.y: unsupported type" in str(e.value)
    assert "params.foo.buzz.5: unsupported type" in str(e.value)
