---
title: "http.Server$stream"
description: "Class http.Server$stream from the generated baml package reference."
---

A minimal HTTP/HTTPS server.

Bind a listening socket with `Server.bind`, then call `serve` with a request
handler to start serving:
```baml
let server = baml.http.Server.bind("127.0.0.1:0");
spawn { server.serve((req) -> { baml.http.Response.new(200, { }, "hi".to_utf8()) }) };
// server.addr now holds the actual bound address, e.g. "127.0.0.1:54321".
```

```baml
class http.Server$stream
```

## Fields

### addr

```baml
addr: string | null
```

The resolved local address the listener is bound to, e.g.
`"127.0.0.1:54321"`. Use it to discover the port after binding `":0"`.

### _state

```baml
_state: $rust_type
```

No description is available yet.

_Source: `<builtin>/baml/ns_http/server.baml:0`_
