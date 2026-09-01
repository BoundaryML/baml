---
title: "ws.WsStream"
description: "Class ws.WsStream from the generated baml package reference."
---

A WebSocket connection.

```baml
class ws.WsStream
```

## Fields

### _handle

```baml
_handle: $rust_type
```

No description is available yet.

## Methods

### close

```baml
function close(self: baml.ws.WsStream) -> null
```

Close the connection.

### next

```baml
function next(self: baml.ws.WsStream) -> string | null throws baml.errors.Io
```

Receive the next text frame, or `null` after the connection closes.

### send

```baml
function send(self: baml.ws.WsStream, text: string) -> null throws baml.errors.Io
```

Send a text frame.

_Source: `<builtin>/baml/ns_ws/ws.baml:103`_
