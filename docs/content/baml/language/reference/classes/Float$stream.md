---
title: "Float$stream"
description: "Class Float$stream from the generated baml package reference."
---

A 64-bit IEEE 754 floating-point number (f64).

Note: division by zero (`x / 0.0`) does not follow raw IEEE — it throws a
catchable `baml.panics.DivisionByZero` rather than yielding `±inf`/`NaN`,
matching integer division. `inf` and `NaN` still arise from `float.inf()` /
`float.nan()`, from overflow (e.g. `1e308 * 10.0` → `inf`), and from other
invalid operations (e.g. `(-1.0).sqrt()` → `NaN`).

```baml
class Float$stream
```

_Source: `<builtin>/baml/float.baml:0`_
