---
title: "sap.parse_type"
description: "Function sap.parse_type from the generated baml package reference."
---

Parse against a runtime `type` value when the schema is data rather than a
lexical type parameter. Narrow the returned `unknown` at the call site.

```baml
function sap.parse_type(t: reflect.Type, text: string) -> unknown throws baml.errors.ParseError
```

_Source: `<builtin>/baml/ns_sap/sap.baml:2106`_
