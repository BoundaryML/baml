---
title: "http.fetch_sse"
description: "Function http.fetch_sse from the generated baml package reference."
---

Sends an HTTP request and returns a Server-Sent Events stream for streaming responses.

On native transports, `timeout` covers the request and streaming body.
`first_event_timeout` starts after open and ends on the first parsed event.
Null/non-positive is unbounded; browser/Wasm does not enforce these fields.

```baml
function http.fetch_sse(request: baml.http.Request, timeout: baml.time.Duration | null, first_event_timeout: baml.time.Duration | null) -> baml.http.SseStream throws baml.errors.Io | baml.errors.Timeout
```

_Source: `<builtin>/baml/ns_http/http.baml:7163`_
