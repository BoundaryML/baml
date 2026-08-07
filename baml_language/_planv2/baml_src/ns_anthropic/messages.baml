class AnthropicClient {
    model: string,
    api_key: string,
    base_url: string?,
    max_tokens: int,

    // NOTE: the task spec named "claude-3-5-haiku-latest" as the default, but
    // that model was retired (the API 404s on it); claude-haiku-4-5 is the
    // current Haiku.
    function new(
        model: string = "claude-haiku-4-5",
        api_key: string? = null,
        base_url: string? = null,
        max_tokens: int = 4096,
    ) -> AnthropicClient {
        AnthropicClient {
            model: model,
            api_key: api_key ?? baml.env.get_or_panic("ANTHROPIC_API_KEY"),
            base_url: base_url,
            max_tokens: max_tokens,
        }
    }

    implements root.ai.Client {
        function id(self) -> string {
            `anthropic/${self.model}`
        }

        function invoke(self, input: root.ai.ModelTurnInput) -> root.ai.ModelTurn {
            root.anthropic.internal.invoke(self, input)
        }
    }
}
