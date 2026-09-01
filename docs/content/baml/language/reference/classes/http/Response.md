---
title: "http.Response"
description: "Class http.Response from the generated baml package reference."
---

An HTTP response returned by `baml.http.fetch` or `baml.http.send`.

```baml
class http.Response
```

## Fields

### status_code

```baml
status_code: int
```

The [HTTP response status code](https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Status)

### headers

```baml
headers: map<string, string>
```

The [HTTP response headers](https://developer.mozilla.org/en-US/docs/Glossary/Response_header)

### url

```baml
url: string
```

The URL the response came from.

### _body

```baml
_body: $rust_type
```

No description is available yet.

## Methods

### bytes

```baml
function bytes(self: baml.http.Response) -> uint8array throws baml.errors.Io | baml.errors.Timeout
```

Returns the response body as raw bytes.
Throws `Timeout` if the request deadline elapses while reading the body,
or `Io` if the body was unavailable.

### end

```baml
function end(self: baml.http.Response) -> null throws baml.errors.Io
```

Ends a streaming response body (see `new_streaming`), signaling
end-of-stream to the client so the chunked response completes. Subsequent
`write` calls raise `Io`. A no-op on an already-ended response.

### new

```baml
function new(status_code: int, headers: map<string, string>, body: uint8array) -> baml.http.Response
```

Builds a response, typically to return from a `baml.http.Server` handler.

`body` is sent verbatim as the response body. `headers` should include a
`Content-Type` if the client needs one; `Content-Length` is set for you.

### new_streaming

```baml
function new_streaming(status_code: int, headers: map<string, string>) -> baml.http.Response
```

Builds a *streaming* response to return from an `http.Server` handler: the
status line and `headers` are sent immediately, then the body is written
incrementally with successive `write` calls and ended with `end`. The
response is framed with chunked transfer-encoding, so each `write` is
flushed to the client as its own chunk — a slow producer streams in real
time (e.g. for Server-Sent Events). Do not set `Content-Length`; it is
omitted for a streamed body.

Because hyper only reads the response body *after* the handler returns, the
writes must run after the handler hands the response back — typically from a
`spawn`ed task that captures it, mirroring hyper's channel-backed body:
```baml
let resp = baml.http.Response.new_streaming(200, { "content-type": "text/event-stream" });
spawn {
  resp.write("data: one\n\n".to_utf8());
  resp.write("data: two\n\n".to_utf8());
  resp.end();
};
resp
```

### ok

```baml
function ok(self: baml.http.Response) -> bool
```

Returns `true` if `status_code` is in the 200–299 range.

### text

```baml
function text(self: baml.http.Response) -> string throws baml.errors.Io | baml.errors.Timeout
```

Returns the response body as a string. Live client responses are decoded
lossily as UTF-8, replacing malformed bytes with U+FFFD. Buffered `Bytes`
responses use strict UTF-8 decoding.
Throws `Timeout` if the request deadline elapses while reading the body,
or `Io` if the body was unavailable or buffered bytes were not valid UTF-8.

### write

```baml
function write(self: baml.http.Response, data: uint8array) -> null throws baml.errors.Io
```

Writes one chunk to a streaming response body (see `new_streaming`),
flushing it to the client. Applies backpressure: `write` suspends until the
previous chunk has been accepted by the connection. Raises `Io` if the
response was not created with `new_streaming`, if it has been `end`ed, or
if the client has hung up.

_Source: `<builtin>/baml/ns_http/http.baml:630`_
