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
