---
title: "time.Disambiguation$stream"
description: "Type alias time.Disambiguation$stream from the generated baml package reference."
---

DST gap/overlap resolution, following TC39 Temporal semantics:
- `"compatible"` (default): later for gaps, earlier for overlaps
- `"earlier"` / `"later"`: always pick that side
- `"reject"`: throw `AmbiguousTimeError`

```baml
type time.Disambiguation$stream = "compatible" | "earlier" | "later" | "reject"
```

_Source: `<builtin>/baml/ns_time/timezone.baml:0`_
