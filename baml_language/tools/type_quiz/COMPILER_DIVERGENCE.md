# Compiler divergence

Places where the compiler contradicts `../../TYPE_SYSTEM.md`. The doc is
prescriptive, so each entry is a compiler defect until a human rules
otherwise. An item whose status is `CompilerDiverges` names its entry here,
and the conformance suite asserts the divergence still holds, so a compiler
that catches up fails the suite until the item is promoted and the entry is
closed.

Entry format: id, the spec text, what the compiler does (with the canary
commit it was observed on), and the items it blocks.

## CD-001: unguarded alias cycles are accepted

Spec, "Productivity": "a fully unguarded cycle is uninhabited: `type B = B`
denotes `never` (and is a compile error, E0068)".

Compiler (canary `9184ee38e`, 2026-09-08): `type B = B;` and
`type A = B; type B = A;` compile cleanly under both `baml check` and
`reflect.Package.compile`, whether or not the alias is used as a parameter,
binding, or field type. No E0068 is emitted.

Blocks: item `aliases/unguarded-cycle`.
