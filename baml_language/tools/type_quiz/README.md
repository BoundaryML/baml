# Type-system quiz

A quiz that teaches the BAML type system by asking a learner to predict the
compiler: does this program compile, and why. Cases are generated from the
rules in `../../TYPE_SYSTEM.md` and every case is verified against the real
compiler, so the quiz cannot drift from the language without CI noticing.

## Layers

| Namespace | Role | Changes when |
|---|---|---|
| `ns_engine` | substrate: seeds, cases, claims, verdicts, the verifier, sampling and sessions | never for a type-system change |
| `ns_algebra` | tested material: the `Ty` model, rendering, relations, sites | the type system changes |
| `ns_bank` | tested material: the rule table quoted from the spec, fact generators, items, naive models, features, weights, name pools | what or how we teach changes |
| `ns_conformance` | the tripwire suites | — |

Two ledgers sit beside the code. `SPEC_GAPS.md` records principles the spec
omits or under-specifies; a rule with no section to cite must cite an entry
there. `COMPILER_DIVERGENCE.md` records where the compiler contradicts the
spec; an item marked as diverging must cite an entry there, and the suite
fails the moment the compiler stops diverging so the entry gets closed.

## How a case is made

A *fact* applies one spec rule to seeded types, or a context rule (array,
map, class argument, function parameter, return, error, union member) to
another fact, and records the relation that follows. An *item* exercises a
fact at a flow site, chosen by seed among a binding, a call argument, a
return, a field initializer, and an array element, flowing the pair forwards
or backwards so the shape never gives the verdict away. The trace of claims,
each rule instantiated at the case's types, is the explanation; its last claim
names which type met which slot. Every item is generated across several seeds
and checked against the compiler on every run.

A case is *interesting* when a plausible wrong intuition predicts the wrong
verdict. Each naive model in `ns_bank/models.baml` is a sparse list of rules
it disagrees with; replaying a case's derivation under the model gives the
model's verdict, and a mismatch makes the case a trap for that model. The
sampler scores candidates by traps, rule count, relation flips, and a hinged
penalty on size, depth, and union width, with a soft penalty on single-rule
cases every model agrees with. A session fixes each step's verdict from a
seeded, balanced schedule before it looks at a candidate, re-drawing within a
budget until one has that verdict, so how interesting a case looks never
predicts its answer; the schedule is a uniform shuffle, so the
step index and the previous answer predict nothing either. Only items the
compiler is verified to agree on are served, and the suite verifies every case
a fixed session serves.

Dependencies flow bank → algebra → engine and `lint.sh` rejects anything
else. The engine owns nothing the compiler can answer: the
only judgments about BAML come from `reflect.Package.compile`.

## Running

```bash
# from baml_language/, under mise
target/debug/baml-cli test --from tools/type_quiz          # offline: every tripwire
target/debug/baml-cli test --from tools/type_quiz --profile live   # grader calibration
tools/type_quiz/lint.sh                                    # layering and bans
```

CI runs the same two commands through `crates/baml_tests/tests/type_quiz.rs`
and the `type-quiz-lint` prek hook.

## Compiler issues surfaced by this tool

Building the quiz is dogfooding, and each of these was found by it. Repros are
minimal single-file packages; none is fixed at the time of writing.

1. **Emit panic on an interface method call on a captured existential inside
   a closure.** `crates/baml_compiler2_emit/src/emit.rs:2025` panics with
   `undefined function: user.Item.id` for
   `items().filter_map((item) -> { [0].every((i) -> { item.id() == "x" }); null })`
   where `items(): Item[]` and `Item` is an interface: the inner closure calls
   a method on `item`, which it captures from the outer one. A method call on
   a closure's own parameter works at any depth, and so does handing the
   captured value to a function that makes the call, which is what
   `root.engine.generate` in `ns_engine/sample.baml` is for.
2. **Run-time membership of a function value is exact, not a subtyping
   check, and `Package.tests()` lies about its value type.** The map is
   declared `map<string, () -> null throws unknown>` but its values reflect as
   `() -> void throws never`, so `if let f: () -> null throws unknown = t.get(k)`
   and the matching `is` are both false for a present key, while a value from
   `get_function<F>` matches `F` exactly. Either `void`/`never` should satisfy
   `null`/`unknown` under function subtyping, or `tests()` should declare what
   it returns. Workaround in `ns_engine/verify.baml` (`passes`); see also
   SPEC_GAPS.md G-002.
3. **Parse ambiguities at block boundaries.** `else { "none" }` parses the block
   as a map literal (`expected ':'`), and an `if let … { `…` } else …` whose
   then-block is a bare template literal fails with `expected expression, found
   else`. Binding the value to a local first (`{ let s = …; s }`) avoids both.
4. **`baml fmt` renders empty class literals as `Foo {  }`** (two spaces) and
   empty class declarations as `class Foo {\n}`. Cosmetic, but it is the
   formatter's canonical output, so the package keeps it rather than fighting
   the hook.
5. **`baml fmt` breaks `?? return` across lines regardless of width.**
   `let s = span ?? return null;` becomes two lines with the `??` dangling, and
   `let bytes = (files.get(name) ?? return null).to_utf8();` becomes five,
   with the parenthesised guard split over three and `.to_utf8()` on its own
   line. See `line_of` in `ns_engine/verify.baml`. Same policy as 4: kept as
   the formatter emits it.
6. **A function type with a `throws` clause does not parse inside call-site
   angle brackets.** `reflect.Type.of<(int) -> string throws never>()` and
   `pkg.get_function<() -> reflect.Type throws never>("root.f")` fail with
   `expected lambda body '{', found '>'`, while `get_function<() -> bool>(…)`
   and the same type behind a `type` alias both work. The engine's oracle and
   verifier go through aliases.
7. **Reflection cannot decompose several kinds.** `unknown`, `never`, a
   recursive alias, and `reflect.Type` all classify as `primitive`, and the
   primitive and literal views expose nothing but `to_string`; a class view
   exposes fields but not type arguments; an interface view exposes nothing
   about its pins. A faithful `from_reflect` is therefore impossible today,
   which is why the render round trip compares the compiler's own values
   (parse-then-print fixed point, and identity with constructor-built types)
   instead.
8. **`to_string` on a function type is lossy.** `(x: int, b?: int) -> int
   throws never` prints as `(int, int) -> int throws never`: names and the
   optional marker are dropped, so the printed spelling denotes a different
   function type than the value describes.
9. **A closure reads the wrong slot when a method call's receiver reads a
   captured local that an earlier closure call assigned.** With `items: Sc[]`,
   `let top = items.reduce((acc, s) -> { if (s.score > acc) { s.score } else { acc } }, items[0].score);`
   followed by `items.map((s) -> { (s.score - top).exp() })` fails at run time
   with `VM internal error: type error: expected map, got float`, the closure
   parameter having been read from the captured float's slot. Copying `top`
   into a fresh local first does not help; the same code with `top` a literal
   works, and so does the free-function spelling `baml.Float.exp(s.score - top)`,
   which `pick` in `ns_engine/sample.baml` uses.
