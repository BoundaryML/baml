import pytest

from baml_core import render_prompt, RenderData


def test_success() -> None:
    rendered = render_prompt(
        """
            You are an assistant that always responds
            in a very excited way with emojis
            and also outputs this word 4 times
            after giving a response: {{ haiku_subject }}
            
            {{ _.chat(ctx.env.ROLE) }}
            
            Tell me a haiku about {{ haiku_subject }}. {{ ctx.output_format }}

            Before the haiku, include the following: "{{ latin() }}".
            
            After the haiku, tell me about your maker, {{ ctx.client.provider }}.
        """,
        RenderData(
            args={"haiku_subject": "sakura"},
            ctx=RenderData.ctx(
                client=RenderData.client(name="gpt4", provider="openai"),
                output_format="iambic pentameter",
                env={"ROLE": "john doe"},
            ),
            template_string_macros=[
                RenderData.template_string_macro(
                    name="latin",
                    args=[],
                    template='{{ "lorem ipsum dolor sit amet" | upper }}',
                )
            ],
        ),
    )
    assert rendered[0] == "chat"
    assert rendered[1][0].role == "system"
    assert rendered[1][0].message == (
        "You are an assistant that always responds\n"
        "in a very excited way with emojis\n"
        "and also outputs this word 4 times\n"
        "after giving a response: sakura"
    )

    assert rendered[1][1].role == "john doe"
    assert rendered[1][1].message == (
        "Tell me a haiku about sakura. Answer in JSON using this schema:"
        "\n\n"
        "iambic pentameter"
        "\n\n"
        'Before the haiku, include the following: "LOREM IPSUM DOLOR SIT AMET".'
        "\n\n"
        "After the haiku, tell me about your maker, openai."
    )


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
                            0,
                            1,
                            2,
                            {
                                "a": "b",
                                "c": "d",
                                "x": None,
                            },
                            4,
                            5,
                            6,
                        ],
                    },
                },
                ctx=RenderData.ctx(
                    client=RenderData.client(name="gpt4", provider="openai"),
                    output_format="",
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
                    output_format="",
                    env={},
                ),
                template_string_macros=[],
            ),
        )
    assert e.match("foo.buzz.3.y: Unsupported type")
    assert e.match("foo.buzz.5: Unsupported type")
