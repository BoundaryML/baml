/// TypeSafe AI's System One API. A BAML function makes typed questions about its
/// rendered text state. Supported results are `bool` (Noul), an enum (Choice),
/// `float` (Noul probability, or Score when `score_criteria` is configured),
/// unions of string literals or enums (Choice), and classes of those fields,
/// including nested classes. Each leaf field becomes a separate question in
/// one request and needs an `@description` or doc comment.
/// Other result types fail with `ai.errors.InvalidRequest` before HTTP is sent.
/// The client builds questions from the return type; prompts do not need
/// `ctx.output_format()`.
///
///     function IsUrgent(message: string) -> bool {
///         client: "typesafeai/jev-latest"
///         prompt: `Does this message convey urgency? ${message}`
///     }
class Client {
    model: string,
    api_key: ai.Credential?,
    base_url: ai.Credential?,
    /// Overrides the question instruction for a non-class output. For class
    /// outputs, each leaf field's `@description` or doc comment supplies its
    /// instruction, and this value is unused. The rendered prompt is always
    /// the request's `state`; by default the instruction asks Jev to answer
    /// the question contained in that state.
    instructions: string?,
    /// For a `float` output, switch from Noul probability to Score. Each entry
    /// describes one ordered level; Jev returns a probability-weighted score
    /// across their zero-based indices.
    score_criteria: string[]?,
    noul_threshold: float,
    /// Total HTTP deadline. Null uses the transport default.
    request_timeout_ms: int?,
    capture_wire: bool,

    function new(
        model: string = "jev-latest",
        api_key: ai.Credential? = null,
        base_url: ai.Credential? = null,
        instructions: string? = null,
        score_criteria: string[]? = null,
        noul_threshold: float = 0.5,
        request_timeout_ms: int? = null,
        capture_wire: bool = true,
    ) -> Client {
        Client {
            model: model,
            api_key: api_key,
            base_url: base_url,
            instructions: instructions,
            score_criteria: score_criteria,
            noul_threshold: noul_threshold,
            request_timeout_ms: request_timeout_ms,
            capture_wire: capture_wire,
        }
    }

    function resolved_api_key(self) -> string {
        if let key: string = ai.wire.resolve_credential(self.api_key, "TYPESAFE_API_KEY") {
            return key;
        }
        throw ai.errors.InvalidRequest {
            provider: "typesafeai",
            status_code: null,
            detail: `no TypeSafe API key: set api_key or export ${ai.wire.credential_sources(self.api_key, "TYPESAFE_API_KEY")}`,
            raw_body: null,
        }
    }

    /// Build a native System One request without resolving the API key. State
    /// and question instructions may be strings, objects, or arrays.
    function build_system_one_request(
        self,
        state: baml.json.json,
        questions: map<string, Question>,
    ) -> baml.http.Request {
        root.internal.system_one_request(self, state, questions, preview = true)
    }

    /// Evaluate native questions and return every answer field and token usage.
    ///
    ///     client.system_one(
    ///         { "text": message },
    ///         { "urgent": typesafeai.noul("Does this need immediate attention?") },
    ///     )
    /// Use this method for structured state, structured instructions, and
    /// questions whose full probability distribution matters.
    function system_one(self, state: baml.json.json, questions: map<string, Question>) -> Response {
        root.internal.system_one(self, state, questions) catch_all (e) {
            _ => throw ai.errors.normalize_invoke(e),
        }
    }

    /// List the model names and aliases available to this account.
    function build_list_models_request(self) -> baml.http.Request {
        root.internal.list_models_request(self, preview = true)
    }

    /// List the model names and aliases available to this account.
    function list_models(self) -> ModelsResponse {
        root.internal.list_models(self) catch_all (e) {
            _ => throw ai.errors.normalize_invoke(e),
        }
    }

    implements ai.Client {
        function id(self) -> string throws never {
            `typesafeai/${self.model}`
        }

        function render(self, input: ai.ModelTurnInput) -> baml.http.Request {
            root.internal.render(self, input, preview = true) catch_all (e) {
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
