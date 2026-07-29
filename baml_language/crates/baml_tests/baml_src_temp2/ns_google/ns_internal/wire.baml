// Gemini response-envelope parsing. The adapter deliberately parses the raw
// response rather than PrimitiveClient.parse so function calls, safety blocks,
// split text parts, thought signatures, and provider metadata stay visible.

function _candidate(
    raw: json,
    raw_output: string,
    provider_name: string,
) -> json throws root.ai.ParseFailed {
    let candidates = baml.json.path_or<json[]>(raw, ".candidates", []);
    if (candidates.length() != 1) {
        throw root.ai.ParseFailed {
            provider: provider_name,
            raw_output: raw_output,
        };
    }
    candidates[0]
}

function _candidate_content(
    raw: json,
    raw_output: string,
    provider_name: string,
) -> json throws root.ai.ParseFailed {
    let candidate = _candidate(raw, raw_output, provider_name);
    baml.json.path<json>(candidate, ".content") catch (e) {
        _ => {
            throw root.ai.ParseFailed {
            provider: provider_name,
            raw_output: raw_output,
        }
        },
    }
}

function _finish_reason(raw: json) -> string? throws never {
    baml.json.path_or<string?>(raw, ".candidates[0].finishReason", null)
}

function _is_refusal_finish_reason(reason: string) -> bool throws never {
    reason == "SAFETY"
        || reason == "RECITATION"
        || reason == "BLOCKLIST"
        || reason == "PROHIBITED_CONTENT"
        || reason == "SPII"
        || reason == "IMAGE_SAFETY"
        || reason == "IMAGE_PROHIBITED_CONTENT"
}

function _refusal_reason(raw: json) -> string? throws never {
    let block_reason = baml.json.path_or<string?>(raw, ".promptFeedback.blockReason", null);
    match (block_reason) {
        let reason: string if reason.length() > 0 => {
            baml.json.path_or<string?>(raw, ".promptFeedback.blockReasonMessage", reason)
        },
        _ => {
            match (_finish_reason(raw)) {
                let reason: string if _is_refusal_finish_reason(reason) => reason,
                _ => null,
            }
        },
    }
}

function _validate_response(
    raw: json,
    raw_output: string,
    provider_name: string,
) -> null throws root.ai.Failure {
    match (_refusal_reason(raw)) {
        let reason: string => {
            throw root.ai.Refused {
            provider: provider_name,
            reason: reason,
        }
        },
        null => {},
    }
    match (_finish_reason(raw)) {
        "MAX_TOKENS" | "MALFORMED_FUNCTION_CALL" | "UNEXPECTED_TOOL_CALL" => {
            throw root.ai.ParseFailed {
                provider: provider_name,
                raw_output: raw_output,
            }
        },
        _ => {},
    }
    let candidates = baml.json.path_or<json[]>(raw, ".candidates", []);
    if (candidates.length() != 1) {
        throw root.ai.ParseFailed {
            provider: provider_name,
            raw_output: raw_output,
        };
    }
    null
}

function _response_text(raw: json) -> string? throws never {
    let pieces: string[] = [];
    for (let part in baml.json.path_or<json[]>(raw, ".candidates[0].content.parts", [])) {
        if (!baml.json.path_or(part, ".thought", false)) {
            match (baml.json.path_or<string?>(part, ".text", null)) {
                let text: string => pieces.push(text),
                null => {},
            }
        }
    }
    if (pieces.length() == 0) {
        null
    } else {
        pieces.join("")
    }
}

function _raw_function_calls(raw: json) -> json[] throws never {
    let calls: json[] = [];
    for (let part in baml.json.path_or<json[]>(raw, ".candidates[0].content.parts", [])) {
        match (baml.json.path_or<map<string, json>?>(part, ".functionCall", null)) {
            let call: map<string, json> => calls.push(call),
            null => {},
        }
    }
    calls
}

function _output_call_value(
    call: json,
    raw_output: string,
    provider_name: string,
) -> json throws root.ai.ParseFailed {
    if (baml.json.path_or(call, ".name", "") != _output_tool_name()) {
        throw root.ai.ParseFailed {
            provider: provider_name,
            raw_output: raw_output,
        };
    }
    baml.json.path<json>(call, ".args.value") catch (e) {
        _ => {
            throw root.ai.ParseFailed {
            provider: provider_name,
            raw_output: raw_output,
        }
        },
    }
}

function _parse_output_call<T>(
    call: json,
    raw_output: string,
    provider_name: string,
) -> T throws root.ai.ParseFailed {
    baml.json.from_json<T>(_output_call_value(call, raw_output, provider_name)) catch (e) {
        _ => {
            throw root.ai.ParseFailed {
            provider: provider_name,
            raw_output: raw_output,
        }
        },
    }
}

function _parse_generation_value<T>(
    raw: json,
    raw_output: string,
    provider_name: string,
    use_sap: bool,
) -> T throws root.ai.Failure {
    _validate_response(raw, raw_output, provider_name);
    let text = match (_response_text(raw)) {
        let value: string => value,
        null => {
            throw root.ai.ParseFailed {
            provider: provider_name,
            raw_output: raw_output,
        }
        },
    };
    if (use_sap) {
        return baml.sap.parse<T>(text) catch (e) {
            _ => {
                throw root.ai.ParseFailed {
                provider: provider_name,
                raw_output: text,
            }
            },
        };
    }
    let value_json = baml.json.parse(text) catch (e) {
        _ => {
            throw root.ai.ParseFailed {
            provider: provider_name,
            raw_output: text,
        }
        },
    };
    baml.json.from_json<T>(value_json) catch (e) {
        _ => {
            throw root.ai.ParseFailed {
            provider: provider_name,
            raw_output: text,
        }
        },
    }
}

function _response_metadata(
    raw: json,
    fallback_model: string,
    provider_name: string,
) -> root.ai.ResponseMetadata throws never {
    let usage_json = baml.json.path_or<json>(raw, ".usageMetadata", null);
    let attributes: map<string, unknown> = {};
    match (baml.json.path_or<map<string, json>?>(raw, ".promptFeedback", null)) {
        let feedback: map<string, json> => attributes.set("prompt_feedback", feedback),
        null => {},
    }
    match (baml.json.path_or<json[]?>(raw, ".candidates[0].safetyRatings", null)) {
        let ratings: json[] => attributes.set("safety_ratings", ratings),
        null => {},
    }
    root.ai.ResponseMetadata {
        provider: provider_name,
        model: baml.json.path_or<string?>(raw, ".modelVersion", fallback_model),
        request_id: baml.json.path_or<string?>(raw, ".responseId", null),
        finish_reason: _finish_reason(raw),
        usage: match (usage_json) {
            let usage: map<string, json> => {
                root.ai.Usage {
                    input_tokens: baml.json.path_or(usage, ".promptTokenCount", 0),
                    output_tokens: baml.json.path_or(usage, ".candidatesTokenCount", 0),
                    cached_input_tokens: baml.json.path_or<int?>(usage, ".cachedContentTokenCount", null),
                    reasoning_tokens: baml.json.path_or<int?>(usage, ".thoughtsTokenCount", null),
                    cost_usd: null,
                }
            },
            _ => null,
        },
        attributes: attributes,
        raw: raw,
    }
}
