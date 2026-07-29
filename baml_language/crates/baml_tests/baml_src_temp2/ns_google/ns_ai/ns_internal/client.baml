function _merged_headers(provider: root.google.ai.Gemini) -> map<string, string>
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

function _merged_query(provider: root.google.ai.Gemini) -> map<string, string>
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
    provider: root.google.ai.Gemini,
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

function _primitive(provider: root.google.ai.Gemini) -> baml.llm.PrimitiveClient
    throws baml.errors.UnknownError {
    let base = baml.llm.from_shorthand(provider.render_shorthand()) catch (e) {
        _ => throw baml.errors.UnknownError.with_message<never>(
            e,
            "Google AI Gemini provider shorthand failed",
        ),
    };
    baml.llm.PrimitiveClient {
        ...base,
        options: baml.llm.PrimitiveClientOptions {
            ...base.options,
            model: provider.model,
            api_key: provider.api_key,
            base_url: provider.base_url,
            headers: _merged_headers(provider),
            query_params: _merged_query(provider),
            request_body: _request_body(provider, base.options.request_body),
            provider_options: baml.llm.GoogleAiOptions {
                enterprise: provider.enterprise,
                credentials: provider.credentials,
                credentials_content: provider.credentials_content,
                location: provider.location,
                project_id: provider.project_id,
            },
        },
    }
}

function _use_sap(provider: root.google.ai.Gemini) -> bool throws never {
    provider.output_mode == root.google.ai.OutputMode.Sap
}

function _validate(provider: root.google.ai.Gemini) -> null
    throws root.ai.InvalidRequest {
    if (
        provider.tool_mode == root.google.ai.ToolMode.Prompt
            && provider.output_mode != root.google.ai.OutputMode.Sap
    ) {
        throw root.ai.InvalidRequest {
            provider: provider.name(),
            status_code: null,
            detail: "`tool_mode = Prompt` requires `output_mode = Sap`",
        };
    }
    null
}

// Request-shape helper for tests and diagnostics. It builds and authenticates
// the complete request but does not send it.
function _build_request<T>(
    provider: root.google.ai.Gemini,
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

function _begin<T>(
    provider: root.google.ai.Gemini,
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
    provider: root.google.ai.Gemini,
    conversation: root.ai.Conversation,
) -> root.ai.ConversationToken throws baml.errors.UnknownError {
    root.google.internal._save_conversation(
        conversation,
        provider,
        provider.name(),
        _resume_identity(provider),
    )
}

function _restore(
    provider: root.google.ai.Gemini,
    token: root.ai.ConversationToken,
) -> root.ai.Conversation throws baml.errors.UnknownError {
    root.google.internal._restore_conversation(
        provider,
        _primitive(provider),
        provider.name(),
        provider.model,
        _resume_identity(provider),
        token,
    )
}

function _resume_identity(provider: root.google.ai.Gemini) -> string throws never {
    baml.json.stringify({
        "model": provider.model,
        "enterprise": provider.enterprise,
        "project_id": provider.project_id,
        "location": provider.location,
        "base_url": provider.base_url,
        "output_mode": provider.output_mode.to_string(),
        "tool_mode": provider.tool_mode.to_string(),
        "parallel_tool_calls": provider.parallel_tool_calls,
    })
}

function _import<T>(
    provider: root.google.ai.Gemini,
    messages: root.ai.Messages,
) -> root.ai.ConversationImport throws baml.errors.UnknownError {
    root.google.internal._import_messages<T>(
        provider,
        _primitive(provider),
        provider.name(),
        provider.model,
        messages,
    )
}
