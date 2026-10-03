function ThrowInvalidResponse() -> never throws ai.errors.InvalidResponse {
    throw ai.errors.InvalidResponse {
        provider: "test-provider",
        message: "invalid provider envelope",
        raw_body: "not JSON",
        status_code: 200,
    }
}

function ThrowProviderError() -> never throws ai.errors.ProviderError {
    throw ai.errors.ProviderError {
        provider: "test-provider",
        message: "provider overloaded",
        error_type: "overloaded_error",
        error_code: "server_error",
        raw_body: `{"error":{"type":"overloaded_error"}}`,
        status_code: null,
    }
}

class SchemaFailureClient {
    implements ai.Client {
        function id(self) -> string {
            "test-provider"
        }

        function render(self, input: ai.ModelTurnInput) -> baml.http.Request {
            let _ = input;
            baml.http.Request {
                method: "POST",
                url: "https://example.invalid",
                headers: {},
                body: "{}",
            }
        }

        function invoke(self, input: ai.ModelTurnInput) -> ai.ModelTurn {
            let _ = input;
            ai.ModelTurn {
                content: [ai.content.Text { text: "not a number" }],
                stop_reason: ai.content.StopReason.Complete,
                calls: [],
            }
        }
    }

    implements ai.stream.StreamingClient {
        function invoke_stream(self, input: ai.ModelTurnInput) -> ai.stream.TurnStream {
            let _ = input;
            ai.stream.TurnStream.from_chunks(["not a number"])
        }
    }
}

client InvalidSchema = SchemaFailureClient {};

function ExtractInt() -> int {
    client: InvalidSchema
    prompt: `Return an integer.`
}
