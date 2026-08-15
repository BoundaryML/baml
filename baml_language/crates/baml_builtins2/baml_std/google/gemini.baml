class GoogleClient {
    model: string,
    // null resolves from the environment at request time; construction
    // never touches env, so declaring/constructing a client is always safe.
    api_key: string?,
    base_url: string?,

    function new(
        model: string = "gemini-2.5-flash",
        api_key: string? = null,
        base_url: string? = null,
    ) -> GoogleClient {
        GoogleClient { model: model, api_key: api_key, base_url: base_url }
    }

    implements ai.Client {
        function id(self) -> string {
            `google/${self.model}`
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
