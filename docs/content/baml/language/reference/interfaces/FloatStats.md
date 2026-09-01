---
title: "FloatStats"
description: "Interface FloatStats from the generated baml package reference."
---

Statistical reductions for `float[]`.

`mean` and `median` are defined only for `float[]` (both return `float`),
so — unlike `Summable`, which also covers `int[]` — this interface has a
single implementor. It exists so the reductions attach as methods
(`xs.mean()`, `xs.median()`) on the structural `float[]` type; the backing
natives are the private `_mean_float` / `_median_float` helpers below.

```baml
interface FloatStats
```

## Required methods

### mean

```baml
function mean(self: Self) -> float throws baml.errors.InvalidArgument
```

No description is available yet.

### median

```baml
function median(self: Self) -> float throws baml.errors.InvalidArgument
```

No description is available yet.

_Source: `<builtin>/baml/containers.baml:25791`_
