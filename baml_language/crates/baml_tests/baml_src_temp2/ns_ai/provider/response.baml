class Usage {
    input_tokens: int,
    output_tokens: int,
    cached_input_tokens: int?,
    reasoning_tokens: int?,
    cost_usd: float?,
}

class ResponseMetadata {
    provider: string,
    model: string?,
    request_id: string?,
    finish_reason: string?,
    usage: Usage?,
    attributes: map<string, unknown>,
    raw: json,
}

class ResponseWithMetadata<T> {
    value: T,
    meta: ResponseMetadata,
    conversation: Conversation?,
}
