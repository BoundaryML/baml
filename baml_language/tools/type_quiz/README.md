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
another fact, and records the relation that follows. An *item* puts a fact at
a site, chosen by seed among the five flow sites — a binding, a call argument,
a return, a field initializer, an array element — and the coherence site, two
`implements` blocks for one interface. A flow site takes the pair in a
direction, forwards or backwards, so the shape never gives the verdict away.

What follows from the relation is the site's business, never the fact's. A
flow compiles for a subtype; two `implements` blocks are rejected for an
equivalence, because one type may have at most one implementation of an
interface however differently the two blocks spell it. So the facts that
conclude an equivalence — a reordered union, an alias, `bool` against
`true | false` — are the ones a flow always accepts and the ones coherence
always rejects, and a learner has to read which construct they are looking at
rather than recognise the shape. A pair only reaches the coherence site if an
`implements` block may name both types, which rules out a union, a literal,
an interface, `never`, `unknown` and a recursive alias. A rejected key names
the diagnostic codes and the byte region of the probe the compiler must point
inside, which the site computes as it renders. The trace of claims,
each rule instantiated at the case's types, is the explanation; its last claim
names which type met which slot. Every item is generated across several seeds
and checked against the compiler on every run.

Where the relation a fact states is strict, the same fact flowing the other
way is a program the compiler answers the other way about, and the item
carries it as a *foil*. The two are the same program bar the swap — the
suite holds them to being the same characters in a different order, and to
parting company only where the direction of the flow is named — so a learner
shown both and asked which one compiles can read nothing off their surface.
An equivalence or an unrelated pair reads the same in both directions and so
has no foil, and neither does anything at the coherence site, where the
reverse draw is the same two blocks the other way up. Fifteen of the bank's
thirty-six items pair; twenty-two answer both ways across their seeds, ten
always compile and four never do.

## What the bank covers

Subtyping and its variance: the taxonomy, unions, literals, `never` and
`unknown`, function parameters, returns and error types, aliases and
recursion, and the invariance of every container.

Coherence, as the other question about a pair: two `implements` blocks
conflict exactly when their targets are the same type, so every equivalence
the bank states is also a case about overlapping implementations.

Interfaces and generics, as subtyping: a class fits where an interface it
implements is expected, an interface that `requires` another fits where the
required one is, a bounded type variable fits where its bound is, and a union
of implementors fits where the interface is — while none of those hold the
other way round, and a class that declares no `implements` block is unrelated
to an interface however well it fits the shape. Each of those relations is
strict, so each states a case that compiles and one that does not, and each
carries a foil. They compose with the variance rules, so a case may be about
a union of implementors inside a function's error type.

The interfaces the bank declares have no members, at the coherence site as
well as in the facts. Every case it makes turns
on *which* types implement an interface and never on what the interface asks
of them, so a method would be text a learner has to read past before reaching
the question. What that leaves out — member resolution, `Self` and dispatch,
coherence, valid implementation targets — is material for cases about
legality rather than about flow, and is not built.

## Depth, and what a case turns on

A case's explanation lists every rule its derivation applied. What the
learner is *credited* with is narrower: only the claims the answer turns on.
A transfer that maps `Sub`, `Super` and `Unrelated` alike screens off
everything below it — invariance is exactly that — so `Box<S>` against
`Box<T>` is answerable knowing only that the two types are written
differently, and the rule beneath is part of the explanation and no part of
the question. An equivalence is the exception, since invariance carries it
through. Each claim carries whether it bears, and the tracer reads only those.

Depth therefore comes from wraps that *carry* a relation — a covariant
position keeps it, a contravariant one turns it round — and the `carried_2`,
`carried_3` and `carried_4` items take a strict pair through that many
function positions, where the verdict turns on the parity of the turns and a
learner who loses count anywhere gets it wrong. The bank's depth is a
measured number the suite pins: how many of the cases it can draw turn on
one rule, two, three, four.

## Choosing the next question

Selection draws in proportion to the information an answer is expected to
carry — the Fisher information over the ability and every rule's offset,
from the same gradients the update uses — raised to `inform`. A case the
learner would surely get right carries nothing, so does one they would
surely miss, and a case on several half-held rules carries more than one on
a single rule; so a learner who has the axioms is asked deeper things as a
consequence, and a beginner is not marched into depth, because a case they
would surely fail is worth nothing either. `inform` at zero aims at a success
rate instead (`aim`): the rate at which people are said to learn best, as
against what measuring them asks for. `exposure` penalises an item each time
it has already been served, so a rule may come round as often as the estimate
wants it while the same template does not. Both are measured in the suite: a
perfect learner's questions get deeper over a sitting, the same item comes
round three times at most, and certifying one costs 55 questions where it
had cost 79 and failed one sitting in two.

## Interestingness

A case is *interesting* when a plausible wrong intuition predicts the wrong
verdict. Each naive model in `ns_bank/models.baml` is a sparse list of rules
it disagrees with; replaying a case's derivation under the model gives the
model's verdict, and a mismatch makes the case a trap for that model. The
sampler scores candidates by traps, rule count, relation flips, and a hinged
penalty on size, depth, and union width, with a soft penalty on single-rule
cases every model agrees with. A session fixes each step's verdict on an even
coin before it looks at a candidate, re-drawing within a budget until one has
that verdict, so how interesting a case looks never predicts its answer; the
coins are independent, so neither the step index nor the previous answer
predicts the next. Only items the
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
| `put_at(knobs, item, seed)` | how a case is put: what a learner is shown and asked. It carries the answer, so it is for a test or a tool that already holds the key |
| `answer_case(profile, knobs, item, seed, given)` | the profile after an answer, the exchange to show, the compiler's own words, and the points |
| `replay(knobs, taken)` | the profile a list of answers leads to, from nothing: how a saved sitting is resumed |
| `adaptive_json(session, full, knobs, taken)` | the sitting as the JSON a learner takes away, with what the model concluded riding along |
| `engine.default_knobs()`, `engine.standing(profile, knobs)`, `engine.points(knobs)` | the tunables, where a sitting stands and why it ended, and what an answer is worth |
| `describe_model(id)` | what a learner who reasons as a naive model does believes, for the readout |
| `sitting_length`, `prompt_at`, `answer_at`, `compiler_report`, `sitting_json` | a sitting planned by seed alone, with no learner model, which the conformance suite drives |
| `review(path)` | read a downloaded sitting back and re-check its cases |

A prompt holds one program, or two when the question is which of them the
compiler accepts, along with where they came from — and neither the key nor
the derivation. What a learner may say follows from what they are shown, so
the two can never disagree. Because a case can be generated again from its
item and seed, the answer never has to be in front of the learner to be
available when they answer. The seed crosses as a `bigint`: a stream state
uses 63 bits, and an `int` reaches a page through a JavaScript number, which
keeps 53.

Nothing crossing into this surface is an enum, nor a class holding one: the
web bridge encodes a TypeScript enum member as a bare string, which the
engine then holds in a slot typed as the enum, where `match` panics and `==`
quietly answers false. A learner's answer is `Given { said, reasoning, mark }`
of strings, and `main.baml` turns it into the engine's enums.

The learner model lives in `ns_engine/learner.baml`. A question is answered
right when the learner applies every rule its case turns on and gets away
with it, so the chance of that is the PRODUCT over the rules of how strongly
they hold each, times a chance of being caught by each trap on the case; the
three things that can then happen — right, wrong, held back — are a
conjunctive (DINA) model with the abstention as an outcome of its own rather
than a discounted wrong answer. Each rule's strength is the learner's general
ability plus a deviation whose prior is centred on zero, which is the
pooling. An answer updates both by the Gaussian posterior for one
observation: the mean moves by the score of the log-likelihood and the
variance falls by the Fisher information the outcome actually carries, so an
answer the model already expected narrows nothing, and a rule the learner
already holds is neither moved nor narrowed by a case that also turns on
something they do not. Blame is derived rather than apportioned, and joint
evidence does not count as independent evidence about each rule.

How often a learner commits to an answer they do not have is ESTIMATED from
the sitting rather than assumed from the bar: a learner who never says "not
sure" is guessing at even odds, and reading their right answers as though
they were not would certify them on a fraction of the evidence. That estimate
is also the calibration the readout shows, since it is the one thing adaptive
selection does not confound.

The rest: points derived from the bar at which a learner is asked to commit,
suspicion of each naive model as a one-step-ahead residual read as a z-score,
teaching at a stall, a rate at which a case that has a foil is put as a
choice between its two programs rather than as a verdict on one, and a stop
rule that is one of mastered, budget, or stalled. Every bound is taken at
`confidence / rules`, so testing twenty-five rules at once is still the claim
it reads as; `target` is set to what the bank can actually prove, which for a
perfect learner is about three rules in four, and raising it is a reason to
write harder items rather than to move the number. Every tunable is a field of
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

## The judge

In a sitting that asks why, a model marks the learner's reasoning against
the case's own explanation, and the learner's own hand is what stands in when
there is no model to ask: `engine.judge_reasoning` takes the revealed case
whole and works out what the model is told — the programs as text, what the
compiler makes of each, the rubric, what the learner claimed and what they
were asked to explain — and gets back `Grade { understanding, feedback,
missed }`: sound, partial or wrong; a short paragraph to the learner; and the
rubric lines their reasoning did not reach. A right answer for a wrong
reason is wrong. The verdict itself is never the model's to decide: the key
decided that, and `judge` in the engine reads only the key. Who marked a case
travels with it, as `Given.marked_by`: the model's name for a judgement, empty
for a mark the learner made themselves.

Everything the grader is told follows from the answer, so none of it can
belong to a different question. `said_of` reads the five words an answer is
given in, once, and `rubric`, `answer_sentence` and `asked_sentence` all work
from the value it returns. The rubric is what the programs shown agree about
plus what is true of the one the learner says the compiler rejects: for a
pair, that second part is what actually decides the case and is exactly what
they were asked to explain, and marking their reason against the agreed part
alone would credit them for the setup while asking nothing about the
difference. With one program the claims are all shared and its own are empty,
so the same expression covers both and there is no case for one and a case
for two.

A judgement that could not be made comes back as a value, not a throw:
`judge_reasoning` returns `Grade | Unjudged`, and `Unjudged.why` carries the
failure's own rendering — which of a refused key, an unreachable model and a
spent quota it was is the only thing the learner can act on, and paraphrasing
it would throw that away. The catch names the whole error channel of a client
call (`ai.errors.Failure` and the four the runtime itself raises), so a new
channel stops the package compiling rather than reaching a learner as a
blank.

**The key is the learner's, and it goes nowhere of ours.** The page keeps it
in the browser and passes it into each call; `judge_client` builds an
Anthropic client with it and the header Anthropic documents for a request
made from a browser, and that one request is the only thing it touches. The
site is static — it has no server — so there is nothing for a key to be sent
to but the provider. Nothing in this package reads an environment variable
on the quiz path. The grader's calibration fixtures (`ns_conformance/
grader.baml`) run only under the `live` profile and only when
`ANTHROPIC_API_KEY` is set, so CI, which runs offline, never calls a model.

## Standing a model in for a learner

`ns_harness/take_quiz.baml` has a language model sit the quiz, in mastery
mode, exactly as the page puts it: the same adaptive selection, the same three
answers, the same reveal after each one. It drives the quiz in-process through
`adaptive_step`, so a step costs only what the model takes to answer.

```
baml run take_quiz --from tools/type_quiz -- \
    --sessions 3 --models opus,sonnet,haiku --out target/type-quiz-llm
```

from `baml_language`, where TYPE_SYSTEM.md and target/ are. Every model sits
every session; `--concurrency` bounds how many sittings run at once;
`--max N` stops a sitting after N answers, for a cheap end-to-end check.

Every model sits every session, so until their answers part company they are
asked the same questions. Runs are written as they go and resume where they
stopped, because the whole state of a sitting is its session number and the
answers given so far — which is also what `adaptive_step` takes.

The model cannot cheat, and not because it was asked not to. It is a
`claude -p` session with every tool denied and no MCP server, which leaves it
none at all: no compiler, no files, no search. It gets `TYPE_SYSTEM.md` and
the scoring rule, and it reasons. The quiz's own answer to a step comes back
in two parts — `asked`, which is everything a learner may see beforehand, and
`marked`, which is what the last answer was worth — so the harness cannot pass
on a key it never receives. What separates two programs of a pair is in
`marked`, not `asked`, because the page says it in the reveal and a driver
that said it first would be asking an easier question.

Each run writes `<model>-<session>.json`: the answers, the model's replies in
full, what each answer was worth, and the transcript a learner would have
downloaded, which `baml run review` reads back like any other.

**What it spends.** `ANTHROPIC_API_KEY` takes precedence over a claude.ai
login, so a harness that passed the environment through would spend API
credits without anyone choosing to. Measured, not assumed: with an invalid key
the call fails outright and the CLI says the key wins over the login; with the
key unset it succeeds on the login. So `--billing plan` is the default and
unsets the key for the models (through `env -u`), and `--billing api` is
something you have to ask for. The cost a run reports is what its tokens would cost at API rates —
on the plan nothing is charged and it is only a proxy for how much quota went.
Measured at the start of a sitting, where context is smallest: about $0.02 a
question for haiku, $0.10 for sonnet, $0.16 for opus, roughly doubling by the
end of a sitting as the conversation grows.

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
11. **String literals have no numeric or unicode escape.** `"\u{1F411}"` is
   nine characters and `"\x41"` is four: only `\\`, `\"`, `\n`, `\r` and `\t`
   are escapes, so a character outside them can only be written as itself. The
   renderer's `quote` in `ns_algebra/render.baml` therefore cannot spell a
   control character at all, and the name pools hold the characters they mean.
12. **An empty class in a union matches any JSON object, so its siblings
   decode as it.** With `type M = Empty | Named`, `baml.json.to_string` writes
   a `Named { model: "m" }` as `{"model":"m"}` and `from_string<M>` reads it
   back as `Empty`, silently. Decoding takes the first member that fits and an
   empty class fits everything, so listing it last happens to work, which makes
   correctness depend on the order of a type alias. The transcript's
   "who marked this" therefore carries an optional model id rather than the
   union that would say it better.
13. **The generated SDK's bytecode is stamped with the git commit, so the web
   app breaks on every commit.** `baml_sdk` records the toolchain as a commit
   hash and the bridge refuses a mismatch —
   `generated bytecode: toolchain <a>; this runtime: <b> — regenerate baml_sdk
   and rebuild the bridge from the same commit`. Nothing about the compiler
   need have changed: committing this package is enough to strand a bridge
   built an hour ago, and the bridge is a multi-minute wasm build. A
   fingerprint over the bytecode format and the compiler's own sources would
   refuse the pairs that actually disagree.

