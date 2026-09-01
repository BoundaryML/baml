---
title: "net.TcpListener$stream"
description: "Class net.TcpListener$stream from the generated baml package reference."
---

A bound TCP listener. Created by `baml.net.TcpListener.bind`. Calling `accept`
suspends the current BAML thread until an incoming connection arrives (or the
thread is cancelled). Modeled on Rust's `std::net::TcpListener`.

```baml
class net.TcpListener$stream
```

## Fields

### _handle

```baml
_handle: $rust_type
```

No description is available yet.

_Source: `<builtin>/baml/ns_net/net.baml:0`_
