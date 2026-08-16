// The OpenAI image-generation client: POST {base_url}/images/generations.
//
// `client "openai-images/gpt-image-1"` in an LLM function constructs this
// class. Its output type is media — `-> image`, `-> image[]`, `-> image?`,
// `-> string | image` — which the runner builds from the turn's media blocks
// (ai/runner.baml) instead of parsing text.
//
//     function DrawLamp(subject: string) -> image {
//         client "openai-images/gpt-image-1"
//         prompt `A studio photograph of ${subject}.`
//     }
//
// The endpoint takes a text prompt only: media inputs are rejected (image
// editing is a different, multipart endpoint) and tools are not supported.
class ImageClient {
    model: string,
    // A literal string, `env.NAME` (a late-bound `baml.env.Ref` read at request
    // time), or null to resolve OPENAI_API_KEY at request time. Construction
    // never touches env, so declaring/constructing a client is always safe.
    api_key: ai.Credential?,
    base_url: ai.Credential?,

    // First-class Images API parameters. Each is lowered into the typed
    // request only when non-null, so an unset option is absent on the wire and
    // the API's own default applies.
    //
    // The tables below are the documented values at the time of writing; the
    // client sends whatever string it is given and lets the API reject an
    // impossible combination, so a newer model's values work without a change
    // here. "GPT image" covers every `gpt-image-*` id — gpt-image-1,
    // gpt-image-1-mini, gpt-image-1.5, gpt-image-2, …
    //
    //   n                   1..10 images per call (dall-e-3 allows only 1)
    //   size                "1024x1024" | "1536x1024" | "1024x1536" | "auto"
    //                       (dall-e-2: 256x256 | 512x512 | 1024x1024;
    //                        dall-e-3: 1024x1024 | 1792x1024 | 1024x1792;
    //                        gpt-image-2 also takes arbitrary WIDTHxHEIGHT
    //                        strings, each side divisible by 16, up to
    //                        3840x2160)
    //   quality             "low" | "medium" | "high" | "auto"
    //                       (dall-e: "standard" | "hd")
    //   background          "transparent" | "opaque" | "auto"  (GPT image only)
    //   output_format       "png" | "jpeg" | "webp"            (GPT image only)
    //                       — also the MIME type of the returned image value
    //   output_compression  0..100, jpeg/webp only             (GPT image only)
    //   moderation          "low" | "auto"                     (GPT image only)
    //   style               "vivid" | "natural"                (dall-e-3 only)
    //   user                end-user id for abuse monitoring
    //   response_format     "b64_json" | "url"                 (dall-e only;
    //                       defaulted per model — see resolved behavior below.
    //                       A "url" answer is fetched and inlined, so the
    //                       value always carries real bytes.)
    n: int?,
    size: string?,
    quality: string?,
    background: string?,
    output_format: string?,
    output_compression: int?,
    moderation: string?,
    style: string?,
    user: string?,
    response_format: string?,

    // Escape hatches. `request_body` is deep-merged into the serialized typed
    // request as the last step (a null value deletes that key); `headers` is
    // applied over the provider defaults before auth; `query_params` is
    // appended to the URL.
    request_body: baml.json.json?,
    headers: map<string, string>?,
    query_params: map<string, string>?,

    function new(
        model: string = "gpt-image-1",
        api_key: ai.Credential? = null,
        base_url: ai.Credential? = null,
        n: int? = null,
        size: string? = null,
        quality: string? = null,
        background: string? = null,
        output_format: string? = null,
        output_compression: int? = null,
        moderation: string? = null,
        style: string? = null,
        user: string? = null,
        response_format: string? = null,
        request_body: baml.json.json? = null,
        headers: map<string, string>? = null,
        query_params: map<string, string>? = null,
    ) -> ImageClient {
        ImageClient {
            model: model,
            api_key: api_key,
            base_url: base_url,
            n: n,
            size: size,
            quality: quality,
            background: background,
            output_format: output_format,
            output_compression: output_compression,
            moderation: moderation,
            style: style,
            user: user,
            response_format: response_format,
            request_body: request_body,
            headers: headers,
            query_params: query_params,
        }
    }

    // The request-time credential: an explicit value, else the provider's
    // canonical one. A missing credential is a catchable
    // ai.errors.InvalidRequest rather than a panic, so the caller's
    // `catch_all` can fall back to another client.
    function resolved_api_key(self) -> string {
        if let key: string = ai.wire.resolve_credential(self.api_key, "OPENAI_API_KEY") {
            return key;
        }
        throw ai.errors.InvalidRequest {
            provider: "openai-images",
            status_code: null,
            detail: `no OpenAI API key: set the client's api_key (a literal or env.NAME), or export one of the variables it consulted — ${ai.wire.credential_sources(self.api_key, "OPENAI_API_KEY")}`,
            raw_body: null,
        }
    }

    // The request-time base URL, or null for the provider default.
    function resolved_base_url(self) -> string? {
        ai.wire.resolve_credential(self.base_url, null)
    }

    implements ai.Client {
        function id(self) -> string {
            root.internal._oai_images_client_id(self)
        }

        function render(self, input: ai.ModelTurnInput) -> baml.http.Request {
            openai.internal.images_render(self, input, preview = true) catch_all (e) {
                _ => throw ai.errors.normalize(e),
            }
        }

        function invoke(self, input: ai.ModelTurnInput) -> ai.ModelTurn {
            root.internal.images_invoke(self, input) catch_all (e) {
                _ => throw ai.errors.normalize_invoke(e),
            }
        }
    }

    // No ai.stream.StreamingClient: the endpoint's streaming mode emits
    // partially rendered image previews, which ai.stream.StreamEvent (text
    // deltas plus turn metadata) cannot carry. `ai.stream.from_spec` reports
    // that as ai.errors.StreamingUnsupported.
}
