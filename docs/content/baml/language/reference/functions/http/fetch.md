---
title: "http.fetch"
description: "Function http.fetch from the generated baml package reference."
---

Sends a GET request to `url` and returns the response.

`timeout` is the total deadline for the request (connection + response
headers + body). `null` (the default) imposes no deadline. On expiry this
throws `root.errors.Timeout`.

```baml
function http.fetch(url: string, timeout: baml.time.Duration | null) -> baml.http.Response throws baml.errors.Io | baml.errors.Timeout
```

_Source: `<builtin>/baml/ns_http/http.baml:5844`_
