// Internal helpers behind `ai.clients.resolve`: the runtime half of the
// `"provider/model"` shorthand and the lookup of a declared client by name.
// Nothing here is part of the public ai surface.

/// (internal) The client a selector string names: a `"provider/model"`
/// shorthand, or, without a "/", the `client Name = ...` declaration of
/// `package` called `selector`.
///
/// `providers` maps a prefix and model to the constructed client, or `null`
/// for an unknown prefix, and `prefixes` lists every prefix it knows (for
/// error messages). This package cannot build either: every provider package
/// implements `root.Client`, so `ai` sits below them in the package graph. The
/// compiler synthesizes both from its own provider table (the same one a
/// literal `client "provider/model"` lowers through) at each
/// `root.clients.resolve` call, and passes the calling package as `package`.
function _from_string(
    selector: string,
    providers: (string, string) -> root.Client?,
    prefixes: string[],
    package: reflect.Package,
) -> root.Client {
    let index: int = selector.index_of("/") else {
        return _declared(selector, prefixes, package);
    };
    let prefix = selector.slice(0, index);
    let model = selector.slice(index + 1, selector.length());
    if let built: root.Client = providers(prefix, model) {
        built
    } else if (prefix == "azure") {
        // The compiler rejects a literal `"azure/…"` the same way
        // (`AZURE_SHORTHAND_REMOVED` in `lower_cst.rs`).
        throw root.errors.InvalidRequest {
            provider: "ai.clients",
            status_code: null,
            detail: `client "${selector}": there is no "azure/<model>" shorthand: an Azure request goes to a deployment, which a model name does not identify; build openai.AzureClient.new(model = "${model}", deployment_id = ..., resource_name = ...)`,
            raw_body: null,
        }
    } else {
        throw root.errors.InvalidRequest {
            provider: "ai.clients",
            status_code: null,
            detail: `no builtin provider for prefix "${prefix}" in client "${selector}"; valid prefixes are ${prefixes.join(", ")} (for any other OpenAI-compatible endpoint, build openai.GenericClient.new(base_url = ..., model = "${model}"))`,
            raw_body: null,
        }
    }
}

/// (internal) The client `package` declares as `client <name> = ...`. A name the
/// package does not declare, or declares as something other than a client, is
/// an `InvalidRequest`.
function _declared(name: string, prefixes: string[], package: reflect.Package) -> root.Client {
    let declared: root.Client = package.get_let(name) else {
        throw root.errors.InvalidRequest {
            provider: "ai.clients",
            status_code: null,
            detail: `client "${name}" is not a "provider/model" string (no "/") and no \`client ${name} = ...\` is declared; valid prefixes are ${prefixes.join(", ")}`,
            raw_body: null,
        }
    };
    declared
}
