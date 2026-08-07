class GoogleClient {
    model: string,
    api_key: string,
    base_url: string?,

    function new(
        model: string = "gemini-2.5-flash",
        api_key: string? = null,
        base_url: string? = null,
    ) -> GoogleClient {
        GoogleClient {
            model: model,
            api_key: api_key ?? baml.env.get_or_panic("GOOGLE_API_KEY"),
            base_url: base_url,
        }
    }

    implements root.ai.Client {
        function id(self) -> string {
            `google/${self.model}`
        }

        function invoke(self, input: root.ai.ModelTurnInput) -> root.ai.ModelTurn {
            root.google.internal.invoke(self, input)
        }
    }
}
