class OpenAiClient {
    model: string,
    // null resolves OPENAI_API_KEY at request time; construction never
    // touches env, so declaring/constructing a client is always safe.
    api_key: string?,
    base_url: string?,
    // Names of environment variables to read INSTEAD, at request time.
    // A `client Name = ...` declaration is evaluated during `$init`, which
    // cannot run io sysops — so a declaration can never read env itself.
    // Naming the variable keeps the declaration pure while still letting a
    // deployment (or a replay harness) point the client somewhere else.
    api_key_env: string?,
    base_url_env: string?,

    function new(
        model: string = "gpt-4o-mini",
        api_key: string? = null,
        base_url: string? = null,
        api_key_env: string? = null,
        base_url_env: string? = null,
    ) -> OpenAiClient {
        OpenAiClient {
            model: model,
            api_key: api_key,
            base_url: base_url,
            api_key_env: api_key_env,
            base_url_env: base_url_env,
        }
    }

    // The request-time credential: an explicit value, else the named env
    // var, else the provider's canonical one.
    function resolved_api_key(self) -> string {
        if let k: string = self.api_key {
            return k;
        }
        if let name: string = self.api_key_env {
            if let v: string = baml.env.get(name) {
                return v;
            }
        }
        baml.env.get_or_panic("OPENAI_API_KEY")
    }

    // The request-time base URL, or null for the provider default.
    function resolved_base_url(self) -> string? {
        if let u: string = self.base_url {
            return u;
        }
        if let name: string = self.base_url_env {
            return baml.env.get(name);
        }
        null
    }

    implements ai.Client {
        function id(self) -> string {
            `openai/${self.model}`
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
