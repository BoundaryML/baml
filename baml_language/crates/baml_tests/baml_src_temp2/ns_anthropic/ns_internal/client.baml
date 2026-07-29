function _primitive_headers(
    provider: root.anthropic.AnthropicProvider,
) -> map<string, string> throws never {
    let headers: map<string, string> = {};
    match (provider.extra_headers) {
        let extra: map<string, string> => {
            for (let key in extra.keys()) {
                headers.set(key, extra[key]);
            }
        },
        null => {},
    }
    // The typed provider field owns the API version.
    headers.set("anthropic-version", provider.anthropic_version);
    headers
}

function _extra_body(
    provider: root.anthropic.AnthropicProvider,
) -> map<string, unknown> throws never {
    let body: map<string, unknown> = {};
    match (provider.extra_body) {
        let extra: map<string, unknown> => {
            for (let key in extra.keys()) {
                // These fields are generated from the provider, task, or
                // conversation and cannot be replaced through extra_body.
                if (
                    key != "model"
                        && key != "system"
                        && key != "messages"
                        && key != "max_tokens"
                        && key != "output_config"
                        && key != "tools"
                        && key != "tool_choice"
                        && key != "stream"
                ) {
                    body.set(key, extra[key]);
                }
            }
        },
        null => {},
    }
    body
}

function _configured_primitive(
    provider: root.anthropic.AnthropicProvider,
) -> baml.llm.PrimitiveClient throws baml.errors.UnknownError {
    let base = baml.llm.from_shorthand(provider.render_shorthand()) catch (e) {
        _ => {
            throw baml.errors.UnknownError.with_message<never>(
                e,
                "Anthropic provider shorthand failed",
            )
        },
    };
    baml.llm.PrimitiveClient {
        ...base,
        options: baml.llm.PrimitiveClientOptions {
            ...base.options,
            model: provider.model,
            api_key: provider.api_key,
            base_url: provider.base_url,
            provider_options: baml.llm.AnthropicOptions {
                max_tokens: provider.max_tokens,
            },
            headers: _primitive_headers(provider),
            request_body: _extra_body(provider),
        },
    }
}

function _messages_built_request<T>(
    provider: root.anthropic.AnthropicProvider,
    task: root.ai.Task<T>,
    log_shape: bool = true,
) -> baml.http.Request throws root.ai.Failure | baml.errors.UnknownError {
    _validate_output_config(provider);
    let primitive = _configured_primitive(provider);
    let specialized = primitive.specialize_prompt(task.prompt) catch (e) {
        _ => {
            throw baml.errors.UnknownError.with_message<never>(
                e,
                "Anthropic prompt specialization failed",
            )
        },
    };
    let built = primitive.build_request(specialized, task.output_type()) catch (e) {
        _ => {
            throw baml.errors.UnknownError.with_message<never>(
                e,
                "Anthropic request build failed",
            )
        },
    };
    let parsed = baml.json.parse(built.body) catch (e) {
        _ => {
            throw baml.errors.UnknownError.with_message<never>(
                e,
                "Anthropic request body was not JSON",
            )
        },
    };
    let body = match (parsed) {
        let object: map<string, json> => object,
        _ => {
            throw baml.errors.UnknownError.with_message<never>(
                parsed,
                "Anthropic request body was not an object",
            )
        },
    };
    // Provider-owned fields win over extra_body.
    body.set("model", provider.model);
    body.set("max_tokens", provider.max_tokens);
    if (_uses_sap(provider)) {
        body.delete("output_config");
    } else {
        body.set(
            "output_config",
            {
                "format": {
                    "type": "json_schema",
                    "schema": _output_schema(task.output_type()),
                },
            },
        );
    }
    if (log_shape) {
        log.info(
            baml.json.stringify({
                "event": "llm.request.shape",
                "provider": "anthropic",
                "model": provider.model,
                "message_count": baml.json.path_or<json[]>(body, ".messages", []).length(),
                "output_mode": provider.output_mode.to_string(),
                "tool_mode": provider.tool_mode.to_string(),
                "has_output_format": body.has("output_config"),
            }),
        );
    }
    baml.http.Request {
        method: built.method,
        url: built.url,
        headers: built.headers,
        body: baml.json.stringify(body),
    }
}

function _messages_generate<T>(
    provider: root.anthropic.AnthropicProvider,
    task: root.ai.Task<T>,
) -> root.ai.ResponseWithMetadata<T> throws root.ai.Failure | baml.errors.UnknownError {
    let request = _messages_built_request(provider, task);
    log.info(
        baml.json.stringify({
            "event": "llm.request",
            "provider": "anthropic",
            "model": provider.model,
            "request_bytes": request.body.length(),
            "output_mode": provider.output_mode.to_string(),
        }),
    );
    let response = baml.http.send(request) catch (e) {
        _ => {
            throw root.ai.NetworkFailure {
                provider: "anthropic",
                detail: "baml.http.send failed before an Anthropic Messages response arrived",
            }
        },
    };
    let text = response.text() catch (e) {
        _ => {
            throw root.ai.NetworkFailure {
                provider: "anthropic",
                detail: "the Anthropic Messages body could not be read completely",
            }
        },
    };
    log.info(
        baml.json.stringify({
            "event": "llm.response",
            "provider": "anthropic",
            "model": provider.model,
            "status_code": response.status_code,
            "response_bytes": text.length(),
        }),
    );
    if (!response.ok()) {
        throw root.ai.internal.classify_http(
            "anthropic",
            response.status_code,
            text,
        );
    }
    let raw = baml.json.parse(text) catch (e) {
        _ => {
            throw root.ai.ParseFailed {
                provider: "anthropic",
                raw_output: text,
            }
        },
    };
    root.ai.ResponseWithMetadata<T> {
        value: _parse_response_value<T>(raw, text, _uses_sap(provider)),
        metadata: _response_metadata(raw, provider.model),
        conversation: null,
    }
}

function _messages_stream<TPartial, T>(
    provider: root.anthropic.AnthropicProvider,
    task: root.ai.Task<T>,
) -> baml.llm.Stream<TPartial, T> throws baml.errors.UnknownError {
    let primitive = _configured_primitive(provider);
    let specialized = primitive.specialize_prompt(task.prompt) catch (e) {
        _ => {
            throw baml.errors.UnknownError.with_message<never>(
                e,
                "Anthropic stream prompt failed",
            )
        },
    };
    let built = primitive.build_request_stream(specialized, task.output_type()) catch (e) {
        _ => {
            throw baml.errors.UnknownError.with_message<never>(
                e,
                "Anthropic stream request failed",
            )
        },
    };
    let parsed = baml.json.parse(built.body) catch (e) {
        _ => {
            throw baml.errors.UnknownError.with_message<never>(
                e,
                "Anthropic stream request body was not JSON",
            )
        },
    };
    let body = match (parsed) {
        let object: map<string, json> => object,
        _ => {
            throw baml.errors.UnknownError.with_message<never>(
                parsed,
                "Anthropic stream request body was not an object",
            )
        },
    };
    if (_uses_sap(provider)) {
        body.delete("output_config");
    } else {
        let schema = _output_schema(task.output_type()) catch (e) {
            _ => {
                throw baml.errors.UnknownError.with_message<never>(
                    e,
                    "Anthropic strict stream output schema is unsupported",
                )
            },
        };
        body.set(
            "output_config",
            {
                "format": {
                    "type": "json_schema",
                    "schema": schema,
                },
            },
        );
    }
    let request = baml.http.Request {
        method: built.method,
        url: built.url,
        headers: built.headers,
        body: baml.json.stringify(body),
    };
    log.info(
        baml.json.stringify({
            "event": "llm.stream.request",
            "provider": "anthropic",
            "method": request.method,
            "url": request.url,
            "request_bytes": request.body.length(),
        }),
    );
    let sse = baml.http.fetch_sse(request) catch (e) {
        _ => {
            throw baml.errors.UnknownError.with_message<never>(
                e,
                "Anthropic stream open failed",
            )
        },
    };
    let accumulator = primitive.new_stream_accumulator() catch (e) {
        _ => {
            throw baml.errors.UnknownError.with_message<never>(
                e,
                "Anthropic stream accumulator failed",
            )
        },
    };
    baml.llm.Stream<TPartial, T> {
        _client: primitive,
        _acc: accumulator,
        _sse: sse,
        _cache: baml.llm.__new_stream_cache<TPartial, T>(),
    }
}
