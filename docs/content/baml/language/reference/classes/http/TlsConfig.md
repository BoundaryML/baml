---
title: "http.TlsConfig"
description: "Class http.TlsConfig from the generated baml package reference."
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
class http.TlsConfig
```

## Fields

### allow_tls1_2

```baml
allow_tls1_2: bool
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
_handshake_timeout_nanos: bigint
```

No description is available yet.

## Methods

### _new

```baml
function _new(cert_pem: uint8array, key_pem: uint8array, allow_tls1_2: bool, handshake_timeout_nanos: bigint) -> baml.http.TlsConfig throws baml.errors.Io
```

No description is available yet.

### new

```baml
function new(cert_pem: uint8array, key_pem: uint8array, allow_tls1_2: bool, handshake_timeout: baml.time.Duration) -> baml.http.TlsConfig throws baml.errors.Io
```

Parses a PEM-encoded certificate chain and private key into a TLS configuration.

### Parameters
- `cert_pem`: PEM-encoded certificate chain (leaf certificate first).
- `key_pem`: PEM-encoded private key (PKCS#8, PKCS#1/RSA, or SEC1/EC).
- `handshake_timeout`: How long a TLS handshake may take before the
  connection is dropped (default 10s), bounding a client that opens a
  socket then stalls mid-handshake. A non-positive duration disables it.

_Source: `<builtin>/baml/ns_http/server.baml:5157`_
