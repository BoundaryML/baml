// The Vercel AI Gateway image client: POST {base_url}/image-model.
//
// `client "ai-gateway-images/bfl/flux-2-pro"` in an LLM function constructs
// this class. The gateway fronts many image providers behind one endpoint; the
// model id — including its provider prefix — rides in the `ai-model-id` header,
// not in the body.
//
//     function DrawLamp(subject: string) -> image {
//         client "ai-gateway-images/bfl/flux-2-pro"
//         prompt `A studio photograph of ${subject}.`
//     }
//
// Image inputs ARE supported: an image prompt part becomes a `files[]` entry,
// which is how the endpoint does editing and variations —
//
//     function Restyle(source: image) -> image {
//         client "ai-gateway-images/openai/gpt-image-1"
//         prompt `Redraw ${source} in watercolor.`
//     }
//
// Audio/video/pdf inputs have no representation and are rejected, as is tool
// calling. (`mask`, the other editing input, has no first-class field yet; pass
// it through `request_body` as `{"mask": {"type": "file", "mediaType": …,
// "data": …}}`.)
class AiGatewayImageClient {
    // The gateway model id, e.g. "bfl/flux-2-pro" — sent as `ai-model-id`.
    model: string,
    // A literal string (including a strict `env.NAME` lookup), an explicit
    // `baml.env.Ref` read at request time, or null to resolve
    // AI_GATEWAY_API_KEY at request time.
    api_key: ai.Credential?,
    // Defaults to https://ai-gateway.vercel.sh/v4/ai.
    base_url: ai.Credential?,

    // First-class AI SDK image-model parameters, lowered only when non-null.
    //   n             number of images; the wire default is 1
    //   size          "1024x1024"-style explicit resolution
    //   aspect_ratio  "16:9"-style ratio (sent as `aspectRatio`)
    //   seed          deterministic generation seed
    n: int?,
    size: string?,
    aspect_ratio: string?,
    seed: int?,

    // Escape hatches. `request_body` is deep-merged into the serialized typed
    // request as the last step (a null value deletes that key) — this is where
    // `providerOptions` goes, e.g.
    // `{"providerOptions": {"blackForestLabs": {"outputFormat": "jpeg"}}}`,
    // which also selects the MIME type of the returned image values.
    request_body: baml.json.json?,
    headers: map<string, string>?,
    query_params: map<string, string>?,

    function new(
        model: string = "openai/gpt-image-1",
        api_key: ai.Credential? = null,
        base_url: ai.Credential? = null,
        n: int? = null,
        size: string? = null,
        aspect_ratio: string? = null,
        seed: int? = null,
        request_body: baml.json.json? = null,
        headers: map<string, string>? = null,
        query_params: map<string, string>? = null,
    ) -> AiGatewayImageClient {
        AiGatewayImageClient {
            model: model,
            api_key: api_key,
            base_url: base_url,
            n: n,
            size: size,
            aspect_ratio: aspect_ratio,
            seed: seed,
            request_body: request_body,
            headers: headers,
            query_params: query_params,
        }
    }

    // The request-time credential: an explicit value, else the gateway's
    // conventional variable. (sys_llm had no env default here and required
    // an explicit `api_key` in every client declaration.)
    // A missing credential is a catchable ai.errors.InvalidRequest rather than
    // a panic, so the caller's `catch_all` can fall back to another client.
    function resolved_api_key(self) -> string {
        if let key: string = ai.wire.resolve_credential(self.api_key, "AI_GATEWAY_API_KEY") {
            return key;
        }
        throw ai.errors.InvalidRequest {
            provider: "ai-gateway-images",
            status_code: null,
            detail: `no AI Gateway API key: set the client's api_key (a literal, env.NAME, or baml.env.ref("NAME")), or export one of the variables it consulted — ${ai.wire.credential_sources(self.api_key, "AI_GATEWAY_API_KEY")}`,
            raw_body: null,
        }
    }

    // The request-time base URL, or null for the gateway default.
    function resolved_base_url(self) -> string? {
        ai.wire.resolve_credential(self.base_url, null)
    }

    implements ai.Client {
        function id(self) -> string {
            root.internal._gw_images_client_id(self)
        }

        function render(self, input: ai.ModelTurnInput) -> baml.http.Request {
            vercel.internal.gateway_images_render(self, input, preview = true) catch_all (e) {
                _ => throw ai.errors.normalize(e),
            }
        }

        function invoke(self, input: ai.ModelTurnInput) -> ai.ModelTurn {
            root.internal.gateway_images_invoke(self, input) catch_all (e) {
                _ => throw ai.errors.normalize_invoke(e),
            }
        }
    }

    // No ai.stream.StreamingClient: the gateway image endpoint answers once
    // with the finished images. `ai.stream.from_spec` reports that as
    // ai.errors.StreamingUnsupported.
}
