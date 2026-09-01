---
title: "id.current"
description: "Function id.current from the generated baml package reference."
---

Returns the current BEX runtime ID for this function invocation: the
override set via `baml.id.set` / `$id = ...` if one is active, otherwise
the call's default `CallRef`. Returns an empty string when no BEX
function is running (e.g. during `$init`).

```baml
function id.current() -> string
```

_Source: `<builtin>/baml/ns_id/id.baml:277`_
