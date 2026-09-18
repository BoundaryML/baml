/// TypeSafe's JEV classifier (POST {base_url}/systemone).
/// Bool and float outputs use Noul; enums and finite unions use Choice.
/// Classes are flattened into one question per leaf. Unsupported output
/// types, empty instructions, tools, and media are rejected before HTTP.
/// JEV does not stream. Use invoke or an ordinary LLM function call.
class Client {
    model: string,
    api_key: ai.Credential?,
    base_url: ai.Credential?,
    request_timeout_ms: int?,
    capture_wire: bool,

    /// Construction is pure. Credentials resolve when invoking, using
    /// TYPESAFE_API_KEY unless an explicit credential is provided.
    function new(
        model: string = "jev-latest",
        api_key: ai.Credential? = null,
        base_url: ai.Credential? = null,
        request_timeout_ms: int? = null,
        capture_wire: bool = true,
    ) -> Client {
        Client {
            model: model,
            api_key: api_key,
            base_url: base_url,
            request_timeout_ms: request_timeout_ms,
            capture_wire: capture_wire,
        }
    }

    implements ai.Client {
        function id(self) -> string {
            `typesafeai/${self.model}`
        }

        /// Preview the request without resolving credentials or performing IO.
        function render(self, input: ai.ModelTurnInput) -> baml.http.Request {
            root.internal.request(self, input, root.internal.plan(input), true) catch_all (e) {
                _ => throw ai.errors.normalize(e),
            }
        }

        function invoke(self, input: ai.ModelTurnInput) -> ai.ModelTurn {
            root.internal.invoke(self, input) catch_all (e) {
                _ => throw ai.errors.normalize_invoke(e),
            }
        }
    }
}
