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
| `main.baml` | the composition root: the only file that knows both the engine and the bank, and the only one that talks to a terminal | — |

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
or backwards so the shape never gives the verdict away. A rejected key names
the diagnostic codes and the byte region of the probe the compiler must point
inside, which the site computes as it renders. The trace of claims,
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

Dependencies flow bank → algebra → engine. `lint.sh` fails on a reference that
runs the other way, on a wildcard match arm outside the engine (so that a new
type kind is a compile error rather than a silently-taken arm), and on a
directory gone missing, which would otherwise let a check pass by scanning
nothing. The engine owns nothing the compiler can answer: the only judgments
about BAML come from `reflect.Package.compile`.

## The quiz surface

The quiz is asked and answered through the top-level functions in
`main.baml`, which are what a generated SDK exports. There is no loop and no
state between calls. The page keeps, for each question, where its case came
from and what the learner said; everything else is worked out again from
those whenever it is wanted.

| Function | For |
|---|---|
| `fresh_profile()` | a learner the model knows nothing about, over every rule the bank can conclude a case with |
| `next_prompt(profile, knobs, session, step)` | the case to ask next, and the rule to teach before it when the case turns on one the learner is stuck on |
| `answer_case(profile, knobs, item, seed, given)` | the profile after an answer, the exchange to show, the compiler's own words, and the points |
| `replay(knobs, taken)` | the profile a list of answers leads to, from nothing: how a saved sitting is resumed |
| `adaptive_json(session, full, knobs, taken)` | the sitting as the JSON a learner takes away, with what the model concluded riding along |
| `engine.default_knobs()`, `engine.standing(profile, knobs)`, `engine.points(knobs)` | the tunables, where a sitting stands and why it ended, and what an answer is worth |
| `describe_model(id)` | what a learner who reasons as a naive model does believes, for the readout |
| `sitting_length`, `prompt_at`, `answer_at`, `compiler_report`, `sitting_json` | a sitting planned by seed alone, with no learner model, which the conformance suite drives |
| `review(path)` | read a downloaded sitting back and re-check its cases |

A prompt deliberately holds the program and where it came from, and neither
the key nor the derivation. Because a case can be generated again from its
item and seed, the answer never has to be in front of the learner to be
available when they answer. The seed crosses as a `bigint`: a stream state
uses 63 bits, and an `int` reaches a page through a JavaScript number, which
keeps 53.

Nothing crossing into this surface is an enum, nor a class holding one: the
web bridge encodes a TypeScript enum member as a bare string, which the
engine then holds in a slot typed as the enum, where `match` panics and `==`
quietly answers false. A learner's answer is `Given { said, reasoning, mark }`
of strings, and `main.baml` turns it into the engine's enums.

The learner model lives in `ns_engine/learner.baml`: Elo-style knowledge
tracing with partial pooling (one global ability, a per-rule deviation that
shrinks toward it), an abstained answer as evidence in its own right, points
derived from the bar at which a learner is asked to commit, difficulty from
the rule orders the bank derives, suspicion of each naive model measured as a
z-score against the learner's own estimate, teaching at a stall, and a stop
rule that is one of mastered, budget, or stalled. Every tunable is a field of
`Knobs`. The scripted learners in `ns_conformance/learner.baml` are what the
model is held to: an expert is certified within budget and calibrated, a
learner who never commits is never certified, a learner who reasons as
TypeScript does is found out as TypeScript while one answering by an
unrelated bit is suspected of nothing, and a learner who knows only the
axioms is certified on no variance rule.

`review` is the one function meant for a terminal: point it at a downloaded
transcript and it reports how the sitting went, what the model concluded, and
whether every case still behaves as it did when it was asked.

```bash
# from baml_language/tools/type_quiz, under mise
baml run review -- --path ~/Downloads/type-quiz-142593372.json
```

## Running

```bash
# from baml_language/, under mise
mise run type-quiz-test    # the whole suite
mise run type-quiz-lint    # layering, banned APIs, wildcard arms
mise run fmt-type-quiz     # the formatter this package is kept under
```

`crates/baml_tests/tests/type_quiz.rs` runs the suite in CI, and the
`type-quiz-lint` prek hook runs the lint after the formatter. Both of those
and the `mise` task keep the CLI's home, cache and profile streams under
`target/`. Invoking `baml-cli test --from tools/type_quiz` directly instead
leaves a few hundred megabytes of them in `tools/type_quiz/.baml`, which the
CLI marks ignored but does not clean up.

The `live` profile is reserved for calibrating the answer grader against a
real model. It selects no tests yet, and `baml-cli` exits 5 on an empty
selection, so there is nothing to run under it until the grader lands.

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
   line. See `within` in `ns_engine/verify.baml`. Same policy as 4: kept as
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
12. **An empty class in a union matches any JSON object, so its siblings
   decode as it.** With `type M = Empty | Named`, `baml.json.to_string` writes
   a `Named { model: "m" }` as `{"model":"m"}` and `from_string<M>` reads it
   back as `Empty`, silently. Decoding takes the first member that fits and an
   empty class fits everything, so listing it last happens to work, which makes
   correctness depend on the order of a type alias. The transcript's
   "who marked this" therefore carries an optional model id rather than the
   union that would say it better.
11. **String literals have no numeric or unicode escape.** `"\u{1F411}"` is
   nine characters and `"\x41"` is four: only `\\`, `\"`, `\n`, `\r` and `\t`
   are escapes, so a character outside them can only be written as itself. The
   renderer's `quote` in `ns_algebra/render.baml` therefore cannot spell a
   control character at all, and the name pools hold the characters they mean.
10. **A local inferred from a `match` with a `.map` arm is typed wrongly, and
   a method call on it fails at run time.** With `type Either = Wrapped |
   string`,
   `let lines = match (e) { let w: Wrapped => w.values.map((v) -> { v }), let s: string => [s] };`
   followed by `lines.join(",")` fails with
   `VM internal error: type error: expected map, got array`, the method having
   been dispatched as though the local were a map. It reproduces with no
   captured value, and with either or both arms mapping; all-literal arms are
   fine. Any of three things avoids it: annotating the local (`let lines:
   string[] = …`), consuming it through a free function
   (`baml.Array.length(lines)`), or returning the `match` directly instead of
   binding it. This package annotates.
