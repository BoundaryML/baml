---
title: "net.UdpSocket"
description: "Class net.UdpSocket from the generated baml package reference."
---

A UDP socket. Created by `baml.net.UdpSocket.bind`. UDP is connectionless and
message-oriented: every `send_to` / `recv_from` carries a single datagram.
Modeled on Rust's `std::net::UdpSocket`.

```baml
class net.UdpSocket
```

## Fields

### _handle

```baml
_handle: $rust_type
```

No description is available yet.

## Methods

### _recv_from

```baml
function _recv_from(self: baml.net.UdpSocket, timeout_nanos: bigint) -> baml.net.Datagram throws baml.errors.Io | baml.errors.Timeout
```

No description is available yet.

### _send_to

```baml
function _send_to(self: baml.net.UdpSocket, data: uint8array, addr: string, timeout_nanos: bigint) -> int throws baml.errors.Io | baml.errors.Timeout
```

No description is available yet.

### bind

```baml
function bind(addr: string) -> baml.net.UdpSocket throws baml.errors.Io
```

Binds to `addr` (e.g. `"0.0.0.0:0"` for an OS-assigned ephemeral port) and
returns the socket.

### close

```baml
function close(self: baml.net.UdpSocket) -> null throws baml.errors.Io
```

Closes the socket.

### recv_from

```baml
function recv_from(self: baml.net.UdpSocket, timeout: baml.time.Duration | null) -> baml.net.Datagram throws baml.errors.Io | baml.errors.Timeout
```

Receives a single datagram, returning its payload bytes together with the
address of the sender.

`timeout` bounds the wait for a datagram; `null` (the default) blocks
indefinitely. On expiry this throws `root.errors.Timeout` and the socket
remains usable. Mirrors `UdpSocket::set_read_timeout`.

### send_to

```baml
function send_to(self: baml.net.UdpSocket, data: uint8array, addr: string, timeout: baml.time.Duration | null) -> int throws baml.errors.Io | baml.errors.Timeout
```

Sends `data` as a single datagram to `addr`. Returns the number of bytes sent.

`timeout` bounds the send; `null` (the default) blocks indefinitely. On
expiry this throws `root.errors.Timeout`. Mirrors `UdpSocket::set_write_timeout`.

_Source: `<builtin>/baml/ns_net/net.baml:3220`_
