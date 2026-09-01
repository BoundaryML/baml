---
title: "http.SseStream"
description: "Class http.SseStream from the generated baml package reference."
---

A Server-Sent Events (SSE) stream. Created by `baml.http.fetch_sse`.

```baml
class http.SseStream
```

## Fields

### url

```baml
url: string
```

No description is available yet.

### status_code

```baml
status_code: int
```

Status code of the opening response. `fetch_sse` throws on non-2xx,
so this is always a success status.

### headers

```baml
headers: map<string, string>
```

Response headers at connection open — the provider request id
(`x-request-id`, `request-id`, ...) lives here.

### _handle

```baml
_handle: $rust_type
```

No description is available yet.

## Methods

### close

```baml
function close(self: baml.http.SseStream) -> null
```

Closes the SSE stream and releases the underlying connection.

### next

```baml
function next(self: baml.http.SseStream) -> string | null throws baml.errors.Io | baml.errors.Timeout
```

Only returns `null` if the stream is done/closed.
Otherwise, waits until a new event is available.

_Source: `<builtin>/baml/ns_http/http.baml:4440`_
