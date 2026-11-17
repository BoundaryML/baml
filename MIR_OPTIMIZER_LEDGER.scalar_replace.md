# Scalar replacement: decisions

The pass lives in `baml_language/crates/baml_compiler2_mir/src/optimize/scalar_replace.rs`.
It runs at `OptLevel::Two` only, inside the cleanup loop of `optimize_body`, right after constant folding.
Tests are in `baml_language/crates/baml_tests/tests/optimizer_scalar_replace.rs`.

## What gets replaced

A group is one root local plus the locals that alias it.

- Root: a local whose every whole-local definition is a class aggregate of one class. It is not a parameter, not the return slot, not captured, not a handler's error or context local.
- Alias: a local with exactly one definition, a plain copy or move of the root or of another alias.

Each field of the class becomes one new unnamed local. A root definition becomes one store per field local. A field read or write becomes a read or write of that local. The alias copies and any `drop` of a member are deleted. The root and alias locals are left with no mentions, and dead-local elimination removes them.

The pass repeats until nothing changes. Replacing an outer object turns the store of an inner object into a copy between locals, so the inner object becomes a candidate on the next round. Nested literals are handled this way.

## What blocks the rewrite

Only four mentions of a group member are allowed: the aggregate that defines the root, the copy that defines an alias, a field projection `member.f` (alone or as the base of a longer place), and `drop(member)`. Anything else blocks the whole group. I chose an allow list so that a new MIR construct blocks by default.

Each case below was checked against the code, and falls out of the allow list:

- Passed to a call, virtual call, sys-op, spawn, await or intrinsic (log). The callee can keep or mutate it.
- Stored in an array, map, another object that itself escapes, a global, a closure cell, or the return slot.
- Thrown or rethrown.
- Captured by a closure. The local is `is_captured`, so it is never a root or alias.
- Compared (`==` and friends), tested for truthiness, used in `??` or `&&`.
- Type-tested or reflected on: `is_type`, `is_type_tag`, `type_tag`, `discriminant`, `narrow_bind`, `switch`.
- Joined into a string.
- Used as the receiver of a bound method, a virtual field read or a virtual field store.
- Copied into a local that is not a valid alias (see below).
- Indexed, or used as an index. This cannot type-check for a class, so it blocks.
- A field index past the class's slot count, or an aggregate whose operand count differs from the slot count. Both would be compiler bugs. They block.
- Defined by anything that is not an aggregate of the same class: a call result, `null`, a different class.

Class rules:

- Finalizers. A class with a `cleanup` method is never replaced. The collector runs the finalizer on the instance, so the instance has to exist. I reuse `escape::may_finalize`, which also answers "may finalize" for every class outside the body's own package and the stdlib, so a dependency gives the same answer mounted or not. I made that function `pub(crate)`.
- Top-level `let` initializers are skipped. `optimize_function_body` has no owning function, so there is no package to ask the finalizer question as. They run once, so this costs nothing.

## Aliases

An alias is replaced only if it provably names the root's current instance at every use. A forward must-analysis tracks this per alias. Nothing holds at function entry. Rebuilding the root strands all its aliases. The alias copy makes the alias as good as the local it read. A handler is entered from every point of every block it protects. If any use of an alias is not covered, the group is blocked.

Example that blocks: `let old = p; p = Point {..}; old.x`. Here `old` still names the first point.

A local with two or more definitions is never an alias. So `let v = if c { a } else { b }` over two existing objects blocks both. This is the main thing left on the table (see below).

## Panics and handlers

Field locals are written at the same program points where the instance's slots were written. A trap between two field writes leaves a handler the same values in both versions.

A root definition must behave like the original: every initializer is read, then the object appears whole. An initializer that reads a place (a field, an index, a cell) can trap, or can read a slot the same construction overwrites (`p = Point { x: p.y, y: p.x }`). Those initializers are read into temporaries first. Only then are the field locals stored. Constants and plain locals are stored directly. Plain local stores cannot trap, so no handler can see a half-built object.

## Types of the field locals

- If only constructions store to a slot, and every construction's operand has the same static type, the field local gets that type. Literal types widen to their base type. A `null`-typed operand counts as unknown, because lowering types some temporaries `null` while they hold an object.
- If the slot is also written after construction, the field local is typed only when the declared field type is `int`, `float` or `bool` and the initializers agree. Otherwise it is `unknown`.
- `unknown` is always safe: both backends fall back to generic operations. It can cost a typed fast path on a non-scalar field that is reassigned. Converting declared field types to `RuntimeTy` (with generic substitution) would fix that. I did not do it because it needs the alias environment of the class's package and I could not verify it cheaply.

## Unions, optionals, generics, arrays

- Unions and optionals. A local typed `A | B` or `A?` is replaced only when every definition builds the same class and it is never tested. Any null check, `match` or narrowing mentions the bare local and blocks. No type test is folded.
- Generics. The aggregate's type-argument templates are dropped with the allocation. Nothing can observe them once reflection and type tests are blocked. Two definitions of one generic class at different arguments share field locals.
- Arrays and maps. Not replaced. Element access has a run-time index, a bounds check and a length. A correct version needs constant-index proofs. Left out on purpose.
- Spread (`Point { ...p, y: 2 }`) lowers to field reads of the base, so it does not block.

## Debug info

Field locals have no name and no scope span. The pass runs at `OptLevel::Two` only, where the debugger's view of named locals is already not preserved.

## Changes outside the new file

- `optimize.rs`: `mod scalar_replace;`, one call in `optimize_body`, and one new parameter on `optimize_body` carrying the database and the owning package. The parameter was needed because the finalizer and layout queries need a database, which `optimize_body` did not have. The two callers pass it. A unit test in `optimize/values.rs` passes `None`.
- `escape.rs`: `may_finalize` is now `pub(crate)`.

The existing escape analysis (`escape.rs`) was not reused beyond that. It is flow-insensitive and merges every copy, and it answers "can another thread reach this", which lets an object be passed to a summarized call or stored in a local container. Scalar replacement needs "is every mention a field access", which is a different and stricter question.

## Snapshots and existing tests changed

Three corpus snapshots changed, each only because an allocation and its field accesses became locals:

- `ns_fixtures/ns_short_circuit_locals/mir.snap` and `bytecode.snap`: the `ShortCircuitBox` in `coalesced_lengths_compared_apart` is gone (`init_instance` and two `load_field_local` removed).
- `ns_fixtures/ns_llm_quoted_prompt/mir.snap`: the `baml.TaggedString` a tagged prompt builds and immediately takes apart is gone, in two functions.

Two edits to the escape-analysis tests in `baml_tests/src/compiler2_mir/escape.rs`. Those tests pin thread-locality of objects that stay in one function, which this pass now removes at `OptLevel::Two`:

- `classify` (behind `assert_locality`) lowers at `OptLevel::One`, so the objects still exist to classify. Eight tests depended on this.
- `bytecode_takes_relaxed_field_paths_for_thread_local_instances`: `build` now returns the counter instead of its field. Returning keeps the instance allocated and still thread-local, so the test still checks the relaxed field instructions.

## Things I noticed and did not change

- `Rvalue::can_discard` lets dead-code elimination drop an unused aggregate of a class with a `cleanup` finalizer. That predates this pass. The finalizer of a discarded instance never runs. Worth a look.

## Left undone, ranked by expected value once inlining exists

1. Aliases with several definitions (a join of two objects, or an inlined callee with several `return` paths that return different locals). Needs either per-field copies with a no-aliasing proof, or a phi-aware analysis.
2. Type tests and null checks on a known class. Fold `is_type`, `narrow_bind`, `type_tag` and null comparisons when the root's class is known. This unlocks optionals and unions, which inlined code that takes `T?` will hit.
3. Partial escape. An object that escapes only on a cold path (logged or thrown on error) could be materialized there and stay scalar on the hot path.
4. Objects passed to calls the inliner declines. Needs callee specialization or a by-field calling convention.
5. Small fixed arrays and tuples with constant indexes.
6. Precise types for non-scalar field locals that are reassigned.
7. Top-level `let` initializers.
