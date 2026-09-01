---
title: "http.Server"
description: "Class http.Server from the generated baml package reference."
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
class http.Server
```

## Fields

### addr

```baml
addr: string
```

The resolved local address the listener is bound to, e.g.
`"127.0.0.1:54321"`. Use it to discover the port after binding `":0"`.

### _state

```baml
_state: $rust_type
```

No description is available yet.

## Methods

### _serve

```baml
function _serve(self: baml.http.Server, handler: (req: baml.http.Request) -> baml.http.Response throws never, tls_config: baml.http.TlsConfig | null, allow_http1: bool, allow_http2: bool, max_body_size: int, max_connections: int, header_read_timeout_nanos: bigint) -> never throws baml.errors.Io
```

No description is available yet.

### bind

```baml
function bind(addr: string) -> baml.http.Server throws baml.errors.Io
```

Binds a TCP listener on `addr` and returns a `Server` ready to `serve`.
`":0"` (e.g. `"127.0.0.1:0"`) asks the OS for an ephemeral port; the chosen
address is available as `.addr`. The socket starts queuing incoming
connections immediately, so a client may connect before `serve` is called.

### Parameters
- `addr`: The address to bind. E.g. `"127.0.0.1:8080"` or `"127.0.0.1:0"`.

### serve

```baml
function serve(self: baml.http.Server, handler: (req: baml.http.Request) -> baml.http.Response throws never, tls_config: baml.http.TlsConfig | null, allow_http1: bool, allow_http2: bool, max_body_size: int, max_connections: int, header_read_timeout: baml.time.Duration) -> never throws baml.errors.Io
```

Serves requests with `handler`, blocking the current thread until it is
cancelled. To run it in the background, use `spawn { server.serve(...) }`.

Each incoming request is dispatched on its own BAML thread, so requests are
handled concurrently (including multiplexed HTTP/2 streams on one
connection). Each request is isolated: if the `handler` panics or its
response can't be written (e.g. a client that hung up early), only that
request fails — the client receives a `500` and the server keeps serving.

A `Server` serves one handler at a time: calling `serve` while it is already
serving raises `Io`. Once a serve ends (its thread is cancelled), the same
`Server` may be served again — with a different `handler`, `tls_config`, or
protocol set if desired. The bound port is held until the `Server` itself is
dropped, not when a serve ends.

### Parameters
- `handler`: Turns each incoming `Request` into a `Response`.
- `tls_config`: Optional TLS configuration. If provided, the server behaves as an HTTPS server.
- `allow_http1` / `allow_http2`: Which HTTP versions to accept (at least one must be true).
- `max_body_size`: Maximum request body size, in bytes, buffered per request
  (default 100 MiB). A larger body is rejected with `413 Payload Too Large`,
  bounding per-request memory.
- `max_connections`: Maximum number of connections served concurrently
  (default 1024). At the cap the accept loop applies backpressure (new
  connections wait in the kernel backlog) rather than spawning unbounded tasks.
- `header_read_timeout`: How long a client has to send the complete request
  headers before the connection is closed (default 30s) — a Slowloris defense.
  A non-positive duration disables it. (HTTP/1 only; HTTP/2 frames its headers.)

### Panics
- Calling `serve` on an unsupported platform (e.g. in the browser) will panic.
- If the serving thread is cancelled, `serve` ends with a
  `baml.panics.Cancelled` panic and any in-flight connections are dropped.

_Source: `<builtin>/baml/ns_http/server.baml:393`_
