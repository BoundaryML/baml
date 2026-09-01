---
title: "http.send"
description: "Function http.send from the generated baml package reference."
---

Sends an HTTP request and returns the response.

`timeout` is the total deadline for the request, as in `fetch`. `null` (the
default) imposes no deadline.

```baml
function http.send(request: baml.http.Request, timeout: baml.time.Duration | null) -> baml.http.Response throws baml.errors.Io | baml.errors.Timeout
```

_Source: `<builtin>/baml/ns_http/http.baml:6498`_
