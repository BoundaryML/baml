class Usage {
    input_tokens: int,
    output_tokens: int,
    cached_input_tokens: int?,
    reasoning_tokens: int?,

    /// A fresh all-zero record — the accumulator seed every runner needs.
    function zero() -> Usage throws never {
        Usage {
            input_tokens: 0,
            output_tokens: 0,
            cached_input_tokens: 0,
            reasoning_tokens: 0,
        }
    }

    /// Adds `next` into this record in place (statement-shaped, like every
    /// mutating method). The accumulation arithmetic custom runners
    /// previously had to hand-write.
    function add(self, next: Usage) -> null throws never {
        self.input_tokens = self.input_tokens + next.input_tokens;
        self.output_tokens = self.output_tokens + next.output_tokens;
        self.cached_input_tokens = (self.cached_input_tokens ?? 0) + (next.cached_input_tokens ?? 0);
        self.reasoning_tokens = (self.reasoning_tokens ?? 0) + (next.reasoning_tokens ?? 0);
        null
    }
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
    metadata: ResponseMetadata,
    conversation: Conversation?,
}
