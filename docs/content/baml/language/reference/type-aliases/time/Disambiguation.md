---
title: "time.Disambiguation"
description: "Type alias time.Disambiguation from the generated baml package reference."
---

DST gap/overlap resolution, following TC39 Temporal semantics:
- `"compatible"` (default): later for gaps, earlier for overlaps
- `"earlier"` / `"later"`: always pick that side
- `"reject"`: throw `AmbiguousTimeError`

```baml
type time.Disambiguation = "compatible" | "earlier" | "later" | "reject"
```

_Source: `<builtin>/baml/ns_time/timezone.baml:234`_
