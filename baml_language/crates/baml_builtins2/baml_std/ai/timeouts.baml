// The deadlines an LLM client runs one request attempt under. Every stdlib
// client constructor takes `timeout: baml.time.Duration | LlmTimeoutOptions |
// null`; a bare Duration is the end-to-end total.

/// The four deadlines of one LLM request attempt. Each retry or fallback
/// attempt starts its own clocks.
///
/// A zero or negative `Duration` means no limit, on any field.
///
/// ```baml
/// let client = openai.ChatClient.new(
///     model = "gpt-4o-mini",
///     timeout = ai.LlmTimeoutOptions {
///         timeout: baml.time.Duration.from_seconds(120),
///         connect_timeout: baml.time.Duration.from_seconds(10),
///         first_token_timeout: baml.time.Duration.from_seconds(30),
///         stream_token_timeout: baml.time.Duration.from_seconds(10),
///     },
/// );
/// ```
class LlmTimeoutOptions {
    /// End-to-end total for one attempt, connecting included. Forwarded to the
    /// underlying `baml.http.send` / `baml.http.send_sse`. `null` is 5 minutes.
    timeout: baml.time.Duration?,
    /// Limit on establishing the connection. `null` is 10 seconds.
    connect_timeout: baml.time.Duration?,

    /// Streaming only: limit on receiving the first non-empty content delta
    /// (text, reasoning or tool input). Starts when the request attempt is
    /// dispatched, so it includes connecting, sending the request and waiting
    /// for response headers. Keep-alives, response metadata and empty deltas
    /// do not end it. `null` is not enforced.
    first_token_timeout: baml.time.Duration?,

    /// Streaming only: limit on the time between non-empty content deltas.
    /// Starts once the first non-empty content delta has arrived, and only
    /// another one resets it. `null` is not enforced.
    stream_token_timeout: baml.time.Duration?,

    /// (internal) A client's `timeout` argument as options: a bare `Duration`
    /// is the end-to-end total, and `null` leaves every deadline at its
    /// default.
    function _resolve(
        timeout: baml.time.Duration | LlmTimeoutOptions | null,
    ) -> LlmTimeoutOptions throws never {
        match (timeout) {
            let options: LlmTimeoutOptions => options,
            let total: baml.time.Duration => {
                LlmTimeoutOptions {
                    timeout: total,
                    connect_timeout: null,
                    first_token_timeout: null,
                    stream_token_timeout: null,
                }
            },
            null => {
                LlmTimeoutOptions {
                    timeout: null,
                    connect_timeout: null,
                    first_token_timeout: null,
                    stream_token_timeout: null,
                }
            },
        }
    }

    /// (internal) Puts the two transport deadlines on `request`, which
    /// `baml.http` then enforces, and returns it. The two token deadlines are
    /// enforced by `ai.stream.TurnStream`.
    function _apply(self, request: baml.http.Request) -> baml.http.Request throws never {
        request.timeout = self.timeout;
        request.connect_timeout = self.connect_timeout;
        request
    }
}
