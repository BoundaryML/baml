// Internal response-header readers behind `ai.errors.classify_http`. Nothing
// here is part of the public ai surface.

// One header by name, tolerating the two casings a response map can carry:
// the HTTP layer lowercases response header names, but a hand-built map (a
// mock, a re-wrapped error) may keep the wire's canonical casing.
function _header(headers: map<string, string>, lower: string, canonical: string) -> string? {
    headers.get(lower) ?? headers.get(canonical)
}

// A non-negative integer header value, or null when it is absent, blank, or
// not an integer (`Retry-After` may also be an HTTP-date, which is not parsed).
function _int_header(headers: map<string, string>, lower: string, canonical: string) -> int? {
    if let raw: string = _header(headers, lower, canonical) {
        let parsed: int? = baml.Int.parse(raw.trim()) catch (e) {
            baml.errors.ParseError => null,
        };
        if let value: int = parsed {
            if (value >= 0) {
                return value;
            }
        }
    }
    null
}

// How long the provider asked us to wait, in milliseconds, from the response
// headers of a 429. `retry-after-ms` (OpenAI/Anthropic rate-limit headers) wins
// over `Retry-After` delta-seconds (RFC 9110); an HTTP-date `Retry-After` and a
// missing header both yield null, which is the caller's cue to use its own
// backoff.
function _retry_after_ms(headers: map<string, string>?) -> int? {
    if let h: map<string, string> = headers {
        if let ms: int = _int_header(h, "retry-after-ms", "Retry-After-Ms") {
            return ms;
        }
        if let seconds: int = _int_header(h, "retry-after", "Retry-After") {
            return seconds * 1000;
        }
    }
    null
}
