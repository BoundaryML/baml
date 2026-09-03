/// Azure OpenAI Service chat completions.
///
/// Azure differs from OpenAI in three ways, all of them config:
///   * the deployment is part of the URL —
///     `https://{resource_name}.openai.azure.com/openai/deployments/{deployment_id}/chat/completions`
///     — with the API version as a query parameter;
///   * the credential rides in an `api-key` header, not `Authorization: Bearer`;
///   * a token ceiling is effectively mandatory, so 4096 is injected when the
///     caller set neither `max_tokens` nor `max_completion_tokens` (including
///     through `request_body`). Passing `request_body = {"max_tokens": null}`
///     removes it again — the merge runs after the default is applied.
///
/// Supply either `base_url` OR both `resource_name` and `deployment_id`.
class AzureClient {
    // The model name reported in `id()` and sent in the body. Azure routes on
    // the deployment, so this is usually the same string as deployment_id.
    model: string,
    // Azure resource name — the `{name}` in `{name}.openai.azure.com`.
    resource_name: string?,
    // The deployment to call. Unset defaults to `model`: Azure deployments are
    // overwhelmingly named after the model they serve, and that convention is
    // what lets `client: "azure/gpt-4o"` — which can only carry a model —
    // reach a real deployment.
    deployment_id: string?,
    // The `api-version` query parameter. Defaults to the current GA dated
    // version, `2024-10-21`: every preview before `2024-09-01-preview` predates
    // `max_completion_tokens` and `stream_options.include_usage`, both of which
    // this client can send, so an older default silently loses them.
    //
    // Null omits the parameter (the dated API then rejects the call, so this is
    // only for callers routing through a proxy that supplies its own).
    //
    // Future option, deliberately not implemented here: Azure's v1 API
    // (`{resource}.openai.azure.com/openai/v1/`, no `api-version` at all, GA
    // since Aug 2025) is the direction Microsoft now recommends. Adopting it
    // means changing `resolved_base_url` to emit the `/openai/v1` prefix
    // without the deployment segment as well as dropping the query parameter,
    // which is a wire change for every existing caller — a separate decision.
    api_version: string?,
    // Full override of the deployment base, e.g. through a private endpoint or
    // gateway. Mutually exclusive with resource_name/deployment_id.
    base_url: ai.Credential?,

    api_key: ai.Credential?,

    request_body: baml.json.json?,
    headers: map<string, string>?,
    query_params: map<string, string>?,

    temperature: float?,
    max_tokens: int?,
    max_completion_tokens: int?,
    top_p: float?,
    stop: string[]?,
    seed: int?,
    // Record request/response bodies (and raw SSE events) on the LLMCall wire
    // records delivered through on_event. Status, headers, and timing are
    // always recorded; this flag only gates the bodies.
    capture_wire: bool,

    /// Creates a client. Construction reads no environment variable, so
    /// declaring one is always safe; credentials and endpoints resolve when a
    /// request is built.
    function new(
        model: string,
        resource_name: string? = null,
        deployment_id: string? = null,
        api_version: string? = "2024-10-21",
        base_url: ai.Credential? = null,
        api_key: ai.Credential? = null,
        request_body: baml.json.json? = null,
        headers: map<string, string>? = null,
        query_params: map<string, string>? = null,
        temperature: float? = null,
        max_tokens: int? = null,
        max_completion_tokens: int? = null,
        top_p: float? = null,
        stop: string[]? = null,
        seed: int? = null,
        capture_wire: bool = true,
    ) -> AzureClient {
        AzureClient {
            model: model,
            resource_name: resource_name,
            deployment_id: deployment_id,
            api_version: api_version,
            base_url: base_url,
            api_key: api_key,
            request_body: request_body,
            headers: headers,
            query_params: query_params,
            temperature: temperature,
            max_tokens: max_tokens,
            max_completion_tokens: max_completion_tokens,
            top_p: top_p,
            stop: stop,
            seed: seed,
            capture_wire: capture_wire,
        }
    }

    /// The request-time credential: the explicit value, else
    /// `AZURE_OPENAI_API_KEY`. Azure sends it in an `api-key` header rather
    /// than as a bearer token.
    function resolved_api_key(self) -> string? {
        ai.wire.resolve_credential(self.api_key, "AZURE_OPENAI_API_KEY")
    }

    /// The deployment segment: the caller's `deployment_id`, else the model.
    function resolved_deployment_id(self) -> string {
        self.deployment_id ?? self.model
    }

    /// The deployment base this client posts to, resolved in the order Azure
    /// users actually configure things:
    ///
    ///   1. an explicit `base_url` (a literal or an `env.NAME` ref), used
    ///      verbatim — it already names the deployment;
    ///   2. `resource_name`, which builds
    ///      `https://{resource}.openai.azure.com/openai/deployments/{deployment}`;
    ///   3. `AZURE_OPENAI_ENDPOINT` — the variable the official Azure OpenAI
    ///      SDKs read — treated as the resource ORIGIN and given the same
    ///      `/openai/deployments/{deployment}` path as rung 2.
    ///
    /// Rung 3 is what makes the `"azure/<model>"` shorthand viable. The
    /// shorthand can only supply a model, and every other provider's shorthand
    /// works off ambient credentials; without an endpoint fallback this client
    /// could never send. With `deployment_id` defaulting to `model`, ambient
    /// `AZURE_OPENAI_ENDPOINT` + `AZURE_OPENAI_API_KEY` is a complete config.
    ///
    /// The engine reports the mutually-exclusive shapes at compile time with
    /// per-key spans; a BAML constructor can only report at request time, so
    /// the error names every source that was checked.
    function resolved_base_url(self) -> string {
        // A `base_url = env.NAME` ref resolves here, at request time; an unset
        // variable behaves exactly like no base_url at all.
        if let explicit: string = ai.wire.resolve_credential(self.base_url, null) {
            if (self.resource_name != null || self.deployment_id != null) {
                throw root.internal._azure_reject(
                    "openai.AzureClient takes base_url OR resource_name/deployment_id, not both",
                )
            }
            return explicit;
        }
        let deployment = self.resolved_deployment_id();
        if let resource_name: string = self.resource_name {
            return `https://${resource_name}.openai.azure.com/openai/deployments/${deployment}`;
        }
        // Azure prints the endpoint with a trailing slash in the portal, so
        // accept it either way rather than emitting a doubled separator.
        if let endpoint: string = ai.wire.resolve_credential(null, "AZURE_OPENAI_ENDPOINT") {
            let origin = if (endpoint.ends_with("/")) {
                endpoint.slice(0, endpoint.length() - 1)
            } else {
                endpoint
            };
            return `${origin}/openai/deployments/${deployment}`;
        }
        throw root.internal._azure_reject(
            "openai.AzureClient needs a deployment endpoint: set base_url, or resource_name, or the AZURE_OPENAI_ENDPOINT environment variable",
        )
    }

    /// The provider record: everything the shared chat core needs to know
    /// about this endpoint.
    function compat(self) -> root.internal.ChatCompat {
        let query: map<string, string> = {};
        if let version: string = self.api_version {
            let _ = query.set("api-version", version);
        }
        root.internal.ChatCompat.new(
            provider = "azure",
            base_url = self.resolved_base_url(),
            auth = root.internal.ChatAuth.ApiKeyHeader,
            api_key_env = "AZURE_OPENAI_API_KEY",
            api_key_required = true,
            default_max_tokens = 4096,
            compat_query = query,
        )
    }

    /// The per-request parameters handed to the shared chat core.
    ///
    /// # Parameters
    ///
    /// - `preview`: omit the credential, for rendering a request without
    ///   sending it.
    function params(self, preview: bool = false) -> root.internal.ChatParams {
        root.internal.ChatParams.new(
            model = self.model,
            api_key = if (preview) {
                null
            } else {
                self.resolved_api_key()
            },
            temperature = self.temperature,
            max_tokens = self.max_tokens,
            max_completion_tokens = self.max_completion_tokens,
            top_p = self.top_p,
            stop = self.stop,
            seed = self.seed,
            headers = self.headers,
            query_params = self.query_params,
            request_body = self.request_body,
        )
    }

    implements ai.Client {
        function id(self) -> string {
            `azure/${self.model}`
        }

        function render(self, input: ai.ModelTurnInput) -> baml.http.Request {
            openai.internal.chat_preview(self.compat(), self.params(preview = true), input) catch_all (e) {
                _ => throw ai.errors.normalize(e),
            }
        }

        function invoke(self, input: ai.ModelTurnInput) -> ai.ModelTurn {
            root.internal.chat_invoke(
                self.compat(),
                self.params(),
                input,
                capture_wire = self.capture_wire,
            ) catch_all (e) {
                _ => throw ai.errors.normalize_invoke(e),
            }
        }
    }

    implements ai.stream.StreamingClient {
        function invoke_stream(self, input: ai.ModelTurnInput) -> ai.stream.TurnStream {
            root.internal.chat_invoke_stream(
                self.compat(),
                self.params(),
                input,
                capture_wire = self.capture_wire,
            ) catch_all (e) {
                _ => throw ai.errors.normalize_invoke(e),
            }
        }
    }
}
