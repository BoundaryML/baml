from typing import Dict, Any
import baml_pyo3


class PromptClient:
    name: str
    provider: str

    def __init__(self, name: str = "", provider: str = ""):
        self.name = name
        self.provider = provider

    def to_dict(self) -> Dict[str, Any]:
        return {
            "name": self.name,
            "provider": self.provider,
        }


class PromptContext:
    client: PromptClient
    output_schema: str
    # we can also make env accessible top-level in the future
    env: Dict[str, str]

    def __init__(
        self,
        client: PromptClient = PromptClient(),
        output_schema: str = "",
        env: Dict[str, str] = {},
    ):
        self.client = client
        self.output_schema = output_schema
        self.env = env

    def to_dict(self) -> Dict[str, Any]:
        return {
            "client": self.client.to_dict(),
            "output_schema": self.output_schema,
            "env": self.env,
        }


def render_prompt(
    prompt_template: str, params: Dict[str, Any], ctx: PromptContext
) -> str:
    return baml_pyo3.render_prompt(prompt_template, {**params, "ctx": ctx.to_dict()})


if __name__ == "__main__":
    print("starting wasm test")
    rendered = render_prompt(
        "Hello {{name} more wtf",
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
        PromptContext(),
    )
    print("Rendered", rendered)
