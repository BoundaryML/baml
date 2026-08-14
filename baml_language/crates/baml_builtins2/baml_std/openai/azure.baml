// openai.AzureClient — Azure OpenAI Service chat completions.
//
// Azure differs from OpenAI in three ways, all of them config:
//   * the deployment is part of the URL —
//     `https://{resource_name}.openai.azure.com/openai/deployments/{deployment_id}/chat/completions`
//     — with the API version as a query parameter;
//   * the credential rides in an `api-key` header, not `Authorization: Bearer`;
//   * a token ceiling is effectively mandatory, so 4096 is injected when the
//     caller set neither `max_tokens` nor `max_completion_tokens` (including
//     through `request_body`). Passing `request_body = {"max_tokens": null}`
//     removes it again — the merge runs after the default is applied.
//
// Supply either `base_url` OR both `resource_name` and `deployment_id`.

class AzureClient {
    // The model name reported in `id()` and sent in the body. Azure routes on
    // the deployment, so this is usually the same string as deployment_id.
    model: string,
    // Azure resource name — the `{name}` in `{name}.openai.azure.com`.
    resource_name: string?,
    // The deployment to call.
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
    base_url: string?,

    api_key: string?,
    api_key_env: string?,
    base_url_env: string?,

    request_body: baml.json.json?,
    headers: map<string, string>?,
    query_params: map<string, string>?,

    temperature: float?,
    max_tokens: int?,
    max_completion_tokens: int?,
    top_p: float?,
    stop: string[]?,
    seed: int?,

    function new(
        model: string,
        resource_name: string? = null,
        deployment_id: string? = null,
        api_version: string? = "2024-10-21",
        base_url: string? = null,
        api_key: string? = null,
        api_key_env: string? = null,
        base_url_env: string? = null,
        request_body: baml.json.json? = null,
        headers: map<string, string>? = null,
        query_params: map<string, string>? = null,
        temperature: float? = null,
        max_tokens: int? = null,
        max_completion_tokens: int? = null,
        top_p: float? = null,
        stop: string[]? = null,
        seed: int? = null,
    ) -> AzureClient {
        AzureClient {
            model: model,
            resource_name: resource_name,
            deployment_id: deployment_id,
            api_version: api_version,
            base_url: base_url,
            api_key: api_key,
            api_key_env: api_key_env,
            base_url_env: base_url_env,
            request_body: request_body,
            headers: headers,
            query_params: query_params,
            temperature: temperature,
            max_tokens: max_tokens,
            max_completion_tokens: max_completion_tokens,
            top_p: top_p,
            stop: stop,
            seed: seed,
        }
    }

    function resolved_api_key(self) -> string? {
        root.internal.chat_resolve_api_key(self.api_key, self.api_key_env, "AZURE_OPENAI_API_KEY")
    }

    // base_url XOR (resource_name AND deployment_id). The engine reports this
    // at compile time with per-key spans; a BAML constructor can only report it
    // when the request is built, so the message names every accepted shape.
    function resolved_base_url(self) -> string {
        if let explicit: string = self.base_url {
            if (self.resource_name != null || self.deployment_id != null) {
                throw baml.errors.InvalidArgument {
                    message: "openai.AzureClient takes base_url OR both resource_name and deployment_id, not both",
                }
            }
            return explicit;
        }
        if let name: string = self.base_url_env {
            if let value: string = baml.env.get(name) {
                return value;
            }
        }
        let resource = self.resource_name;
        let deployment = self.deployment_id;
        if let resource_name: string = resource {
            if let deployment_id: string = deployment {
                return `https://${resource_name}.openai.azure.com/openai/deployments/${deployment_id}`;
            }
            throw baml.errors.InvalidArgument {
                message: "openai.AzureClient needs deployment_id when resource_name is set",
            }
        }
        if (deployment != null) {
            throw baml.errors.InvalidArgument {
                message: "openai.AzureClient needs resource_name when deployment_id is set",
            }
        }
        throw baml.errors.InvalidArgument {
            message: "openai.AzureClient needs either base_url or both resource_name and deployment_id",
        }
    }

    function compat(self) -> root.internal.ChatCompat {
        let query: map<string, string> = {  };
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

    function params(self) -> root.internal.ChatParams {
        root.internal.ChatParams.new(
            model = self.model,
            api_key = self.resolved_api_key(),
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

        function invoke(self, input: ai.ModelTurnInput) -> ai.ModelTurn {
            root.internal.chat_invoke(self.compat(), self.params(), input) catch_all (e) {
                _ => throw ai.errors.normalize(e),
            }
        }
    }

    implements ai.stream.StreamingClient {
        function invoke_stream(self, input: ai.ModelTurnInput) -> ai.stream.TurnStream {
            root.internal.chat_invoke_stream(self.compat(), self.params(), input) catch_all (e) {
                _ => throw ai.errors.normalize(e),
            }
        }
    }
}
