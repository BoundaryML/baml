class AnthropicClient {
    model: string,
    // null resolves from the environment at request time; construction
    // never touches env, so declaring/constructing a client is always safe.
    api_key: string?,
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
            api_key: api_key,
            base_url: base_url,
            max_tokens: max_tokens,
        }
    }

    implements ai.Client {
        function id(self) -> string {
            `anthropic/${self.model}`
        }

        function invoke(self, input: ai.ModelTurnInput) -> ai.ModelTurn {
            root.internal.invoke(self, input) catch_all (e) {
                _ => throw ai.errors.normalize(e),
            }
        }
    }

    implements ai.stream.StreamingClient {
        function invoke_stream(self, input: ai.ModelTurnInput) -> ai.stream.TurnStream {
            root.internal.invoke_stream(self, input) catch_all (e) {
                _ => throw ai.errors.normalize(e),
            }
        }
    }
}
