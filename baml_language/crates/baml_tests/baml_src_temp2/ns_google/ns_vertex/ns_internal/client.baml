function _merged_headers(provider: root.google.vertex.Gemini) -> map<string, string>
    throws never {
    let headers: map<string, string> = {};
    match (provider.extra_headers) {
        let extra: map<string, string> => {
            for (let key in extra.keys()) {
                headers.set(key, extra[key]);
            }
        },
        null => {},
    }
    headers
}

function _merged_query(provider: root.google.vertex.Gemini) -> map<string, string>
    throws never {
    let query: map<string, string> = {};
    match (provider.extra_query) {
        let extra: map<string, string> => {
            for (let key in extra.keys()) {
                query.set(key, extra[key]);
            }
        },
        null => {},
    }
    // Vertex Express Mode accepts an API key at the project-less publisher
    // endpoint. Standard project/location Vertex remains ADC-only here.
    match (provider.api_key) {
        let key: string => query.set("key", key),
        null => {},
    }
    query
}

function _is_reserved_body_key(key: string) -> bool throws never {
    key == "contents"
        || key == "systemInstruction"
        || key == "tools"
        || key == "toolConfig"
}

// Prompt/media and function protocol fields are constructed by the adapter.
// generationConfig remains mergeable here; the shared generation/tool builders
// subsequently overwrite or remove response schema fields they own.
function _request_body(
    provider: root.google.vertex.Gemini,
    base_body: map<string, unknown>,
) -> map<string, unknown> throws never {
    let body: map<string, unknown> = {};
    for (let key in base_body.keys()) {
        if (!_is_reserved_body_key(key)) {
            body.set(key, base_body[key]);
        }
    }
    match (provider.extra_body) {
        let extra: map<string, unknown> => {
            for (let key in extra.keys()) {
                if (!_is_reserved_body_key(key)) {
                    body.set(key, extra[key]);
                }
            }
        },
        null => {},
    }
    body
}

function _primitive(provider: root.google.vertex.Gemini) -> baml.llm.PrimitiveClient
    throws baml.errors.UnknownError {
    let base = baml.llm.from_shorthand(provider.render_shorthand()) catch (e) {
        _ => throw baml.errors.UnknownError.with_message<never>(
            e,
            "Vertex Gemini provider shorthand failed",
        ),
    };
    let resolved_base_url = match (provider.api_key) {
        let key: string => provider.base_url
            ?? "https://aiplatform.googleapis.com/v1/publishers/google/models",
        null => provider.base_url,
    };
    baml.llm.PrimitiveClient {
        ...base,
        options: baml.llm.PrimitiveClientOptions {
            ...base.options,
            model: provider.model,
            base_url: resolved_base_url,
            headers: _merged_headers(provider),
            query_params: _merged_query(provider),
            request_body: _request_body(provider, base.options.request_body),
            provider_options: baml.llm.VertexAiOptions {
                credentials: provider.credentials,
                credentials_content: provider.credentials_content,
                location: provider.location,
                project_id: provider.project_id,
            },
        },
    }
}

function _use_sap(provider: root.google.vertex.Gemini) -> bool throws never {
    provider.output_mode == root.google.vertex.OutputMode.Sap
}

function _validate(provider: root.google.vertex.Gemini) -> null
    throws root.ai.InvalidRequest {
    if (
        provider.tool_mode == root.google.vertex.ToolMode.Prompt
            && provider.output_mode != root.google.vertex.OutputMode.Sap
    ) {
        throw root.ai.InvalidRequest {
            provider: provider.name(),
            status_code: null,
            detail: "`tool_mode = Prompt` requires `output_mode = Sap`",
        };
    }
    if (
        provider.api_key != null
            && (
                provider.project_id != null
                    || provider.location != null
                    || provider.credentials != null
                    || provider.credentials_content != null
            )
    ) {
        throw root.ai.InvalidRequest {
            provider: provider.name(),
            status_code: null,
            detail: "Vertex api_key selects Express Mode and cannot be combined with project/location/ADC settings",
        };
    }
    null
}

// Request-shape helper for tests and diagnostics. It builds and authenticates
// the complete request but does not send it.
function _build_request<T>(
    provider: root.google.vertex.Gemini,
    task: root.ai.Task<T>,
) -> baml.http.Request throws root.ai.Failure | baml.errors.UnknownError {
    _validate(provider);
    root.google.internal._generation_request(
        _primitive(provider),
        task,
        provider.name(),
        _use_sap(provider),
    )
}

function _generate<T>(
    provider: root.google.vertex.Gemini,
    task: root.ai.Task<T>,
) -> root.ai.ResponseWithMetadata<T>
    throws root.ai.Failure | baml.errors.UnknownError {
    _validate(provider);
    root.google.internal._generate(
        _primitive(provider),
        task,
        provider.name(),
        provider.model,
        _use_sap(provider),
    )
}

function _complete<T>(
    provider: root.google.vertex.Gemini,
    task: root.ai.Task<T>,
) -> root.ai.ResponseWithMetadata<T>
    throws root.ai.Failure | baml.errors.UnknownError {
    _validate(provider);
    root.google.internal._complete(
        provider,
        provider,
        task,
        provider.name(),
    )
}

function _begin<T>(
    provider: root.google.vertex.Gemini,
    task: root.ai.Task<T>,
) -> root.ai.Conversation throws root.ai.Failure | baml.errors.UnknownError {
    _validate(provider);
    root.google.internal._new_conversation(
        provider,
        _primitive(provider),
        provider.name(),
        provider.model,
        task,
    )
}

function _save(
    provider: root.google.vertex.Gemini,
    conversation: root.ai.Conversation,
) -> root.ai.ConversationToken throws baml.errors.UnknownError {
    root.google.internal._save_conversation(conversation, provider.name())
}

function _restore(
    provider: root.google.vertex.Gemini,
    token: root.ai.ConversationToken,
) -> root.ai.Conversation throws baml.errors.UnknownError {
    root.google.internal._restore_conversation(
        provider,
        _primitive(provider),
        provider.name(),
        provider.model,
        token,
    )
}

function _import(
    provider: root.google.vertex.Gemini,
    messages: root.ai.Messages,
) -> root.ai.ConversationImport throws baml.errors.UnknownError {
    root.google.internal._import_messages(
        provider,
        _primitive(provider),
        provider.name(),
        provider.model,
        messages,
    )
}
