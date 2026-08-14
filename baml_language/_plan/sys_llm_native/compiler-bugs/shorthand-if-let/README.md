# Property-shorthand rewrite misses `if let` binders (post-hir_ty merge)

`{ "key": key }` — a QUOTED map key whose text equals the value identifier —
errors with E0003 "property shorthand `key` requires an in-scope value named
`key`" when (and only when) the value identifier is bound by an enclosing
`if let`. Renaming the binder (function `g` in repro.baml) compiles fine, so
the entry is being rewritten through the property-shorthand path despite the
key being quoted, and that path's resolution does not see `if let` binders.

Found: 2026-08-13, first check after merging a0f4605e8 (hir_ty S0-S5).
Workaround applied: google/ns_internal/vertex.baml vertex_url binder renamed.
Repro: `baml-cli check` in a project containing repro.baml — `f` errors, `g` ok.
