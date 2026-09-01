---
title: "net.TcpStream"
description: "Class net.TcpStream from the generated baml package reference."
---

A TCP connection. Created by `baml.net.TcpStream.connect` or returned by
`baml.net.TcpListener.accept`. Modeled on Rust's `std::net::TcpStream`.

```baml
class net.TcpStream
```

## Fields

### _handle

```baml
_handle: $rust_type
```

No description is available yet.

## Methods

### _connect

```baml
function _connect(addr: string, timeout_nanos: bigint) -> baml.net.TcpStream throws baml.errors.Io | baml.errors.Timeout
```

No description is available yet.

### close

```baml
function close(self: baml.net.TcpStream) -> null throws baml.errors.Io
```

Closes the connection.

### connect

```baml
function connect(addr: string, timeout: baml.time.Duration | null) -> baml.net.TcpStream throws baml.errors.Io | baml.errors.Timeout
```

Opens a TCP connection to `addr` (e.g. `"127.0.0.1:8080"`) and returns the stream.

`timeout` bounds how long to wait for the connection to be established;
`null` (the default) waits indefinitely (subject to the OS default). On
expiry this throws `root.errors.Timeout`.

_Source: `<builtin>/baml/ns_net/net.baml:627`_
