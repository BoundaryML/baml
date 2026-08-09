class OpenAiClient {
    model: string,
    api_key: string,
    base_url: string?,

    function new(
        model: string = "gpt-4o-mini",
        api_key: string? = null,
        base_url: string? = null,
    ) -> OpenAiClient {
        OpenAiClient {
            model: model,
            api_key: api_key ?? baml.env.get_or_panic("OPENAI_API_KEY"),
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
}
