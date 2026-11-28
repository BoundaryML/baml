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

## CD-003 (resolved): nested unions in annotated let bindings

Issue #5652 fixed assignment checking for nested union annotations. Given
`right: bool | string | bigint`, both `let left: bool | (string | int) = right`
and its flat spelling now produce E0001 because the destination excludes
`bigint`. Previously the grouped annotation accepted this value.

The quiz retains `unions/nested-in-let-annotation` as a verified regression
case. Both examples require rejection and are excluded from learner sessions.
The generator can draw annotated let bindings for nested unions in
either direction.
