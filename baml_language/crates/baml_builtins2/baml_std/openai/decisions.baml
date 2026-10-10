/// OpenAI Decisions (POST {base_url}/decisions), with TypeSafe-style outputs.
/// Bool outputs threshold a predicate at 0.5; float outputs retain its probability.
/// Enums and finite unions use Choice. Classes flatten into one question per leaf.
/// Use role("instructions") or @description for questions, and user content for
/// evidence. Text and images are supported; tools and streaming are not.
/// Float outputs are probabilities, not the API's ordered-rubric score type.
class DecisionsClient {
    model: string,
    api_key: ai.Credential?,
    base_url: ai.Credential?,
    // The deadlines of one request attempt: a Duration is the end-to-end
    // total; ai.LlmTimeoutOptions also sets the connect limit. This client
    // does not stream, so first_token_timeout and stream_token_timeout are
    // ignored. Null is the default: 5 minutes total, 10 seconds to connect.
    timeout: baml.time.Duration | ai.LlmTimeoutOptions | null,
    capture_wire: bool,

    /// Construction is pure. Credentials resolve when invoking, using
    /// OPENAI_API_KEY unless an explicit credential is provided.
    function new(
        model: string = "gpt-6-luna",
        api_key: ai.Credential? = env.OPENAI_API_KEY,
        base_url: ai.Credential? = "https://api.openai.com/v1",
        timeout: baml.time.Duration | ai.LlmTimeoutOptions | null = null,
        capture_wire: bool = true,
    ) -> DecisionsClient {
        DecisionsClient {
            model: model,
            api_key: api_key,
            base_url: base_url,
            timeout: timeout,
            capture_wire: capture_wire,
        }
    }

    implements ai.Client {
        function id(self) -> string {
            `openai-decisions/${self.model}`
        }

        /// Preview the request without resolving credentials or performing IO.
        function render(self, input: ai.ModelTurnInput) -> baml.http.Request {
            root.internal.decisions.request(self, input, root.internal.decisions.plan(input), true) catch_all (e) {
                _ => throw ai.errors.normalize(e),
            }
        }

        function invoke(self, input: ai.ModelTurnInput) -> ai.ModelTurn {
            root.internal.decisions.invoke(self, input) catch_all (e) {
                _ => throw ai.errors.normalize_invoke(e),
            }
        }
    }
}
