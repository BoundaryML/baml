---
title: "net.TcpListener"
description: "Class net.TcpListener from the generated baml package reference."
---

A bound TCP listener. Created by `baml.net.TcpListener.bind`. Calling `accept`
suspends the current BAML thread until an incoming connection arrives (or the
thread is cancelled). Modeled on Rust's `std::net::TcpListener`.

```baml
class net.TcpListener
```

## Fields

### _handle

```baml
_handle: $rust_type
```

No description is available yet.

## Methods

### accept

```baml
function accept(self: baml.net.TcpListener) -> baml.net.TcpStream throws baml.errors.Io
```

Waits for the next incoming connection and returns it as a `TcpStream`.

### bind

```baml
function bind(addr: string) -> baml.net.TcpListener throws baml.errors.Io
```

Binds to `addr` (e.g. `"127.0.0.1:8080"`) and returns a listener ready to
accept incoming connections.

### close

```baml
function close(self: baml.net.TcpListener) -> null throws baml.errors.Io
```

No description is available yet.

_Source: `<builtin>/baml/ns_net/net.baml:2477`_
