---
title: "http.TlsConfig$stream"
description: "Class http.TlsConfig$stream from the generated baml package reference."
---

Used to convert an HTTP server into an HTTPS server for secure connections.

To force HTTP clients onto HTTPS, run two servers — a plaintext one on the
HTTP port whose handler redirects, and the TLS one on the HTTPS port:
```baml
let secure = http.Server.bind("0.0.0.0:443");
let upgrade = http.Server.bind("0.0.0.0:80");
spawn { upgrade.serve((req) -> {
    baml.http.Response.new(308, { "Location": "https://" + req.headers.get("host") + req.url }, "".to_utf8())
}) };
spawn { secure.serve(my_handler, tls_config = my_tls) };
```

```baml
class http.TlsConfig$stream
```

## Fields

### allow_tls1_2

```baml
allow_tls1_2: bool | null
```

Whether to allow connections using TLSv1.2.
This is enabled by default, but may be disabled to require TLSv1.3.

BAML's standard HTTPS server does not provide TLS/SSL versions lower than TLSv1.2 as they are insecure.

### _certificate

```baml
_certificate: $rust_type
```

No description is available yet.

### _private_key

```baml
_private_key: $rust_type
```

No description is available yet.

### _handshake_timeout_nanos

```baml
_handshake_timeout_nanos: bigint | null
```

No description is available yet.

_Source: `<builtin>/baml/ns_http/server.baml:0`_
