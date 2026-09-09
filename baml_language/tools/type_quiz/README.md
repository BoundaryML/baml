# Type-system quiz

A quiz that teaches the BAML type system by asking a learner to predict the
compiler: does this program compile, and why. Cases are generated from the
rules in `../../TYPE_SYSTEM.md` and every case is verified against the real
compiler, so the quiz cannot drift from the language without CI noticing.

## Layers

| Namespace | Role | Changes when |
|---|---|---|
| `ns_engine` | substrate: seeds, cases, verdicts, the verifier, sampling, the learner | never for a type-system change |
| `ns_algebra` | tested material: the `Ty` model, rendering, relations, sites | the type system changes |
| `ns_bank` | tested material: rules, naive models, items, weights | what or how we teach changes |
| `ns_conformance` | the tripwire suites | — |

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

1. **Emit panic on existential dispatch inside a nested closure.**
   `crates/baml_compiler2_emit/src/emit.rs:2025` panics with
   `undefined function: user.Item.id` for
   `items().filter_map((item) -> { [0].every((i) -> { item.id() == "x" }); null })`
   where `items(): Item[]` and `Item` is an interface. One closure level works.
   Workaround in `ns_conformance/engine.baml` (`item_is_deterministic`).
2. **Function values never match function-typed patterns.** With
   `t: map<string, () -> null throws unknown>` and a present key,
   `if let f: () -> null throws unknown = t.get(k)` takes the `else` branch and
   `t.get(k) is (() -> null throws unknown)` is `false`; `t.get(k) != null` and
   `t[k]` behave. Workaround in `ns_engine/verify.baml` (`passes`).
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
