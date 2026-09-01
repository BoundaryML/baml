---
title: "http.Response$stream"
description: "Class http.Response$stream from the generated baml package reference."
---

An HTTP response returned by `baml.http.fetch` or `baml.http.send`.

```baml
class http.Response$stream
```

## Fields

### status_code

```baml
status_code: int | null
```

The [HTTP response status code](https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Status)

### headers

```baml
headers: map<string, string>
```

The [HTTP response headers](https://developer.mozilla.org/en-US/docs/Glossary/Response_header)

### url

```baml
url: string | null
```

The URL the response came from.

### _body

```baml
_body: $rust_type
```

No description is available yet.

_Source: `<builtin>/baml/ns_http/http.baml:0`_
