# Compiler divergence

Places where the compiler contradicts `../../TYPE_SYSTEM.md`. The doc is
prescriptive, so each entry is a compiler defect until a human rules
otherwise. An item whose status is `CompilerDiverges` names its entry here,
and the conformance suite asserts the divergence still holds, so a compiler
that catches up fails the suite until the item is promoted and the entry is
closed.

Entry format: id, the spec text, what the compiler does (with the canary
commit it was observed on), the kind of contradiction the item declares
(`Accepts`: the compiler accepts what the spec rejects; `Rejects`: it rejects
what the spec accepts; `Misreports`: it rejects as the spec does but with
other codes or elsewhere), and the items it blocks.

## CD-001: unguarded alias cycles are accepted

Spec, "Productivity": "a fully unguarded cycle is uninhabited: `type B = B`
denotes `never` (and is a compile error, E0068)".

Compiler (canary `9184ee38e`, 2026-09-08): `type B = B;` and
`type A = B; type B = A;` compile cleanly under both `baml check` and
`reflect.Package.compile`, whether or not the alias is used as a parameter,
binding, or field type. No E0068 is emitted.

Contradiction: `Accepts`.

Blocks: item `aliases/unguarded-cycle`.

## CD-002: function types are rejected as implementation targets

Spec, "Concrete Types": concrete types "include all primitives, all class
types, all enum types, all function types, and a few special built-in types".
Spec, "Interfaces": "Only concrete types may implement interfaces."

Compiler (canary `0488221d0`, 2026-09-10):
`implement Marker for (int) -> int throws never {}` is rejected with
`E0138: cannot implement an interface for (int) -> int throws never — the
target must be a single concrete type`. Every other concrete target in the
same sweep is accepted: `int`, `string`, `int[]`, `map<string, int>`, an enum,
and an alias of `int`. The non-concrete targets are rejected with the same
code, correctly: an existential, a union, `int?`, and `unknown`.

Contradiction: `Rejects`.

Blocks: the concrete half of the implementation-target pool, which omits
function types until this is settled.
