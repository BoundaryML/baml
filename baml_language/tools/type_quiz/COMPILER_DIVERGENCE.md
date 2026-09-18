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

## CD-003: a `let` annotated with a union that nests a union is checked backwards

Spec, "BAML Subtyping Cases": "unions: `T <: (T | ...)` for all `T`", and
unions are associative, so `A | (B | C)` is `A | B | C` and holds no `D`.
Spec, "Implementation & Design Guidelines": "runtime values may never violate
their compile-time type contracts."

Compiler (canary `71dfab507`, 2026-09-18):

```baml
function through(right: bool | string | bigint) -> bool | string | int {
    let left: bool | (string | int) = right;
    left
}
```

compiles, and `through(5n)` returns a value that reflects as `bigint` in a
slot typed `bool | string | int`; an exhaustive `match` over `bool`, `string`
and `int` then takes the `int` arm. Written flat, `let left: bool | string |
int = right` is rejected with E0001 as it should be.

The trigger is exact: the annotation of a `let` is a union at the top level
with a PARENTHESISED UNION among its members — `bool | (string | int)`,
`(bool | string) | int`, `(bool | string | int) | null`. It is not triggered
by a union parenthesised whole (`(string | int)`), by a parenthesised member
that is not a union (`bool | (string)`), or by the nesting sitting inside an
array element or a generic argument; and the same type spelled as a
parameter, a field, a return type or an array's element type is checked
correctly. Where the initializer shares no member with the annotation the
binding IS rejected, but with the check visibly reversed — `expected
`bigint`, found `int | string | bool``, the span on the annotation rather
than the initializer — which is what a refutable pattern tested against the
initializer would report. Where they share a member, as above, nothing is
reported at all.

Found by the near miss `union_regrouped_differs`, the first fact to put a
nested union that is NOT an equivalence at a binding: `union_associates` had
been putting the same spelling there all along, and an equivalence compiles
either way.

Contradiction: `Accepts`.

Blocks: item `unions/nested-in-let-annotation`. Holds every pair that would
spell such an annotation out of the `let` site (`root.algebra.sites_for`).
