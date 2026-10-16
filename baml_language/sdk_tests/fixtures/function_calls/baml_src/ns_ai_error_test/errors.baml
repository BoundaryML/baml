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

/// The record of one call billed to `test-provider`, with no HTTP exchange.
function test_provider_call() -> ai.events.ModelCall {
    ai.events.ModelCall {
        resolved_model: ai.events.ModelKey { provider: "test-provider", model: "test-model" },
        usage: null,
        timing: ai.events.Timing { start_time_utc_ms: null, duration_ms: null, ttft_ms: null },
        http_request: null,
        http_response: null,
        sse_responses: null,
    }
}

class SchemaFailureClient {
    implements ai.Client {
        function requested_model(self) -> string {
            "test-model"
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
            ai.internal.with_own_call(
                ai.ModelTurn {
                    content: [ai.content.Text { text: "not a number" }],
                    stop_reason: ai.content.StopReason.Complete,
                    calls: [],
                    child_turns: [],
                },
                test_provider_call(),
                null,
                "test-model",
                null,
            )
        }
    }

    implements ai.stream.StreamingClient {
        function invoke_stream(self, input: ai.ModelTurnInput) -> ai.stream.TurnStream {
            let _ = input;
            let events: ai.stream.Event[] = [ai.stream.TextDelta { text: "not a number" }, ai.stream.TurnDone {}];
            let cursor = 0;
            ai.stream.TurnStream.from_event_source(
                () -> {
                    let next = events.at(cursor);
                    cursor += 1;
                    match (next) {
                        let e: ai.stream.Event => e,
                        null => ai.stream.Done {},
                    }
                },
                call = test_provider_call(),
            )
        }
    }
}

client InvalidSchema = SchemaFailureClient {};

function ExtractInt() -> int {
    client: InvalidSchema
    prompt: `Return an integer.`
}
