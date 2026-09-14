// Internal helpers behind `ai.clients.resolve`: the runtime half of the
// `"provider/model"` shorthand. Nothing here is part of the public ai surface.

/// (internal) Construct the client a `"provider/model"` shorthand names.
///
/// `providers` maps a prefix and model to the constructed client, or `null`
/// for an unknown prefix, and `prefixes` lists every prefix it knows (for
/// error messages). This package cannot build either: every provider package
/// implements `root.Client`, so `ai` sits below them in the package graph. The
/// compiler synthesizes both from its own provider table (the same one a
/// literal `client "provider/model"` lowers through) at each
/// `root.clients.resolve` call.
function _from_shorthand(
    shorthand: string,
    providers: (string, string) -> root.Client?,
    prefixes: string[],
) -> root.Client {
    let separator = shorthand.index_of("/");
    if let index: int = separator {
        let prefix = shorthand.slice(0, index);
        let model = shorthand.slice(index + 1, shorthand.length());
        if let built: root.Client = providers(prefix, model) {
            built
        } else {
            throw root.errors.InvalidRequest {
                provider: "ai.clients",
                status_code: null,
                detail: `no builtin provider for prefix "${prefix}" in client "${shorthand}"; valid prefixes are ${prefixes.join(", ")} (for any other OpenAI-compatible endpoint, build openai.GenericClient.new(base_url = ..., model = "${model}"))`,
                raw_body: null,
            }
        }
    } else {
        throw root.errors.InvalidRequest {
            provider: "ai.clients",
            status_code: null,
            detail: `client "${shorthand}" is not a "provider/model" string (no "/"); valid prefixes are ${prefixes.join(", ")}`,
            raw_body: null,
        }
    }
}
