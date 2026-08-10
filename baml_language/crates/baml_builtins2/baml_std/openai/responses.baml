class OpenAiClient {
    model: string,
    // null resolves from the environment at request time; construction
    // never touches env, so declaring/constructing a client is always safe.
    api_key: string?,
    base_url: string?,

    function new(
        model: string = "gpt-4o-mini",
        api_key: string? = null,
        base_url: string? = null,
    ) -> OpenAiClient {
        OpenAiClient {
            model: model,
            api_key: api_key,
            base_url: base_url,
        }
    }

    implements ai.Client {
        function id(self) -> string {
            `openai/${self.model}`
        }

        function invoke(self, input: ai.ModelTurnInput) -> ai.ModelTurn {
            root.internal.invoke(self, input)
        }
    }

    implements ai.StreamingClient {
        function invoke_stream(self, input: ai.ModelTurnInput) -> ai.TurnStream {
            root.internal.invoke_stream(self, input)
        }
    }
}
