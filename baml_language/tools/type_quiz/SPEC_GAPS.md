# Spec gaps

Principles the quiz needed that `../../TYPE_SYSTEM.md` does not state, or
states too loosely to derive a case from. Every entry is a principle in the
doc's own register, never an example for its own sake. A rule with no section
to cite names an entry here instead, so a gap cannot exist without a record.

Entry format: id, kind (`omitted`, `under-specified`, `doc-vs-compiler`), the
principle, proposed wording, and what cites it.

## G-001: where subtyping is checked

Kind: omitted.

Principle: a value of static type `S` is accepted at a position that expects
type `T` exactly when `S <: T`. The positions are the initializer of an
annotated binding, a call argument against its parameter, a function body's
value against the declared return type, a field initializer against the
field's type, and an element against an array's element type. The spec
defines the subtyping relation in detail but never says where it is applied;
the variance example (`let b: (int | string)[] = a`) relies on the reader
already assuming assignment is a subtyping check.

Proposed wording, for the start of "Subtyping Rules": "Subtyping is the
compatibility relation the compiler checks wherever a value meets an expected
type: binding initializers, call arguments, return values, field initializers,
and container elements. A value of type `S` is accepted where `T` is expected
if and only if `S <: T`; there are no implicit conversions."

Cited by: rule `flow/requires-subtype`.

## G-002: `void` against `null` in function types

Kind: under-specified.

Principle: "Functions" says `void` is valid only as a return type and that a
side-effect block's unit value is `null`, but the subtyping rules never say
how `() -> void` relates to `() -> null`. The question is not academic: the
stdlib declares `Package.tests()` as `map<string, () -> null throws unknown>`
while the values it produces reflect as `() -> void throws never`, and a
run-time membership test between the two is `false`. Either `void` is a
spelling of `null` in return position, in which case the two function types
are related by error covariance alone and the run-time test is a compiler
defect, or `void` is a distinct type and the stdlib signature is wrong.

Proposed wording, for "Functions": "`void` in return position denotes the
unit type, whose only value is `null`; `() -> void` and `() -> null` are the
same type." Or the alternative, if `void` is meant to be distinct.

Cited by: nothing yet; recorded from the engine's `passes` workaround.

## G-003: precedence between an inherent member and an interface member

Kind: under-specified.

Principle: when a concrete receiver has both an inherent member and an
interface member of the same name, one of them answers an unqualified call,
and the doc never says which. "Members of union-typed receivers" settles the
union case ("Inherent methods never participate"), and nothing settles the
concrete case. Measured on canary `0488221d0` (2026-09-10): a class with an
inherent `label` and an in-body `implements Named { label }` compiles, and
`t.label()` runs the INTERFACE body — pinned by a test block, which fails
against the inherent body's value and passes against the interface's. Note
that this contradicts the Rust-parity reading (an inherent member wins over a
trait member), which is what a reader coming from the rest of the doc would
assume.

Proposed wording, for "Interfaces": either "An inherent member shadows an
interface member of the same name: an unqualified call resolves to the
inherent one, and the interface member is reachable only through `.as<I>`",
or the opposite, whichever is intended. Whichever is chosen, the union-
receiver rule should be restated as the special case of it.

Cited by: nothing yet; recorded from the U6 scoping probes.

## G-004: `never` as an implementation target

Kind: omitted.

Principle: "Only concrete types may implement interfaces", and the Taxonomy
puts `never` in none of the three groups — it is called out separately as the
bottom type. So the doc reads against `implement I for never` without ever
addressing it. Measured on canary `0488221d0` (2026-09-10): the compiler
ACCEPTS it. An impl for `never` is vacuous (there are no values to dispatch
on) and `never <: I` already holds for every existential by the bottom-type
rule, so the impl adds nothing; but Rust does admit `impl Trait for !`, so
accepting it is a defensible position the doc should take explicitly rather
than leave to be inferred.

Proposed wording, for "Interface Implementations": "`never` is not an
implementation target: it has no values to dispatch on, and `never <: I`
already holds for every interface-existential `I` by the bottom-type rule."
Or the alternative, if vacuous impls are meant to be admitted.

Cited by: nothing yet; recorded from the U6 scoping probes.

## G-005: the member surface of an interface-existential

Kind: omitted.

Principle: a value in an interface-typed slot offers the interface's own
declarations — its methods and its fields — and nothing the concrete type
declares inherently, because the existential says only that SOME implementor
is inside. The same holds for a value typed by an interface-bounded type
variable. The doc states this for union receivers ("The member surface of a
union-typed receiver is exactly the interface methods of the interfaces that
every member implements, and nothing else") and for nothing else, though it is
the rule a reader most needs: the erasure is what makes an interface-typed
slot different from the value that was put in it. Measured on canary
`0488221d0` (2026-09-10): `m.inherent()` and `m.id` on a `Base`-typed slot are
E0007, as is `v.inherent()` on a `T extends Base`; the interface's own method
and its declared field both resolve.

Proposed wording, for "Interfaces": "A value typed by an interface-existential
or by an interface-bounded type variable offers exactly the members its
interfaces declare — fields as well as methods — and no inherent member of the
concrete type inside it. The union-receiver rule above is this rule applied to
the interfaces every member shares."

Cited by: nothing yet; recorded from the U6 scoping probes.
