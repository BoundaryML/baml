---
title: "sap.ParseCache"
description: "Class sap.ParseCache from the generated baml package reference."
---

Cached type and assertion data used by incremental schema-aligned parsing.

```baml
class sap.ParseCache<TStream, TFinal>
```

## Fields

### _data

```baml
_data: $rust_type
```

No description is available yet.

## Methods

### new

```baml
function new(t_stream: reflect.Type, t_final: reflect.Type) -> baml.sap.ParseCache<TStream, TFinal>
```

Internal constructor. Callers normally use `__new_parse_cache` so the
type arguments determine both runtime type descriptors.

_Source: `<builtin>/baml/ns_sap/sap.baml:375`_
