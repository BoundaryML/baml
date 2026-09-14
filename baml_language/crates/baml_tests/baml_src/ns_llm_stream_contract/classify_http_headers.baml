// `ai.errors.classify_http`'s optional `headers` argument — the other half of
// the client-contract fixes in this namespace.
//
// `RateLimited.retry_after_ms` used to be structurally unfillable: the
// classifier only received (status, body), so every 429 reported "no idea when
// to retry" even when the provider had said so in a header. `classify_http`
// now takes the response headers as a defaulted 4th parameter, and
// `ai.wire.send_as` passes `resp.headers`.

// A classified failure rendered as a comparable label.
function retry_label(error: unknown) -> string {
    match (error) {
        let limited: ai.errors.RateLimited => {
            [
                "RateLimited",
                limited.provider,
                `retry_after_ms=${limited.retry_after_ms ?? -1}`,
            ].join("|")
        },
        let network: ai.errors.NetworkFailure => `NetworkFailure|${network.detail}`,
        let invalid: ai.errors.InvalidRequest => `InvalidRequest|${invalid.detail}`,
        _ => `other:${error.to_string()}`,
    }
}

// Classify a 429 carrying `headers`, and label the result.
function classify_429(headers: map<string, string>) -> string {
    retry_label(ai.errors.classify_http("mockprovider", 429, "{}", headers = headers))
}

function classify_http_retry_after_seconds() -> string {
    classify_429({ "retry-after": "3" })
}

test "classify_http_retry_after_seconds" {
    // RFC 9110 delta-seconds -> milliseconds
    assert.equal(classify_http_retry_after_seconds(), "RateLimited|mockprovider|retry_after_ms=3000")
}

function classify_http_retry_after_ms_wins() -> string {
    classify_429({ "retry-after": "3", "retry-after-ms": "1500" })
}

test "classify_http_retry_after_ms_wins" {
    // the millisecond header is the more precise one, so it wins
    assert.equal(classify_http_retry_after_ms_wins(), "RateLimited|mockprovider|retry_after_ms=1500")
}

function classify_http_retry_after_canonical_case() -> string {
    classify_429({ "Retry-After": "2" })
}

test "classify_http_retry_after_canonical_case" {
    assert.equal(classify_http_retry_after_canonical_case(), "RateLimited|mockprovider|retry_after_ms=2000")
}

function classify_http_retry_after_http_date_is_null() -> string {
    classify_429({ "retry-after": "Wed, 21 Oct 2026 07:28:00 GMT" })
}

test "classify_http_retry_after_http_date_is_null" {
    // an HTTP-date Retry-After is not parsed; null means "use your own backoff"
    assert.equal(classify_http_retry_after_http_date_is_null(), "RateLimited|mockprovider|retry_after_ms=-1")
}

function classify_http_no_headers_argument() -> string {
    // the pre-existing 3-argument call still compiles and still classifies
    retry_label(ai.errors.classify_http("mockprovider", 429, "{}"))
}

test "classify_http_no_headers_argument" {
    assert.equal(classify_http_no_headers_argument(), "RateLimited|mockprovider|retry_after_ms=-1")
}

function classify_http_non_429_unchanged() -> string {
    let headers: map<string, string> = { "retry-after": "3" };
    [
        retry_label(ai.errors.classify_http("mockprovider", 503, "boom", headers = headers)),
        retry_label(ai.errors.classify_http("mockprovider", 400, "bad", headers = headers)),
    ].join(" / ")
}

test "classify_http_non_429_unchanged" {
    assert.equal(
        classify_http_non_429_unchanged(),
        "NetworkFailure|http 503 / InvalidRequest|http 400",
    )
}

// End to end through the real transport helper: a mock 429 whose Retry-After
// header reaches the caller's RateLimited.
class RetryEcho {
    ok: bool?,
}

function send_as_429(base_url: string) -> string {
    let req = baml.http.Request {
        method: "POST",
        url: base_url + "/v1/messages",
        headers: { "content-type": "application/json" },
        body: "{}",
    };
    let decoded = ai.wire.send_as<RetryEcho>(req, "mockprovider");
    `unexpected-success:${decoded.ok ?? false}`
}

function classify_http_send_as_plumbs_headers() -> string {
    root.llm_mock.mock_json_serve(
        [
            root.llm_mock.MockResponse.with_headers(
                429,
                `{"error":"slow down"}`,
                { "content-type": "application/json", "retry-after": "5" },
            ),
        ],
        (mock: root.llm_mock.MockJsonServer) -> {
            send_as_429(mock.base_url()) catch_all (e) {
                _ => retry_label(e),
            }
        },
    )
}

test "classify_http_send_as_plumbs_headers" {
    assert.equal(
        classify_http_send_as_plumbs_headers(),
        "RateLimited|mockprovider|retry_after_ms=5000",
    )
}
