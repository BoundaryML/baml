---
title: "ws.connect"
description: "Function ws.connect from the generated baml package reference."
---

Open a WebSocket connection with additional request headers.

```baml
function ws.connect(url: string, headers: map<string, string>, timeout: baml.time.Duration) -> baml.ws.WsStream throws baml.errors.Io | baml.errors.Timeout
```

_Source: `<builtin>/baml/ns_ws/ws.baml:622`_
