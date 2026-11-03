// A class that holds `$rust_type` state is created only by its own functions.
// A class literal could only copy a handle out of another value, which would
// relabel what the handle means, so the literal itself is refused.

// A handle moved across classes would hand an SSE stream's functions a
// response's body.
function cross_class_handle() -> baml.http.SseStream {
    let response = baml.http.Response.new(200, {}, "body".to_utf8());
    baml.http.SseStream { url: response.url, status_code: 200, headers: response.headers, _handle: response._body }
}

// A spread is the same literal.
function spread_copy(limit: baml.spawn.Limit) -> baml.spawn.Limit throws never {
    baml.spawn.Limit { ...limit }
}

// A `@@rust_type` class is the same rule with no field to copy: only its own
// functions create one.
function tagged_literal() -> baml.hash.DefaultHasher throws never {
    baml.hash.DefaultHasher {}
}
