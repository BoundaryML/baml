---
title: "http.SseStream$stream"
description: "Class http.SseStream$stream from the generated baml package reference."
---

A Server-Sent Events (SSE) stream. Created by `baml.http.fetch_sse`.

```baml
class http.SseStream$stream
```

## Fields

### url

```baml
url: string | null
```

No description is available yet.

### status_code

```baml
status_code: int | null
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

_Source: `<builtin>/baml/ns_http/http.baml:0`_
