---
title: "http.Request$stream"
description: "Class http.Request$stream from the generated baml package reference."
---

An HTTP request — outgoing (built for `baml.http.send`/`fetch_sse`) or
incoming (passed to an `http.Server` handler).

For an incoming server request: `url` is the request-target as received
(origin-form, e.g. `/path?q=1`, on HTTP/1; the authority lives in the `host`
header), `body` is the bytes decoded lossily as UTF-8 (non-UTF-8 bytes become
U+FFFD), and a header sent multiple times has its values joined with `, `.

```baml
class http.Request$stream
```

## Fields

### method

```baml
method: string | null
```

No description is available yet.

### url

```baml
url: string | null
```

No description is available yet.

### headers

```baml
headers: map<string, string>
```

No description is available yet.

### body

```baml
body: string | null
```

No description is available yet.

_Source: `<builtin>/baml/ns_http/http.baml:0`_
