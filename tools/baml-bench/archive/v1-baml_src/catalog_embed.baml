// GENERATED — the design-principles.md catalog embedded at pack time so the
// packed `baml-bench` binary is self-contained. Regenerate with:
//   python3 scripts/embed_catalog.py   (or re-run the generation snippet in git history)
// Double-hash raw string: the catalog contains `"#` but no `"##`.

function embedded_catalog() -> string {
    ##"
        # BAML Design-Principle Catalog

        Distilled from all 50 BEPs on beps.boundaryml.com (bodies + comment threads), 2026-07-06. This is the grading corpus for the adherence algorithm in `adherence-algorithm.md`: each **principle card** says why a feature exists, how code should use it, what fighting it looks like, when a chunk gets graded against it, and whether a static check or an LLM judgment does the checking. Raw source docs live in `data/beps/`.

        ## How to read a card

        - **Principle** — the recorded design intent (the "why").
        - **Intended usage** — the shape adherent code takes.
        - **Anti-patterns / slop signals** — concrete misuses; these are what graders quote as evidence.
        - **Applicability trigger** — the routing predicate: which chunks this card applies to.
        - **Detectability** — `static` (AST/grep), `semantic` (LLM judgment), or `hybrid`.

        ## Grading weight by BEP status

        | Status | Weight |
        |---|---|
        | implemented | full — violations are slop |
        | accepted | full for rationale, soft for exact API spellings |
        | pending / proposed | rationale only — grade contradictions of the recorded reasoning (e.g. SQL string concatenation), never absence of the unshipped API |
        | draft | directional; flag, don't score |
        | rejected / superseded | inverted — the rejected design appearing in user code IS the anti-pattern |

        **Toolchain gate:** before applying any card, the grader must check the project's pinned toolchain actually ships the surface in question (e.g. `${}` interpolation vs legacy Jinja prompts, `$tool` companions, `readonly`). A card can only be violated by code that had the primitive available.

        ## The seven cross-cutting themes

        Nearly every card instantiates one of these. They are the language's definition of quality, and the highest-level answer to "what is slop":

        1. **Derive the second artifact from the first.** Streaming types from types, tool schemas from functions, CLIs from signatures, `to_debug` from class structure, `$parse`/`$render_prompt` companions from LLM functions. Slop = maintaining a hand-written parallel copy that can drift.
        2. **One mechanism, with prescribed replacements.** One arrow token, one binding marker (`let`), one interpolation grammar, arrays-not-variadics, closures-not-currying, unions-not-overloading. Slop = simulating an excluded feature with weaker parts.
        3. **Policy lives at the caller/config layer, not inline.** Retries in client config, budgets/timeouts as middleware, sampling as data dependencies, test repetition as runners, exit codes decoupled from returns. Slop = hand-rolled retry loops, inline counters, per-function hardcoded policy.
        4. **Purpose-built types over weak primitives.** Temporal taxonomy for time, `bigint` beyond i64, `json` for interchange, `Uuid`/`Regex` as values, `type` values compared by identity. Slop = stringly-typed domain values, epoch-int arithmetic, hand-rolled parsers/escapers.
        5. **Explicit, visible intent at the point of use.** Nominal `implements`, named defaulted args, `ns_` opt-in, `reflect.` for types-as-values, explicit conversions into JSON/prompts/bytes, `defer` next to acquisition. Slop = ambient/inferred behavior, action at a distance, string-matched dispatch.
        6. **Strict by default; every relaxation is a named opt-in.** No sniffing, no implicit coercion, panics excluded from wildcards, `Cancelled` propagates, alphabet-strict base64, `on_error: "skip"` paired with diagnostics. Slop = broad catch-and-ignore, silent lossy defaults, lenient flags with unconsumed diagnostics.
        7. **Language-native evaluation and observability.** Tests are the eval harness (judges are functions, testsets are datasets), metrics/trace identity are language surface, quality criteria live in executable asserts. Slop = external eval scripts, trace-id fields on domain types, criteria that exist only in prose.

        ## Index of batches

        1. Functions, interfaces & organization — BEPs 17, 33, 44, 57, 62, 12, 8, 4
        2. Control flow, patterns, cleanup & concurrency — BEPs 15, 11, 16, 42, 51, 41, 34, 31
        3. Type system & data — BEPs 21, 22, 38, 39, 43, 55, 13, 29, 5
        4. Standard library surface — BEPs 45, 46, 47, 48, 49, 52, 60, 32, 37, 61
        5. LLM machinery & testing — BEPs 9, 58, 59, 23, 36, 40, 56, 30
        6. Observability, execution & tooling — BEPs 50, 53, 54, 26, 27, 28, 35

        ---
        # Batch: Functions, interfaces & organization (BEPs 17, 33, 44, 57, 62, 12, 8, 4)

        Batch philosophy: These BEPs share one governing bias: BAML optimizes for AI agents and fast-moving humans writing *traceable* code — every construct should make intent visible at the point of use (no imports, `prefix = external`, named-only defaulted args, explicit `implements`, one arrow token) rather than relying on ambient context or structural inference. The language deliberately keeps a small number of orthogonal mechanisms with prescribed replacements for excluded features (arrays over variadics, closures over currying, unions over overloading, `class` over anonymous records), so "fighting the design" almost always shows up as hand-rolled simulations of a feature the language already provides. Where BAML resembles TypeScript it is a deliberate borrowing, and where it diverges (nominal conformance, `->` everywhere, invariant fields, `root.*` paths) the divergence is load-bearing — code that imports TS idioms wholesale is a slop signal, not a familiarity win.

        ---

        ### P-017-1: One arrow token everywhere
        - **Source**: BEP-17 (Function and Lambda Return Types Syntax, status: implemented)
        - **Principle**: The final decision is `->` as the single token for function declaration return types, function types, and lambdas — simpler to teach ("same token everywhere") and avoids agents assuming unsupported TypeScript syntax.
        - **Intended usage**: `function f(x: int) -> int { ... }`, `type Cb = (int) -> int`, `let cb: (T) -> T = (arg) -> T { ... }`. Match arms remain `pattern => result`.
        - **Anti-patterns / slop signals**:
          - TS-style `: ReturnType` on function declarations or `=>` lambdas (`(x) => x + 1`) — the rejected alternative.
          - Mixing return-type tokens across the codebase (some `:`, some `->`).
          - Other TS-isms leaking in (`console.log`, `import`, invented TS features).
        - **Applicability trigger**: any function declaration, lambda, or function-type annotation.
        - **Detectability**: static — grep/AST for `): Type {` declaration forms and `=>` outside match arms.

        ### P-017-2: Braces and parens are mandatory on lambdas
        - **Source**: BEP-17 (Function and Lambda Return Types Syntax, status: implemented)
        - **Principle**: All lambda bodies take `{ }` and all lambda parameter lists take `( )` — this removes function-type-vs-lambda-body ambiguity and keeps lambdas consistent enough to admit LLM-function bodies inside them.
        - **Intended usage**: `items.map((x) -> { x.name })`.
        - **Anti-patterns / slop signals**:
          - Braceless expression lambdas (`(x) -> x + 1`) or paren-less params (`x -> ...`) — TS habits the design explicitly ruled out.
        - **Applicability trigger**: any lambda expression.
        - **Detectability**: static — parser/grep for lambda forms without braces or parens.

        ### P-017-3: Exactly two function forms; last expression is the value
        - **Source**: BEP-17 (Function and Lambda Return Types Syntax, status: implemented)
        - **Principle**: Because BAML has explicit `self` and no `this` binding, there is no semantic reason for TypeScript's six function forms; BAML keeps only named `function` declarations and anonymous lambdas, with implicit return of the block's final expression.
        - **Intended usage**: named things use `function Name(...)`; anything inline/anonymous is a lambda; a block's last expression is its value.
        - **Anti-patterns / slop signals**:
          - Simulating function expressions (`let f = function(...)`-style thinking) instead of a lambda or a named declaration.
          - Reflexive `return` on every final expression, especially in small lambdas — fighting expression-orientation.
        - **Applicability trigger**: function/lambda definitions and their final statements.
        - **Detectability**: hybrid — static for form; light judgment for "gratuitous return" style.

        ### P-017-4: Types-as-values go through `reflect.`
        - **Source**: BEP-17 (Function and Lambda Return Types Syntax, status: implemented)
        - **Principle**: Bare type names in type position are always types; using a type as a runtime value requires the explicit `reflect.` qualification — keeping signatures TS-clean while making reflection visible.
        - **Intended usage**: `typebuilder.add_property("hello", reflect.string)`; composite reflected types via functions, e.g. `reflect.map(reflect.string, reflect.int)`.
        - **Anti-patterns / slop signals**:
          - Passing bare type names as runtime values, or inventing string-name-based type plumbing (`"string"` literals fed to schema builders) to dodge reflection.
        - **Applicability trigger**: any runtime schema construction / reflection-adjacent code.
        - **Detectability**: static — grep for schema-builder calls whose type arguments are strings or bare identifiers rather than `reflect.*`.

        ### P-033-1: Optional, nullable, and defaulted are three orthogonal concepts
        - **Source**: BEP-33 (Optional Parameters with Default Values, status: implemented)
        - **Principle**: A parameter is optional iff it carries `= expr` in the definition; `T?` means nullable, and the two compose independently. Optionality in *types* is spelled `name?: T`, and types never carry default expressions.
        - **Intended usage**: `function Search(query: string, max_results: int = 10, config: Config? = null) -> Result`; type written as `(string, max_results?: int) -> Result`.
        - **Anti-patterns / slop signals**:
          - Using `x: T? = null` plus an in-body `if x == null { x = <real default> }` when the default is expressible directly as `x: T = <expr>` — conflating nullable with optional and reimplementing defaults by hand.
          - Treating `T?` params as "optional" and forcing callers to pass explicit `null`.
        - **Applicability trigger**: any function signature with nullable or defaulted parameters; any body that null-checks a parameter to substitute a constant.
        - **Detectability**: hybrid — the null-then-assign-default pattern is greppable; deciding the default was expressible needs judgment.

        ### P-033-2: Defaulted arguments are named at the call site
        - **Source**: BEP-33 (Optional Parameters with Default Values, status: implemented)
        - **Principle**: `Search("cats", max_results = 5)` — naming defaulted args is self-documenting, survives signature refactors, and maps to every host language's idiom; positional passing of defaulted params is rejected specifically because an AI silently relying on parameter order breaks silently.
        - **Intended usage**: required params positional or named; everything defaulted is `name = value`; once named, stay named.
        - **Anti-patterns / slop signals**:
          - Long positional call chains (`CreateUser("Alice", "a@x.com", "admin", true, 30)`) where naming would disambiguate.
        - **Applicability trigger**: call sites with 3+ arguments or any bool/int literals whose meaning isn't obvious.
        - **Detectability**: hybrid — positional-vs-named is static; "should have been named" is judgment.

        ### P-033-3: Defaults are call-time expressions — use their power
        - **Source**: BEP-33 (Optional Parameters with Default Values, status: implemented)
        - **Principle**: Defaults evaluate at call time inside the function scope (avoiding Python's mutable-default bug) and may reference earlier parameters — `chunk_size: int = len(text) / 3` is the designed way to express derived defaults.
        - **Intended usage**: literals for the common case (they project into host-language signatures); expression defaults referencing earlier params for derived values.
        - **Anti-patterns / slop signals**:
          - Computing a derived default in the body behind a sentinel when a left-to-right expression default would do.
          - Fear-of-mutable-default idioms imported from Python (defaulting to `null` and constructing `[]` inside the body) — `tags: string[] = []` is safe by design.
        - **Applicability trigger**: functions whose bodies begin by "filling in" parameter values.
        - **Detectability**: semantic — needs LLM judgment that the body prologue is a hand-rolled default.

        ### P-033-4: Use the prescribed replacements for excluded parameter features
        - **Source**: BEP-33 (Optional Parameters with Default Values, status: implemented)
        - **Principle**: BAML deliberately excludes variadics, `**kwargs`, overloading, positional-only/keyword-only markers, argument labels, partial-application syntax, and call-site spread — each with a named replacement — because catch-alls defeat type checking and hidden signatures make AI-written bugs hard to trace.
        - **Intended usage**: explicit array param instead of `...args` (`sum([1,2,3])`); declared `config: map<string, string>` or a class instead of kwargs; union types or distinct names (`ParseText`/`ParseTokens`) instead of overloading; a closure `(q: string) -> { Search(q, filter = "recent") }` instead of partial application.
        - **Anti-patterns / slop signals**:
          - `map<string, unknown>`-style "options bags" for a fixed, known set of options that should be declared defaulted params.
          - Near-duplicate functions differing only in arity that defaults would collapse (`Foo` / `FooWithLimit`).
          - Wrapper functions that re-declare and forward every param where a closure suffices.
        - **Applicability trigger**: any options-bag parameter, any family of same-stem function names, any forwarding wrapper.
        - **Detectability**: hybrid — name-family and map-typed option params are greppable; "should have been defaults" needs judgment.

        ### P-044-1: Interfaces are for open, extensible dispatch
        - **Source**: BEP-44 (Interfaces, status: implemented)
        - **Principle**: The motivating use cases are tool/agent dispatch, cross-cutting shapes, mockable boundaries, and heterogeneous collections — abstractions where adding a new implementor must not touch existing consumers ("Adding a fourth tool doesn't touch `RunTool`").
        - **Intended usage**: `interface Tool { function call(self) -> string }`, each tool `implements Tool { ... }`, consumers take `t: Tool`; swappable boundaries (`VectorStore` with `PineconeStore`/`InMemoryStore`); `Plugin[]` heterogeneous arrays; interface returns from LLM functions render `oneOf(implementors)`.
        - **Anti-patterns / slop signals**:
          - A closed union of concrete classes plus a `match` in every consumer, where each new variant requires edits at every dispatch site — the exact pattern the BEP says "doesn't scale."
          - String tag fields (`kind: "web_search"`) with string-matched dispatch instead of interface methods.
          - Interfaces for a genuinely closed set where exhaustive `match` on a union is the honest model (sealed interfaces are explicitly deferred; unions remain right for closed sets).
        - **Applicability trigger**: any polymorphic dispatch over 2+ concrete types; any `match` narrowing to concrete classes repeated across files.
        - **Detectability**: hybrid — repeated concrete-class matches are greppable; open-vs-closed intent is judgment.

        ### P-044-2: Conformance is nominal and explicit
        - **Source**: BEP-44 (Interfaces, status: implemented)
        - **Principle**: `implements` is required, never inferred — a shape-matching class does not conform. Comments record why: interfaces have runtime semantic weight (match arms, LLM `oneOf`), so structural auto-conformance would let unrelated edits silently change behavior.
        - **Intended usage**: in-body `implements I { ... }` blocks; out-of-body `implements I for T { ... }` for field-free behavioral mix-ins on primitives/foreign types (serialization, hashing) — never for data.
        - **Anti-patterns / slop signals**:
          - Functions taking `unknown` and probing fields to fake structural typing.
          - Duplicating a full interface's method set on a class *without* declaring `implements` — shape-conformance that buys nothing nominally.
        - **Applicability trigger**: class definitions whose method/field sets mirror an interface; `unknown`-typed dispatch code.
        - **Detectability**: hybrid — mirror-shape-without-implements is mostly static; intent needs judgment.

        ### P-044-3: Default methods eliminate copy-paste conformance
        - **Source**: BEP-44 (Interfaces, status: implemented)
        - **Principle**: Interfaces carry method bodies as inheritable defaults; a class with matching fields conforms with an empty `implements I {}`, and overrides can delegate to the original via `default.method()`.
        - **Intended usage**: `interface Timestamped { ... function is_stale(self, t) -> bool { ... } }` with `implements Timestamped {}`; extension overrides like `return now().to_string() + " " + default.log(msg)`.
        - **Anti-patterns / slop signals**:
          - The same method body copy-pasted into multiple `implements` blocks instead of hoisted to an interface default.
          - Overrides that re-inline the default's logic instead of calling `default.method()`.
          - Redundantly re-declaring a default method with an identical body (override that changes nothing).
        - **Applicability trigger**: multiple implementors of one interface; any override of a defaulted method.
        - **Detectability**: hybrid — identical bodies across impl blocks are near-static (diff); "should be a default" is judgment.

        ### P-044-4: Fields live on the class; the implements block is a verified contract
        - **Source**: BEP-44 (Interfaces, status: implemented)
        - **Principle**: Interface fields are not injected — classes declare them at top level, auto-linked by name, or wired with `interface_field as class_field` when names/types conflict. Field types match invariantly, because subtype fields are a covariant-write hazard (the Java `ArrayStoreException` class of bug the BEP chose to be "safer than TypeScript" about).
        - **Intended usage**: `class Server { host: string  port: int  implements Config {} }`; conflicts via `implements Named { name as real_name }`.
        - **Anti-patterns / slop signals**:
          - Declaring a class field at a subtype of the interface's field type — rejected by design; workarounds around it signal fighting invariance.
          - Renaming class fields to dodge a conflict when an `as` link is the designed tool.
        - **Applicability trigger**: classes implementing field-bearing interfaces, especially two interfaces sharing field names.
        - **Detectability**: static — field/type comparison against implemented interfaces.

        ### P-044-5: The type is the disambiguator; downcasts are always explicit
        - **Source**: BEP-44 (Interfaces, status: implemented)
        - **Principle**: Ambiguous members are resolved by *holding the value at an interface type* (variable annotation or `upcast<I>(x)`, a compile-time widening); downcasting and cross-interface conversion always go through `match`/`is`, so every runtime class check is visible in source and a variable's static type accurately predicts its callable methods.
        - **Intended usage**: `upcast<Serializer>(d).encode()`; `let ci: Converter<int> = m`; `match (a) { let d: Dog => ..., _ => ... }` for narrowing.
        - **Anti-patterns / slop signals**:
          - Avoiding ambiguity by renaming methods or splitting classes when an `upcast` would do.
          - Hand-rolled discriminator fields checked at runtime instead of `is`/`match` narrowing.
          - Any attempt to treat `upcast` as a downcast.
        - **Applicability trigger**: multi-interface classes; any code converting between interface views of a value.
        - **Detectability**: static for `upcast` misuse; hybrid for discriminator-field slop.

        ### P-044-6: `requires`, not inheritance — every conformance is local
        - **Source**: BEP-44 (Interfaces, status: implemented)
        - **Principle**: An interface states preconditions with `requires`; implementors must write a separate `implements` block for each required interface (strict, even for all-default parents). This was chosen over `extends` precisely because inheritance-bundled conformance hides field injection and creates diamond ambiguity.
        - **Intended usage**: `interface Person requires Named, Aged { ... }`; `class Employee { implements Named {}  implements Aged {}  implements Person {} }`. Diamonds are fine because each block resolves independently.
        - **Anti-patterns / slop signals**:
          - Copying a required parent's members into the child interface (duplicating the contract instead of `requires`).
          - Deep `requires` chains used as pseudo-class-inheritance for data (field-heavy interfaces are suspect; behavior contracts are the sweet spot).
        - **Applicability trigger**: interface declarations with related/parent contracts; classes implementing an interface family.
        - **Detectability**: static for duplicated members across related interfaces; semantic for inheritance-mimicry.

        ### P-057-1: Ask "could one type implement this twice?" — associated type vs generic parameter
        - **Source**: BEP-57 (Associated Types for Interfaces, status: implemented)
        - **Principle**: A generic parameter `<T>` is a caller-tuned input knob (multiple impls per type is a *feature*, e.g. `Converter<int>` + `Converter<string>`); an associated type `type Item` is an implementor-fixed output (exactly one binding). Modeling an output as a generic parameter causes the witness-parameter tax and spurious ambiguity — the exact mistake pre-1.0 Rust made with `Iterator<A>`.
        - **Intended usage**: `interface Iterator { type Item  function next(self) -> Item? }` with `implements Iterator { type Item = int ... }`; caller-choice interfaces stay generic (`Converter<T>`). Both may coexist (`Cache<K>` with `type Value`).
        - **Anti-patterns / slop signals**:
          - Iterator/stack/parser-shaped interfaces declared as `interface Stack<T>` where no type would ever implement two instantiations.
          - Witness parameters: `function topOf<T, S extends Stack<T>>(s: S) -> T?` where `S.Item` eliminates `T`.
          - Conversely, an associated type where callers genuinely need to pick different fillings per use.
        - **Applicability trigger**: any generic interface declaration; any generic function with a type parameter appearing only inside another parameter's bound.
        - **Detectability**: hybrid — the redundant-witness-param shape is statically recognizable; the one-impl-vs-many judgment is semantic.

        ### P-057-2: Project associated types instead of threading them
        - **Source**: BEP-57 (Associated Types for Interfaces, status: implemented)
        - **Principle**: Consumers name an implementor's type by projection — `S.Item` off a bound type variable — and disambiguate same-named members with the parenthesized `(Base as Interface).Item` cast (type-level), which is deliberately distinct from `upcast<I>(value)` (value-level).
        - **Intended usage**: `function first<I extends Iterator>(it: I) -> I.Item? { it.next() }`; `(CsvFile as Reader).Item` when two interfaces both declare `Item`.
        - **Anti-patterns / slop signals**:
          - Re-adding a type parameter that duplicates a projectable associated type.
          - Confusing levels: `upcast` used to name a type, or `(… as …)` used to widen a value.
        - **Applicability trigger**: generic functions bounded by associated-type interfaces.
        - **Detectability**: static — signature analysis.

        ### P-057-3: Interface-typed values must bind their associated types
        - **Source**: BEP-57 (Associated Types for Interfaces, status: implemented)
        - **Principle**: Holding a value at a bare associated-type interface erases the binding and makes `Item`-mentioning methods unresolvable — the type in a variable/parameter/LLM-return position must be fully bound, `Iterator<Item = int>` (Rust's `dyn Iterator<Item = u8>` rule).
        - **Intended usage**: `let it: Iterator<Item = int> = IntStack {...}`; LLM functions return only fully-bound forms; generic bounds (`<I extends Iterator>`) keep the concrete type and need no binding.
        - **Anti-patterns / slop signals**:
          - Falling back to concrete class types everywhere to dodge the binding syntax (losing the abstraction), when `Iterator<Item = X>` was the designed spelling.
        - **Applicability trigger**: variables/params typed at associated-type interfaces.
        - **Detectability**: static.

        ### P-062-1 [DRAFT]: Wrappers preserve the exact signature via `<T extends Function>` + spread
        - **Source**: BEP-62 (The Function Interface, status: proposed/draft — grade only for forward-alignment, don't penalize absence)
        - **Principle**: Signature-preserving decoration (retry/trace/cache/mock/middleware) should abstract over `Function` and reconstruct the callee via the spread identity `(...T.Params, ...T.OptionalParams) -> T.Returns throws T.Throws ≡ T`, rather than hand-writing per-arity wrappers.
        - **Intended usage**: `function traced<T extends Function>(name: string, f: T) -> T { ... }` with `defer` for cleanup on return/throw/panic.
        - **Anti-patterns / slop signals**:
          - Families of arity-specific wrappers (`traced1`, `traced2`, …) once this lands.
          - Wrappers that drop named/optional params or the throws channel when forwarding.
        - **Applicability trigger**: any higher-order wrapper around functions.
        - **Detectability**: hybrid.

        ### P-062-2 [DRAFT]: Wrappers advertise new failure modes honestly
        - **Source**: BEP-62 (The Function Interface, status: proposed/draft)
        - **Principle**: A wrapper that can fail in a way the wrapped function cannot must *not* claim `-> T`; it must return the widened `Function<..., Throws = T.Throws | Timeout>` so callers see and handle the new error. The principle generalizes to all of BAML's typed-throws discipline.
        - **Intended usage**: the BEP's `with_timeout` worked example — wrong version returns `-> T`, right version widens `Throws`.
        - **Anti-patterns / slop signals**:
          - Swallowing new failure modes inside a wrapper (catch-and-return-sentinel) to keep a narrow declared type.
          - Declaring `throws never` (or omitting throws) on code that can genuinely fail.
        - **Applicability trigger**: wrappers/adapters that introduce timeouts, IO, or validation around a callee.
        - **Detectability**: semantic — needs reading the wrapper body against its declared channel.

        ### P-062-3 [DRAFT]: `class` is the one data type; anonymous records are deliberately absent
        - **Source**: BEP-62 (The Function Interface, status: proposed/draft; the map-inference rule reflects current design stance)
        - **Principle**: BAML refuses first-class anonymous records because they are "a poor fit for agentic coding — implicit, easy to confuse with maps"; a bare `{ k: v }` literal is always a `map`, and named tuples exist *only* as the shape of a function's optional parameters, never as data modeling.
        - **Intended usage**: real data shapes are nominal `class`es; open key-value data is `map<K, V>`; named-tuple literals appear only where a function-params type flows in from context.
        - **Anti-patterns / slop signals**:
          - `map<string, unknown>` or parallel-array structures standing in for what should be a declared `class` (the "30+ Pair types" pain — ad-hoc record substitutes).
          - Treating `OptionalParams`-shaped values as general records (dotting into them, growing keys).
        - **Applicability trigger**: any structured value that isn't a class instance.
        - **Detectability**: hybrid — map-typed structured data is greppable; "should be a class" is judgment.

        ### P-062-4 [DRAFT]: Absence, null, and missing-map-key are three different things
        - **Source**: BEP-62 (The Function Interface, status: proposed/draft)
        - **Principle**: `baml.Unset` marks *only* named-tuple/optional-param absence (so "not passed" re-fires the callee's default), while `map.get` returns `V | null` and never throws; conflating them — or using `baml.Unset` as a general sentinel — is explicitly warned against ("I wonder if LLMs would just use `baml.Unset` everywhere").
        - **Intended usage**: `is null` narrowing for map lookups; `.has`/`is baml.Unset` only on named-tuple reads; `.unset(key)` to deliberately re-enable a callee default.
        - **Anti-patterns / slop signals**:
          - `baml.Unset` appearing outside optional-parameter plumbing.
          - `catch` around map lookups (a missing key is a value, not an error).
        - **Applicability trigger**: absence-handling code around maps and optional params.
        - **Detectability**: static — grep `baml.Unset` usage sites and `catch` around `.get`.

        ### P-012-1: LLM functions stay plain functions; low-level access uses `$` companions
        - **Source**: BEP-12 (Companion Functions, status: implemented)
        - **Principle**: Rather than compiling functions into callable classes (rejected: `type()` ambiguity, record-polluted autocomplete, "keep function semantics simple and explicit"), modular access to an LLM function's stages is exposed via compiler-generated `$`-qualified companions: `ExtractResume$render_prompt`, `$build_request`, `$parse`.
        - **Intended usage**: `ExtractResume$parse(raw)` to run parsing on captured output; `$render_prompt`/`$build_request` for inspection and tooling.
        - **Anti-patterns / slop signals**:
          - Hand-rolled prompt rendering or output parsing that duplicates what the function's own `$render_prompt`/`$parse` companions provide (e.g., re-parsing LLM JSON manually instead of `Fn$parse`).
          - Wrapping an LLM function in a class to hang helper methods off it — literally the rejected design.
        - **Applicability trigger**: code that inspects prompts, builds requests, or parses raw LLM output for a declared function.
        - **Detectability**: hybrid — manual JSON-parsing next to a declared LLM function is greppable; equivalence to a companion needs judgment.

        ### P-012-2: `$`-qualified names are compiler territory
        - **Source**: BEP-12 (Companion Functions, status: implemented)
        - **Principle**: `$` in an identifier is reserved for compiler/tooling-generated companion symbols; users cannot define them, keeping the escape-hatch channel unambiguous and extensible.
        - **Intended usage**: only *call* `$` symbols; never declare them.
        - **Anti-patterns / slop signals**:
          - Any user-declared `$` identifier (compile error, but generated/vendored code may attempt it).
        - **Applicability trigger**: any declaration containing `$`.
        - **Detectability**: static — trivial grep.

        ### P-008-1: Folders are organizational; namespaces are a deliberate opt-in
        - **Source**: BEP-8 (Namespaces, status: implemented)
        - **Principle**: Plain folders never affect scoping — only `ns_*` folders create namespaces — because the tax should fall on the uncommon case (semantic boundaries) not the common one (organizing files). "When an AI assistant creates a folder... it just creates a folder. The prefix is a speed bump that forces intentionality."
        - **Intended usage**: small/medium projects stay entirely in the root namespace with plain folders (`prompts/`, `types/`, `tests/`); `ns_llm/`, `ns_http/` appear only when name collisions or genuine boundaries demand them.
        - **Anti-patterns / slop signals**:
          - `ns_` folders used for mere file organization (a namespace per folder, mirroring a directory tree) — premature namespacing that reintroduces the folders-as-namespaces tax the design rejected.
          - A single-namespace project where symbols end up cross-qualified for no boundary reason.
        - **Applicability trigger**: project layout review; any `ns_*` folder.
        - **Detectability**: hybrid — folder census is static; whether a boundary is warranted is judgment.

        ### P-008-2: Prefix = external, no prefix = local — and no imports, ever
        - **Source**: BEP-8 (Namespaces, status: implemented)
        - **Principle**: Reading any line, a bare symbol is local to the namespace, `root.*` is elsewhere in this project, and any other prefix (`baml.*`, `sam.*`) is an external dependency — one hop, no import statements, no alias layer.
        - **Intended usage**: same-namespace symbols used bare across files; cross-namespace always `root.llm.Foo` (even from a nested child to its parent — nesting names, it does not scope); externals as `pkg.path.Symbol`.
        - **Anti-patterns / slop signals**:
          - Qualifying local symbols with `root.<own-namespace>.` (noise that breaks the "no prefix = mine" invariant).
          - Alias shims (`type Foo = root.llm.Foo` at file top) recreating import lists — the no-import principle stands against this.
          - Assuming a nested `ns_openai/` can see `ns_llm/` parent symbols unqualified.
        - **Applicability trigger**: every cross-namespace reference.
        - **Detectability**: static — resolution rules are mechanical.

        ### P-008-3: The tree tells the story; collisions are namespaces' job
        - **Source**: BEP-8 (Namespaces, status: implemented)
        - **Principle**: Namespaces exist to prevent naming conflicts and stage a package ecosystem — two namespaces may both define `Response`; within one namespace duplicates are errors — and `tree | grep ns_` should reveal the project's semantic map.
        - **Intended usage**: colliding domain names (`llm.Response` vs `http.Response`) resolved by namespace, not by name-mangling.
        - **Anti-patterns / slop signals**:
          - Prefix-mangled type names (`LlmResponse`, `HttpResponse2`) inside a single flat namespace once the project is big enough that a namespace boundary is the designed fix.
          - Namespace names shadowing root declarations or contorted names to dodge the rule.
        - **Applicability trigger**: repeated name-stem families across a large flat project.
        - **Detectability**: hybrid — name-stem clustering is greppable; the "should split" call is judgment.

        ### P-004-1 [PENDING]: Visibility ceremony is target-relative — apps shouldn't be littered with `public`
        - **Source**: BEP-4 (Inferred Visibility (public / private), status: pending — advisory, not yet enforced)
        - **Principle**: Unlabeled declarations default by build target (`app` → exported, `library` → private), because forcing per-declaration visibility decisions during exploration produces the known failure mode: "developers add `pub` liberally to make the compiler happy, intending to clean it up later. They usually don't."
        - **Intended usage**: app code leaves declarations unlabeled; library code marks the intended API `public` and lets inference promote signature-referenced types; `private(file)` for genuinely file-local helpers.
        - **Anti-patterns / slop signals**:
          - Blanket `public` on every declaration in an app target (ceremony the default already provides).
          - Library targets with no explicit `public` markings.
        - **Applicability trigger**: once BEP-4 lands — visibility keywords vs the target in `baml.toml`.
        - **Detectability**: static — count labeled vs unlabeled declarations against target mode.

        ### P-004-2 [PENDING]: Explicit `private` is a promise the compiler keeps
        - **Source**: BEP-4 (Inferred Visibility (public / private), status: pending)
        - **Principle**: Inference promotes only *unlabeled* types into a public signature's closure; an explicit `private` type in a `public` signature is a hard error, and private *fields* never constrain their type's visibility — plus the API lockfile (`baml api lock`/`diff`) catches accidental surface changes in CI.
        - **Intended usage**: mark a type `private` only when it must never appear in the exported API; keep internals out of public signatures; run `baml api diff` in CI.
        - **Anti-patterns / slop signals**:
          - "Fixing" a private-type-in-public-signature error by deleting `private` rather than restructuring the signature — silently breaking the encapsulation promise.
          - A perpetually-regenerated lockfile with no review (defeats the CI tripwire).
        - **Applicability trigger**: once BEP-4 lands — public signatures and lockfile churn.
        - **Detectability**: hybrid — the error-fix pattern shows in history; lockfile churn is static.
        # Batch: Control flow, patterns, cleanup & concurrency (BEPs 15, 11, 16, 42, 51, 41, 34, 31)

        Batch philosophy: BAML optimizes for TS/Python developers writing LLM orchestration code, so the *common case gets the least syntax* (bare type patterns, shorthand destructuring, uncolored functions, ambient cancellation), while ambiguity is always resolved by one explicit, greppable marker (`let` for bindings, `implements` for capabilities, `await` for suspension points). Safety is pushed into the runtime and compiler rather than convention — call-once cleanup latches, compile-enforced divergence in `let ... else`, exhaustiveness checking, error `cause` chains — so "slop" in BAML characteristically looks like hand-reimplementing a guarantee the language already provides (manual field extraction, hand-rolled retry loops, idempotency flags, sequential awaits of independent work). Rejected/superseded designs (BEP-011's original framing, BEP-016's arm-only catch binding, BEP-031's lazy dispatch) mark specific styles that code should *not* exhibit against the implemented semantics.

        ---

        ### P-015-1: Destructure in the pattern, not the arm body
        - **Source**: BEP-015 (Patterns, status: implemented)
        - **Principle**: Class and array patterns exist so that testing a type *and* extracting its contents is one step; manual field extraction, length checks, and indexing after a type match are exactly the boilerplate the feature was built to delete.
        - **Intended usage**: `match (val) { User { name, age } => greet(name, age); [] => fallback(); [first, ..rest] => processAll(first, rest) }` instead of matching `let u: User` then writing `let name = u.name`, or matching `User[]` then `if (len(users) == 0) ... users[0] ... users.slice(1)`.
        - **Anti-patterns / slop signals**:
          - A match arm binds the whole value (`let u: User =>`) and its body immediately extracts 2+ fields into locals.
          - Array arms containing `len(x) == 0` / `x[0]` / `x.slice(1)` logic that `[]`, `[first]`, `[first, ..rest]` patterns express directly.
          - Empty class destructure `User {}` (write `User =>`), all-wildcard destructure `User { name: _, age: _ }` (that's a type check, not a destructure).
        - **Applicability trigger**: any `match` over a class, union-of-classes, or array type; any arm body whose first statements are field/index accesses on the matched value.
        - **Detectability**: hybrid — grep/AST finds `let x: Type =>` arms followed by field extraction and `len(`/index idioms inside arms; LLM judgment confirms a pattern would have expressed it.

        ### P-015-2: Flatten value branching into arms (field literals + guards), not `if`/`else` inside arms
        - **Source**: BEP-015 (Patterns, status: implemented)
        - **Principle**: Field-literal patterns and guards let each arm declare exactly what it matches, so branching on field values belongs in additional arms, not nested conditionals inside one arm.
        - **Intended usage**: `User { name, role: "admin" } => grantAccess(name)` / `User { name, age } if age > 18 => greet(name)` / `User { name } => greetMinor(name)` — three arms instead of one arm with an `if`/`else if`/`else` ladder. Guards are only for comparisons a literal can't express: `User { age } if age == 10` is slop for `User { age: 10 }`.
        - **Anti-patterns / slop signals**:
          - A single match arm whose body is an `if`/`else if` chain over fields of the matched value.
          - Guards testing simple equality against a literal (`if x.f == "admin"`) where a field-literal pattern exists.
        - **Applicability trigger**: match arms containing conditional chains keyed on fields of the scrutinee; any guard of the form `if <binding> == <literal>`.
        - **Detectability**: hybrid — the `if binding == literal` guard is nearly static; nested-conditional-instead-of-arms needs semantic judgment about whether arms could express it.

        ### P-015-3: Minimal `let` — write it only where it disambiguates
        - **Source**: BEP-015 (Patterns, status: implemented)
        - **Principle**: The design rule is "`let` appears only when the pattern is otherwise ambiguous about intent" — field shorthand and array elements are already bindings; `let` is reserved for field rename, rebind-and-destructure, and top-level catch-all. The formatter canonicalizes to the minimum number of `let`s.
        - **Intended usage**: `User { name, age }`, `[first, ..rest]`, `User { age: let real_age }`, `let other =>`. Required-`let` sites are exactly: field rename, rebind+destructure (`scores: let foo: [...]`), top-level catch-all.
        - **Anti-patterns / slop signals** (all in the BEP's own "Legal but Cursed" list):
          - Redundant `let` everywhere: `User { let name, let age }`, `[let first, let second]`.
          - Alias chains (`let a: let b: let c: [...]`) — "if you need this, something upstream is wrong."
          - Rename to the same name: `User { age: let age }`.
          - `let _ =>` where `_ =>` suffices.
        - **Applicability trigger**: any pattern (match arm, `let` destructure, `if let`, `for`) containing `let` inside `{ }` or `[ ]`, or `let` not followed by a rename/rebind/catch-all.
        - **Detectability**: static — these forms are syntactically identifiable (and the formatter would strip them; unformatted-looking pattern code is itself a signal).

        ### P-015-4: Prefer flat refutable binding (`let ... else`) over nesting
        - **Source**: BEP-015 (Patterns, status: implemented)
        - **Principle**: `let P = e else { diverge }` exists so a refutable extraction doesn't force the rest of the function into an indented `if let` block; the BEP's explicit rule of thumb is "reach for `let ... else` before `if let`. Flat beats nested." The `else` block must diverge (compiler-enforced).
        - **Intended usage**: `let User { name } = val else { return }` followed by flat code; use `if let`/`while let` only when the success path is genuinely local to a block; use `match` only when there are multiple patterns to consider; use plain `let P = e` when the type guarantees the match (irrefutable).
        - **Anti-patterns / slop signals**:
          - `if let` whose body is the entire remainder of the function (should be `let ... else`).
          - A two-arm `match` where one arm diverges and the other holds all the logic (a `let ... else` in disguise).
          - Manual pre-checks (`if x is User`-style match) followed by a separate extraction, instead of one refutable binding.
        - **Applicability trigger**: any refutable extraction: `if let`, two-arm matches with a diverging arm, or type-check-then-extract sequences.
        - **Detectability**: hybrid — AST can flag `if let` spanning to end-of-function and diverging two-arm matches; judgment needed on whether the success path is "genuinely local."

        ### P-011-1: Bindings are always explicit (`let`); bare identifiers in patterns are always type lookups
        - **Source**: BEP-011 (Match with `let` bindings, status: **superseded** — but this core rule was carried into implemented BEP-015; graded as current semantics)
        - **Principle**: `let` is the single binding marker so that a typo'd or deleted type name is a compile error, never a silent catch-all — "no heuristics, no case-sensitivity tricks, no import-dependent behavior changes" (the "spooky action at a distance" every prior-art language rejected).
        - **Intended usage**: `Foo =>` is a type test; `let other =>` is the named catch-all; `let n: int =>` is bind-and-narrow. Match *and* `catch` share this one pattern language.
        - **Anti-patterns / slop signals**:
          - Code written as if bare lowercase identifiers were catch-alls (Python/Rust habit) — e.g. relying on naming convention to distinguish types from bindings.
          - Writing `let _: Type` where bare `Type` is the preferred form (BEP calls the bare form preferred).
          - Simulating a named catch-all by re-referencing the scrutinee with awkward casts instead of `let other`.
        - **Applicability trigger**: every `match` and `catch` block.
        - **Detectability**: static — `let _: T` vs `T`, and binder usage, are pure syntax.

        ### P-011-2: Order arms specific-to-general and lean on exhaustiveness; guarded arms need an unguarded fallback
        - **Source**: BEP-011 (Match with `let` bindings, status: superseded — semantics carried forward)
        - **Principle**: Matches read top-to-bottom, literals/variants before their enclosing types, catch-all last; exhaustiveness is a compiler contract (adding an enum variant should break non-exhaustive matches), and guards deliberately don't count toward it.
        - **Intended usage**: enumerate all variants of a closed union/enum *without* a catch-all when you want the compiler to flag future additions; add `Success =>` after `let s: Success if s.score > 0.9 =>` to complete coverage.
        - **Anti-patterns / slop signals**:
          - Reflexive `let _ =>`/`_ =>` catch-alls on closed enums/unions, defeating exhaustiveness checking (the BEP's "new variant forces the developer to handle it" benefit).
          - Redundant unreachable arms retained "for safety."
          - Guard ladders with no unguarded same-type fallback pushed into a catch-all instead.
        - **Applicability trigger**: any `match`/`catch` over an enum or closed union.
        - **Detectability**: hybrid — catch-all-on-closed-union is statically detectable; whether the catch-all is *warranted* (genuinely open data, e.g. LLM output) needs judgment.

        ### P-016-1: Use the `catch (e)` binding and per-arm narrowing as implemented — the arm-only-binding style was rejected
        - **Source**: BEP-016 (Catch Binding Syntax, status: **rejected** — the rejected alternative is the anti-pattern)
        - **Principle**: BEP-016 proposed dropping the top-level `catch (e)` binding in favor of re-binding on every arm; its rejection affirms the implemented design: one common error binding, type-narrowed per arm, syntactically parallel to `match`.
        - **Intended usage**: `expr catch (e) { TimeoutError => retry(e.retryAfterMs), let other => log(other.message) }` — use `e` narrowed in each arm; add a per-arm rebinding only when it genuinely reads better.
        - **Anti-patterns / slop signals**:
          - Rebinding on every arm (`num: int => ...`, `text: string => ...`) while never using the top-level `e` — recreating the rejected design and adding noise.
          - Ignoring narrowing: casting or re-checking `e`'s type inside an arm that already narrowed it.
        - **Applicability trigger**: any `catch` expression with typed arms.
        - **Detectability**: static — unused top-level binding plus universal per-arm rebinds, or redundant type re-checks inside narrowed arms, are AST-visible.

        ### P-042-1: Declare `defer` adjacent to acquisition; never place cleanup after the use site
        - **Source**: BEP-042 (Cleanup Semantics: defer, finally, and Destructors, status: implemented)
        - **Principle**: `defer` exists because cleanup written after a fallible use never runs when a `catch` rethrows (the "rethrow problem"); scheduling cleanup at scope exit next to acquisition guarantees it on normal return, error, and panic alike.
        - **Intended usage**: `let f = fs.open("out.txt"); defer { f.close() }` immediately after acquiring; block scoping means `defer` inside a loop body correctly runs per iteration (unlike Go) — no hoisting needed.
        - **Anti-patterns / slop signals**:
          - `f.close()` / `resource.release()` as a trailing statement after code that can throw, with no `defer`.
          - Duplicated cleanup calls on both the success path and inside every catch arm (hand-rolled `finally`).
          - Pushing cleanup to the host language because the BAML scope "can't" guarantee it.
          - Go-style hoisting of resource acquisition out of a loop purely to avoid defer accumulation (unnecessary — BAML defer is block-scoped).
        - **Applicability trigger**: any code acquiring a closable resource — `fs.open`, connections, spawned tasks, anything implementing `Cleanup`.
        - **Detectability**: hybrid — grep finds `.close()`/`.release()`/`.cleanup()` outside `defer` blocks and duplicated cleanup in catch arms; judgment needed for whether intervening code can throw.

        ### P-042-2: `Cleanup` is a nominal, opt-in safety net — `defer` for determinism, `implements baml.gc.Cleanup` for "if I forget"
        - **Source**: BEP-042 (Cleanup Semantics, status: implemented)
        - **Principle**: The two mechanisms are tiered by guarantee: `defer` gives deterministic scope-exit cleanup; the `Cleanup` interface gives a non-deterministic GC-time backstop. Finalization is nominal (`implements`, per BEP-044) precisely so a method merely *named* `cleanup` never silently acquires GC machinery.
        - **Intended usage**: resource-holding classes write `implements Cleanup { function cleanup(self) {...} }`, and call sites still `defer { obj.cleanup() }` for deterministic release; the greppable `implements baml.gc.Cleanup` enables tooling ("constructed a Cleanup value, never deferred it").
        - **Anti-patterns / slop signals**:
          - Code that *depends* on GC timing to release a file/connection promptly (constructing a `Cleanup` value and never deferring/explicitly cleaning it on a hot path).
          - A resource-holding class with an ad-hoc `close()`/`dispose()` method and no `implements Cleanup` — forgoing the backstop the interface was built for.
          - A class with a method named `cleanup` used as an ordinary reusable operation while *also* implementing `Cleanup` (the latch makes second calls no-ops; the BEP says give reusable operations a different name).
        - **Applicability trigger**: any class holding files, connections, tasks, or other non-memory resources; any explicit `.cleanup()` call site.
        - **Detectability**: hybrid — `implements Cleanup` presence and defer pairing are greppable; "does this class hold a resource" is semantic.

        ### P-042-3: Don't hand-roll call-once guards or finalizer deregistration — the runtime latch owns idempotency
        - **Source**: BEP-042 (Cleanup Semantics, status: implemented; the latch was added specifically in response to a comment about double-run via defer + GC)
        - **Principle**: `cleanup` runs at most once — atomically latched by the runtime across explicit calls, defers, and GC — so idempotency is "a guarantee, not a style request," and Go-style `SetFinalizer(obj, nil)` deregistration bookkeeping has no equivalent and no purpose.
        - **Intended usage**: write a non-idempotent `cleanup` body freely (return connection to pool, decrement counter); pair `defer { pool.cleanup() }` with the GC backstop with zero extra coordination.
        - **Anti-patterns / slop signals**:
          - `is_closed`/`cleaned` boolean flags checked at the top of a `cleanup` body — reimplementing the runtime latch.
          - Defensive "only cleanup if not already cleaned" checks at call sites before `obj.cleanup()`.
          - Any manual registration/deregistration scaffolding around finalization.
        - **Applicability trigger**: bodies of `cleanup(self)` implementations and their call sites.
        - **Detectability**: static-leaning hybrid — boolean-latch fields named `closed`/`cleaned`/`disposed` guarding cleanup bodies are grep-detectable; occasionally such a flag serves an unrelated purpose.

        ### P-042-4: Let errors chain — don't swallow defer errors or stringify causes
        - **Source**: BEP-042 (Cleanup Semantics, status: implemented)
        - **Principle**: A throw during unwinding automatically becomes the propagating error with the prior error preserved as a structured `cause` — designed explicitly so no error is silently lost and so the superseded error stays a first-class value rather than trace text (the rejected `finally` masks; the rejected v1 "replace" demoted to text).
        - **Intended usage**: `catch (e, ctx)` and walk `ctx.cause` / `ctx.root_cause()` when the original failure matters (LIFO buries the root — `root_cause()` exists precisely for that); `ctx.to_string()` for full-chain rendering.
        - **Anti-patterns / slop signals**:
          - Blanket `catch { _ => {} }` inside every `defer` body just to "avoid masking" the primary error — unnecessary; chaining already preserves both (Swift/Zig's forbid-throwing-defer style, which the BEP explicitly rejected for BAML).
          - Concatenating error messages into strings to preserve a "cause" (`throw Error(msg + "; caused by " + e.message)`).
          - Handlers that only ever inspect `e` when the logic clearly wants the original body error (`root_cause()`).
        - **Applicability trigger**: `defer` bodies containing catches; error handlers that reconstruct causal history manually.
        - **Detectability**: semantic — needs LLM judgment on whether a swallow/string-concat is defensive slop versus a genuine decision to discard.

        ### P-051-1: Compose with lazy iterator adapters; implement the `Iterator`/`Iterable` interfaces for custom sequences
        - **Source**: BEP-051 (Iterators, status: implemented)
        - **Principle**: Iterators are pull-based and lazy so that `.map()/.filter()/.chain()` compose without materializing intermediates, unifying collections and streams under one protocol — and so `for`-in works over *user* types via `implements Iterable`.
        - **Intended usage**: chain stdlib adapters (`map`, `filter`, `filter_map`, `flatten`, `chain`, `step_by`, `peekable`) and terminals (`reduce`, `find`, `count`, `collect`); a custom stream/pagination source is a class with `implements Iterator<T, E>` (and `Iterable` returning `self`), consumed by ordinary `for (let item in it)`.
        - **Anti-patterns / slop signals**:
          - Hand-written index loops (`while (i < len(arr)) { ... arr[i] ... i += 1 }`) where `for`-in or an adapter chain does the job.
          - Reimplementing `filter`+`next` as a manual search loop instead of `find`; manual accumulate loops instead of `reduce`.
          - A custom "stream" exposed as `get_next_or_null()` / callback methods instead of implementing the `Iterator` interface — fighting the protocol the BEP exists to unify.
          - Eagerly materializing (`collect`) between every adapter step.
        - **Applicability trigger**: any loop over a collection; any user type representing a sequence, stream, or paginated source.
        - **Detectability**: hybrid — index-loop and manual-find shapes are AST-detectable; "should this class have been an Iterator" is semantic.

        ### P-051-2: Iterators are single-consumer; one outstanding `next()` at a time
        - **Source**: BEP-051 (Iterators, status: implemented)
        - **Principle**: `next()` mutates the iterator, so at most one call may be outstanding; concurrent `spawn { it.next() }` from multiple places on one iterator is a declared data race. Concurrency composes with iteration via green threads (`spawn { it.next() }` *once*, as a Future), not via sharing.
        - **Intended usage**: sequential `for`-in, or a single `spawn { it.next() }` to overlap waiting for the next item with other work.
        - **Anti-patterns / slop signals**:
          - Multiple simultaneous spawned `next()` calls on the same iterator (e.g. fan-out workers all pulling one shared iterator).
          - Sharing one iterator across tasks as a makeshift work queue.
        - **Applicability trigger**: any code where an iterator value crosses a `spawn` boundary or is stored where multiple tasks reach it.
        - **Detectability**: hybrid — `spawn`-wrapped `next()` on a shared binding is greppable; aliasing analysis of "same iterator" needs judgment.

        ### P-051-3: Respect the fallibility and termination contract of `next()`
        - **Source**: BEP-051 (Iterators, status: implemented)
        - **Principle**: `next()` returns `T | baml.iter.Done` and `throws E` — end-of-iteration is a sentinel value, errors are real throws that flow into the enclosing function's inferred `throws` type; and unbounded terminals (`count`, `collect`, `reduce`) are not safe on infinite iterators (`Repeat` default, unbounded sources).
        - **Intended usage**: let `for`-in handle `Done`; let `throws` inference carry `E`; annotate the loop binding (`for (let item: int in it)`) only when multiple `Iterator` implementations make it ambiguous; bound infinite iterators before a consuming terminal.
        - **Anti-patterns / slop signals**:
          - Encoding errors as in-band sentinel values from `next()` (returning `null`/error objects as items) instead of throwing.
          - Catch-and-swallow around every `next()`/loop to keep the function's `throws` clean, rather than letting the union propagate.
          - Calling `count`/`collect` on an obviously unbounded iterator (`Repeat` with default count).
        - **Applicability trigger**: custom `Iterator` implementations and consuming terminal calls.
        - **Detectability**: hybrid — `Repeat` + terminal is near-static; in-band error encoding needs semantic review.

        ### P-034-1: No function coloring — concurrency is a call-site decision, never an API split
        - **Source**: BEP-034 (Concurrency via spawn/await, status: **proposed**; core stance shared with superseded BEP-031)
        - **Principle**: `spawn`/`await` are ordinary expressions; a function's signature reflects what it returns and throws, not how it's scheduled — declared a "non-negotiable constraint" specifically to avoid viral async annotation and sync/async duplicate APIs.
        - **Intended usage**: any function may `spawn` internally without changing its signature; return `Future<T, E>` only as a deliberate API choice to hand the caller timing control (`kick_off`-style), not because the language forces it.
        - **Anti-patterns / slop signals**:
          - Parallel `foo` / `foo_async` function pairs, or `_sync` wrappers — recreating the bifurcated stdlib the design forbids.
          - Every function in a call chain returning `Future<T, E>` and every caller immediately awaiting — cargo-culted coloring with no timing-control intent.
        - **Applicability trigger**: module-level API surface: function names and return types involving `Future`.
        - **Detectability**: static-leaning hybrid — name-pair and return-`Future`-then-immediately-awaited patterns are greppable; intent behind a returned Future needs judgment.

        ### P-034-2: Spawn independent slow work; don't serialize what has no data dependency
        - **Source**: BEP-034 (Concurrency via spawn/await, status: proposed)
        - **Principle**: The motivating workload is multiple independent IO/LLM steps; without `spawn`, a pipeline pays the *sum* of latencies instead of the max — concurrency inside BAML is the feature's entire reason to exist (versus dropping to host-language `gather`/`Promise.all` glue).
        - **Intended usage**: `let names = spawn { ExtractNames(doc) }; let dates = spawn { ExtractDates(doc) }; ... await names, await dates` — spawn all, then await; name spawns (`spawn "extract-names" {...}`) for traces; `items.map(x -> spawn { process(x) })` is fine (green threads are cheap).
        - **Anti-patterns / slop signals**:
          - Sequential calls to 2+ independent LLM/HTTP functions in one body (the exact before-picture in the BEP).
          - `await` immediately following each `spawn` in a loop or sequence (`await spawn {...}; await spawn {...}`) — spawn syntax with fully sequential semantics.
          - Orchestrating BAML-function parallelism from host-language glue when it could live in BAML.
        - **Applicability trigger**: any function body with multiple calls to LLM functions, `baml.http.fetch`, or other slow IO.
        - **Detectability**: hybrid — `await spawn` adjacency is static; "are these calls independent" is semantic.

        ### P-034-3: Use stdlib combinators and `.then()` normalization, not hand-rolled coordination
        - **Source**: BEP-034 (Concurrency via spawn/await, status: proposed)
        - **Principle**: `baml.future.race/any/all/all_complete/all_settled` encode the coordination shapes (with the BEP's explicit guidance that `any`, not `race`, is what you want for LLM fan-out — a fast error kills a `race`), and `.then()` exists to normalize heterogeneous futures so combinators and unions stay on domain types.
        - **Intended usage**: `await baml.future.any([openai, anthropic, google])` for first-success; `all_settled` when no failure should throw; `.then((r) -> r.content)` to make heterogeneous provider futures a common `Future<string, ...>` before combining; `await` distributes over future unions so you match on `OpenAIResponse | AnthropicResponse`, never on `Future<...>` container types.
        - **Anti-patterns / slop signals**:
          - Manual polling loops over `is_settled()` or ad-hoc first-success logic instead of `any`.
          - `race` used for LLM provider fan-out where first *success* is wanted.
          - Matching on future container types instead of awaiting the union or normalizing with `.then()`.
          - Collecting results by awaiting in a loop while separately tracking failures — reinventing `all_settled`.
        - **Applicability trigger**: any code holding 2+ futures.
        - **Detectability**: hybrid — `race(` on provider fan-outs and `is_settled` polling loops are greppable; "which combinator was intended" needs judgment.

        ### P-034-4: Never let a wildcard catch swallow `Cancelled`
        - **Source**: BEP-034 (Concurrency via spawn/await, status: proposed)
        - **Principle**: `Cancelled` is deliberately a *panic*, not an error, precisely so `catch (e) { let e => fallback }` wildcards don't silently convert a cancellation into a fallback value and keep the pipeline running; cancellation is ambient and observed only at `await` points (greppable suspension points, no ctx plumbing).
        - **Intended usage**: let `Cancelled` propagate through wildcard catches automatically; opt in explicitly with `let c: baml.panics.Cancelled => ...` only where a cancellation-specific response is genuinely wanted; no hand-plumbed context/abort-signal parameters — parent→child cascade comes free via token composition.
        - **Anti-patterns / slop signals**:
          - Catching `baml.panics.Cancelled` broadly and returning a normal value (converting cancel into fake success).
          - Go-style manual cancel-token parameters threaded through functions that never use them.
          - Hand-built cancellation flags checked in loops where ambient `await`-point delivery already applies.
        - **Applicability trigger**: catch arms mentioning `Cancelled`; function signatures carrying token/context parameters.
        - **Detectability**: static-leaning hybrid — explicit `Cancelled` arms and token-shaped parameters are greppable; whether a Cancelled handler is legitimate needs judgment.

        ### P-034-5: Rate limiting is a shared `TaskGroup` value; cross-cutting spawn concerns go in `with` middleware
        - **Source**: BEP-034 (Concurrency via spawn/await, status: proposed)
        - **Principle**: `TaskGroup` is a value — two spawns share a limit iff they reference the same object — because a global name-keyed registry creates action-at-a-distance, last-writer-wins limits, and test-isolation failures; and `with withRetry(3), withTimeout(...)` exists so retry/timeout/limit logic is declared at the spawn site instead of reimplemented inside every body.
        - **Intended usage**: `let group = TaskGroup.new(limit = 5)` passed via `baml.spawn.options(group = group)`; `spawn "extract" with withRetry(3), withTimeout(Duration.seconds(10)) { ... }`.
        - **Anti-patterns / slop signals**:
          - Hand-rolled retry loops or sleep-based backoff inside spawn bodies.
          - Manual concurrency throttling (chunking an array and awaiting batch-by-batch) instead of one `TaskGroup`.
          - Simulating a global registry: stashing a `TaskGroup` under a string key in module-global state so unrelated modules share it implicitly.
        - **Applicability trigger**: spawn bodies containing retry/timeout/throttle logic; fan-outs over arrays with manual batching.
        - **Detectability**: hybrid — retry loops and batch-await shapes are AST-recognizable; distinguishing legitimate custom policies needs judgment.

        ### P-034-6: Fire-and-forget must be `detach`ed and self-error-handling
        - **Source**: BEP-034 (Concurrency via spawn/await, status: proposed)
        - **Principle**: Dropping a future is legal, but an unhandled error in a fire-and-forget task propagates to the parent's next `await`; work meant to outlive or not disturb the parent (telemetry, background warming) must use `detach = true` and catch its own errors — the BEP labels this the "safe pattern."
        - **Intended usage**: `spawn "telemetry" with baml.spawn.options(detach = true) { emit_metric(...) catch (e) { let e => baml.log.warn("telemetry failed") } }`.
        - **Anti-patterns / slop signals**:
          - A dropped (never-awaited, never-bound) spawn whose body can throw and has no internal catch.
          - Background/side-effect spawns without `detach = true`, so a parent crash cancels them or their failures poison unrelated awaits.
        - **Applicability trigger**: any `spawn` whose future is discarded.
        - **Detectability**: static-leaning hybrid — discarded spawn results are AST-detectable; whether the body can throw needs type/semantic info.

        ### P-031-1: Do not write code assuming lazy dispatch — `spawn` starts eagerly *(anti-pattern from superseded BEP)*
        - **Source**: BEP-031 (Async IO via Stackful Coroutines, status: **superseded** by BEP-034 — the superseded semantics are the anti-pattern)
        - **Principle**: BEP-031's lazy dispatch (body doesn't run until an `await`/yield point, giving the caller an "exclusive access window") was explicitly reversed in BEP-034 decision #2: eager dispatch was chosen because lazy makes side-effect timing depend on caller control flow. Code relying on the lazy model is wrong under implemented semantics.
        - **Intended usage**: treat `spawn { body }` as beginning execution at the spawn site; if a spawned body shares mutable state with the spawner, that's a race *from the moment of spawn*, not from the first await.
        - **Anti-patterns / slop signals**:
          - Mutating a captured array/object between `spawn` and `await` on the assumption the body hasn't started (BEP-031's Example 3 reasoning).
          - Comments or logic expecting IO to fire "when awaited" — e.g. deliberately delaying an `await` to sequence side effects.
          - Two in-flight tasks read-modify-writing shared state across await points (the JS-style interleaving bug both BEPs acknowledge the language does not prevent).
        - **Applicability trigger**: code touching variables captured by a spawn body between the spawn and its await; ordering-sensitive side effects around spawns.
        - **Detectability**: semantic — requires reasoning about shared-state access windows; grep can shortlist writes to spawn-captured bindings.

        ### (BEP-041 — Async Iterator Protocol, status: rejected)
        No principles: the document is an explicit CLI-tooling test artifact ("should be disregarded") with no design content.
        # Batch: Type system & data (BEPs 21, 22, 38, 39, 43, 55, 13, 29, 5)

        Batch philosophy: BAML deliberately ships **purpose-built, semantically precise types** (Temporal-style time types, `bigint`, `json`, opaque `type`, range types) so that users never have to encode domain semantics in weaker primitives like `int`, `string`, or hand-rolled helper functions — and it aligns those types with battle-tested prior art (TC39 Temporal, Go's `reflect.TypeFor`, serde) rather than inventing new taxonomies. Conversions between representations are always **explicit and often fallible** (`.to_json()`, `to_seconds() throws Overflow`, `parse` routing by string shape), never implicit coercions, because implicit conversion hides error origins and defeats the type system's contracts. Cross-cutting decisions — snake_case stdlib naming, IO-swappable clocks, snapshot-by-value host boundaries, opt-in deep immutability — all favor checkable contracts over convention, which makes "fighting the design" (reimplementing stdlib features by hand, string-typing timestamps, dummy `$parse` functions) highly detectable slop.

        ---

        ### P-021-1: Use the Temporal taxonomy, not raw numbers or strings, for time
        - **Source**: BEP-021 (Dates and Times, status: implemented)
        - **Principle**: BAML adopts TC39 Temporal's type taxonomy wholesale — `Instant` (absolute), `ZonedDateTime` (absolute + timezone), `PlainDateTime`/`PlainDate`/`PlainTime` (civil, no timezone), `Duration`, `TimeZoneOffset` — because Temporal's split is the product of years of committee review of DST/civil-time pitfalls, and each type encodes which operations are meaningful.
        - **Intended usage**: Any timestamp, date, wall-clock reading, or time span should be held in the matching `baml.time` type: `Instant.now()`, `Instant.parse("2026-03-18T20:04:27Z")`, `instant + Duration.from_hours(2)`, `PlainDateTime.parse("1979-05-27T07:32:00").to_zoned("America/Los_Angeles")`.
        - **Anti-patterns / slop signals**:
          - Storing timestamps as bare `int`/`bigint` epoch values or ISO strings in class fields and doing arithmetic on them directly.
          - Hand-rolled date parsing/formatting (string slicing on ISO strings, `split("-")` on dates).
          - Using `Instant` (absolute) where the domain quantity is a civil date/time (e.g. a birthday), or vice versa.
          - Reimplementing `Duration` as an `int` of seconds/millis with magic-constant multiplication scattered around.
        - **Applicability trigger**: any chunk that creates, parses, compares, or does arithmetic on timestamps, dates, times, or time spans.
        - **Detectability**: hybrid — grep for epoch-int fields, ISO-string regexes, and `* 1000`/`* 3600` magic constants near time-named variables; LLM judgment to confirm the value is semantically a time.

        ### P-021-2: Timezone-aware values are absolute-time-backed; never do component-wise wall-clock math
        - **Source**: BEP-021 (Dates and Times, status: implemented)
        - **Principle**: `ZonedDateTime` is internally an `Instant` plus a timezone precisely because year/month/day/hour decomposition is ambiguous at DST boundaries (times that occur twice or never). Arithmetic on `ZonedDateTime` is defined over absolute time; timezone resolution happens only at the format/component boundary.
        - **Intended usage**: `zdt + Duration.from_hours(1)` for arithmetic; `zdt.with_timezone(tz)` to re-view the same absolute time; `from_components(..., disambiguation: "compatible")` / `PlainDateTime.to_zoned(tz, disambiguation)` when locating a wall-clock reading, consciously choosing a DST disambiguation mode.
        - **Anti-patterns / slop signals**:
          - Manually adding/subtracting a fixed offset to "convert timezones" instead of `with_timezone`/`timezone_offset()`.
          - Decomposing into components, mutating `hour`/`day` fields, and reassembling — reintroducing the DST ambiguity the type exists to prevent.
          - Hard-coded offset constants (e.g. `-7 * 3600`) for named zones instead of IANA identifiers.
        - **Applicability trigger**: any chunk that converts between timezones, adds calendar-ish quantities to times, or handles localized display.
        - **Detectability**: hybrid — offset-constant arithmetic is greppable; whether component reassembly is a DST hazard needs LLM judgment.

        ### P-021-3: Parse routing by string shape: offset-carrying → `Instant`/`ZonedDateTime`, zoneless → `Plain*`
        - **Source**: BEP-021 (Dates and Times, status: implemented)
        - **Principle**: `Instant.parse` and `ZonedDateTime.parse` accept only strings that carry a UTC offset (RFC 3339/ISO 8601/RFC 2822/RFC 9557) and reject zoneless strings, which belong to `PlainDateTime.parse`. This partition prevents silently assigning a wrong timezone to a civil reading.
        - **Intended usage**: pick the parse entry point by what the input actually contains; if the source data is zoneless, parse as `PlainDateTime` and call `.to_zoned(tz)` with an explicit timezone decision.
        - **Anti-patterns / slop signals**:
          - String-munging to defeat the router: appending `"Z"` or `"+00:00"` to zoneless input so `Instant.parse` accepts it (silently asserts UTC).
          - Catching `ParseError` and falling back through parse functions blindly instead of knowing the input's shape.
        - **Applicability trigger**: any chunk calling a time-type `parse` or preprocessing a string before doing so.
        - **Detectability**: static-leaning hybrid — string concatenation of `"Z"`/offset suffixes immediately before `Instant.parse` is greppable; intent confirmation is semantic.

        ### P-021-4: Lossy/fallible conversions are explicit; clock and system-timezone access is swappable IO
        - **Source**: BEP-021 (Dates and Times, status: implemented)
        - **Principle**: Converting time to numeric quantities can overflow and is declared `throws ConversionError.Overflow`; `now()`, `TimeZoneOffset.local()`, and `system_timezone()` are deliberately IO functions so hosts can swap the clock for deterministic tests.
        - **Intended usage**: handle or propagate conversion errors consciously; treat `now()` as an injectable effect — deterministic tests should be able to run without real wall-clock dependence.
        - **Anti-patterns / slop signals**:
          - Swallowing `Overflow`/`ParseError` with a silent default value.
          - Sprinkling `Instant.now()` deep inside pure computation logic instead of taking a time parameter, making the function untestable.
        - **Applicability trigger**: any chunk calling `now()`/`local()`/`to_*seconds` or converting time types to numbers.
        - **Detectability**: semantic — needs judgment about whether the error handling and `now()` placement are principled.

        ### P-022-1: Use `bigint` where range exceeds `i64`; lean on `int <: bigint` subtyping
        - **Source**: BEP-022 (bigint, status: implemented)
        - **Principle**: `bigint` exists to represent arbitrary-precision integers (its motivating case is nanosecond timestamps) with the same arithmetic semantics as `int`; `int` is a subtype of `bigint`, so mixed `bigint x int` arithmetic is directly supported and `let x: bigint = 123` is valid without the `n` suffix.
        - **Intended usage**: declare `bigint` for quantities that can exceed ±2^63 (nanosecond epochs, cryptographic values, unbounded counters); write mixed arithmetic naturally (`ns * 1000`, no casts); use `123n` literals when a literal must be bigint-typed.
        - **Anti-patterns / slop signals**:
          - Representing large integers as strings and doing string-based arithmetic or comparison.
          - Using `float` for large integer quantities (silent precision loss beyond 2^53).
          - Manual overflow-chunking (hi/lo pairs of `int`s) to simulate wide integers.
          - Redundant explicit casts/wrappers when passing `int` where `bigint` is accepted.
        - **Applicability trigger**: any chunk handling values plausibly outside `i64` range — timestamps in ns, IDs from external systems, accumulating products/powers.
        - **Detectability**: hybrid — hi/lo pair fields and string-number arithmetic are greppable; whether a quantity can overflow `i64` needs domain judgment.

        ### P-022-2: `bigint` serializes as a JSON number, and prefer its infallible/checked methods
        - **Source**: BEP-022 (bigint, status: implemented)
        - **Principle**: BAML always serializes `bigint` as a JSON number (chosen for LLM prompt readability), deserializes numbers of any size (and, via SAP, strings) into `bigint`, and provides methods (`abs` infallible, `parse`, `pow`, `isqrt`, `random`) mirroring `int`'s — while deliberately excluding bit-counting operations because they are undefined for arbitrary-width two's complement.
        - **Intended usage**: rely on the built-in serde behavior for `bigint` fields in LLM function inputs/outputs; use `bigint.parse` for textual big numbers; use `.abs()` freely (it cannot fail, unlike `int.abs()` on `i64::MIN`).
        - **Anti-patterns / slop signals**:
          - Manually stringifying `bigint` fields before serialization "for safety" inside BAML code.
          - Reimplementing `parse`/`pow`/`isqrt` for big numbers.
          - Attempting bit-count logic (`count_ones`-style loops) on `bigint`, which the design intentionally excludes.
        - **Applicability trigger**: any chunk serializing `bigint` values or doing numeric methods on them.
        - **Detectability**: static-leaning hybrid — manual `to_string()` before serialization and hand-rolled numeric loops are greppable.

        ### P-038-1: Use the `json` type for arbitrary/external JSON — never dummy `$parse` functions, hand-rolled escapers, or throwaway wrapper classes
        - **Source**: BEP-038 (Native json Type, status: implemented)
        - **Principle**: `json` (`type json = null | bool | int | float | string | json[] | map<string, json>`) exists specifically because projects without it independently reinvented three workarounds: dummy LLM functions declared solely to reach `$parse`, hand-written `json_escape`/`json_str` helper suites, and full wrapper-class hierarchies to extract a few fields from external APIs. The stdlib (`baml.json.parse/stringify/encode/decode/decode_str`) replaces all three.
        - **Intended usage**:
          ```baml
          let result: EmbeddingResponse = baml.json.decode_str<EmbeddingResponse>(resp.text())
          let body = baml.json.stringify({"input": text, "model": "text-embedding-3-small"})
          ```
        - **Anti-patterns / slop signals**:
          - An LLM `function` with an empty `#""#` prompt and a real `client` whose only call sites are `Fn$parse(...)` — the canonical pre-BEP-038 hack. (Note: the *testing* pattern of feeding captured JSON through a real function's `$parse` is legitimate; the slop is a *dummy function declared only for parsing*.)
          - User-defined `json_escape`, `json_str`, `json_float_array`, or manual string concatenation building JSON bodies.
          - Classes declared solely to reach one nested field of a third-party API response when `json` narrowing would do.
        - **Applicability trigger**: any chunk that builds or consumes JSON for non-BAML APIs (HTTP bodies, external responses).
        - **Detectability**: static — empty-prompt functions, `escape`-named string helpers, and `"\""+ ... +"\""` concatenation are all greppable; single-use wrapper classes need light semantic confirmation.

        ### P-038-2: Conversion into/out of `json` is always explicit
        - **Source**: BEP-038 (Native json Type, status: implemented)
        - **Principle**: Values enter and leave `json` only through explicit calls — `.to_json()`, `baml.json.encode(v)`, `baml.json.decode<T>(j)` — never implicit coercion, because implicit conversion obscures where errors originate and would set a coercion precedent BAML doesn't have.
        - **Intended usage**: `let j: json = user.to_json()`; override `to_json`/`from_json` on a class when serialization must skip/rename/transform fields (e.g. redacting `ssn`); rely on auto-derivation for plain data shapes.
        - **Anti-patterns / slop signals**:
          - Writing manual field-by-field map-building (`{"name": u.name, "age": u.age}`) when the auto-derived `to_json` produces exactly that.
          - Custom `from_json` that duplicates the default structural decode with no behavioral difference.
        - **Applicability trigger**: any chunk converting classes to/from `json` values.
        - **Detectability**: semantic — requires comparing hand-written conversion bodies against what auto-derivation would produce.

        ### P-038-3: Narrow `json` with pattern matching, not blind assumptions
        - **Source**: BEP-038 (Native json Type, status: implemented)
        - **Principle**: BEP-015 pattern matching was designed with `json` as its primary use case; narrowing a `json` value to a concrete shape should go through `match` or `let ... else`, which gives exhaustive, typed control flow for free.
        - **Intended usage**:
          ```baml
          let obj: map<string, json> = baml.json.parse(body) else { throw "not an object" }
          let items: json[] = obj["items"] else { throw "items not an array" }
          ```
        - **Anti-patterns / slop signals**:
          - Stringifying a `json` value and re-parsing or regexing it to inspect its shape.
          - Passing unnarrowed `json` deep through call chains that then assume a shape without a match.
        - **Applicability trigger**: any chunk that reads structure out of a `json` value.
        - **Detectability**: hybrid — `stringify` immediately followed by string inspection is greppable; unnarrowed-assumption flows need LLM tracing.

        ### P-038-4: `json` is for external data interchange; typed schemas remain the default — don't sprinkle `json` everywhere
        - **Source**: BEP-038 (Native json Type, status: implemented; concern raised explicitly in comments)
        - **Principle**: `json` is distinct from `unknown` (external-interchange subset vs internal type-erasure) and is a *deliberate* choice for genuinely arbitrary shapes. The comment thread flags the exact slop risk: "i do really wonder if llms will just sprinkle `json` everywhere. So we just have to watch out for this." An LLM function returning `json` gets no schema in `ctx.output_format` and forfeits SAP's schema-aware parsing.
        - **Intended usage**: declare `-> json` on an LLM function only when the output shape is truly open-ended; use `json` fields only where the payload is genuinely dynamic; use `unknown` only for internal type-erased containers, never for API payloads.
        - **Anti-patterns / slop signals**:
          - LLM functions returning `json` when a concrete class/enum schema is knowable — throws away BAML's core structured-output value.
          - `json`-typed fields whose contents are always the same shape at every call site.
          - Using `unknown` for external JSON data (wrong tool; `json` is the sound subset).
        - **Applicability trigger**: any declaration using `json` or `unknown` in a signature or field, especially LLM function return types.
        - **Detectability**: semantic — a reviewer must judge whether the shape was knowable; the LLM-function-returning-`json` case is a cheap static pre-filter.

        ### P-038-5: `@alias`/`@skip` are LLM-path-only decorators; the JSON interchange path is separate
        - **Source**: BEP-038 (Native json Type, status: implemented)
        - **Principle**: `@alias` and `@skip` affect only `ctx.output_format` and `$parse` — the prompt-rendering pipeline. `to_json` is structural and always uses raw field names; interchange renaming is done by overriding `to_json`, not by decorators. These are intentionally separate pipelines.
        - **Intended usage**: use decorators to shape what the LLM sees; override `to_json`/`from_json` to shape what external systems see.
        - **Anti-patterns / slop signals**:
          - Relying on `@alias` to rename keys in an HTTP request body built via `encode`/`stringify` (it won't — a correctness bug and a design misunderstanding).
          - Duplicating a class solely to get different field names for interchange when a `to_json` override suffices.
        - **Applicability trigger**: any class carrying `@alias`/`@skip` whose instances are also encoded to `json` for non-LLM consumers.
        - **Detectability**: hybrid — co-occurrence of decorators and `encode`/`to_json` on the same class is greppable; whether the author expected renaming needs judgment.

        ### P-039-1: `reflect.type_of<T>()` is the single compile-checked bridge from types to `type` values; string lookup is only for dynamic names
        - **Source**: BEP-039 (Runtime Type Reflection via reflect.type_of, status: implemented)
        - **Principle**: `reflect.type_of<T>()` supersedes both BEP-017's per-type shorthands (`reflect.string`) and the use of `baml.llm.get_return_type("Name")` for statically known types — one mechanism, compile-time checked, no requirement that a matching LLM function exist. String-based lookup remains only for names arriving from user input or configuration.
        - **Intended usage**: `let t: type = reflect.type_of<User>()`; `reflect.type_of<T>()` inside generic bodies to read the caller's concrete type.
        - **Anti-patterns / slop signals**:
          - `baml.llm.get_return_type("SomeFunction")` with a string literal where the type is statically known — runtime resolution and stringly-typed fragility for no reason.
          - Declaring a dummy LLM function just so its return type can be looked up.
          - Using deprecated `reflect.string`-style shorthands if present.
        - **Applicability trigger**: any chunk obtaining or passing `type` values.
        - **Detectability**: static — literal-string `get_return_type` calls are directly greppable.

        ### P-039-2: `type` values are opaque and compiler-minted; equality is identity — compare and key on them directly, never on stringified names
        - **Source**: BEP-039 (Runtime Type Reflection via reflect.type_of, status: implemented)
        - **Principle**: `type` is deliberately opaque (no user constructor, no structural introspection until a future `.inspect()`) so that canonicalization holds: `a == b` means the same BAML type. It supports `==`, hashing, and map keys, enabling type-driven dispatch tables.
        - **Intended usage**:
          ```baml
          let registry: map<type, string> = {
              reflect.type_of<User>(): "users",
              reflect.type_of<Order>(): "orders",
          }
          ```
        - **Anti-patterns / slop signals**:
          - Comparing `t.to_string() == "User"` instead of `t == reflect.type_of<User>()` — string comparison defeats canonical identity.
          - String-keyed dispatch maps of type names where a `map<type, ...>` is the designed tool.
          - Attempting to encode type structure in parallel string/enum descriptors when opaque comparison would suffice.
        - **Applicability trigger**: any chunk comparing types, keying registries by type, or branching on type identity.
        - **Detectability**: static-leaning hybrid — `.to_string()` compared to a string literal near `type` values is greppable.

        ### P-039-3: Write generic functions and let the compiler thread type info — don't hand-monomorphize per-type copies
        - **Source**: BEP-039 (Runtime Type Reflection, status: implemented; reinforces the motivation of BEP-013, superseded)
        - **Principle**: Hidden `type`-parameter threading means a generic function calling another generic (`fetch_as<T>` calling `baml.json.decode<T>`) composes automatically, with no user-visible plumbing. This is the sanctioned path for reusable type-parametric logic.
        - **Intended usage**:
          ```baml
          function fetch_as<T>(url: string) -> T {
              baml.json.decode<T>(baml.json.parse(http.get(url).body))
          }
          ```
        - **Anti-patterns / slop signals**:
          - Near-identical function copies per concrete type (`fetch_user`, `fetch_order`, ...) differing only in the type name — hand-monomorphization.
          - Passing type information as extra string parameters through call chains.
          - Concrete wrapper-class families (`PaginatedUsers`, `PaginatedPosts`) where a generic type is expressible (BEP-013's motivating case; BEP-013 itself is **superseded**, so grade against the generics surface the current toolchain actually supports).
        - **Applicability trigger**: any chunk with ≥2 functions or classes that are textual near-duplicates modulo a type name.
        - **Detectability**: hybrid — near-duplicate detection is mechanizable; confirming a generic was expressible needs judgment.

        ### P-043-1: Use the built-in core-type methods; don't re-derive them with manual loops or arithmetic
        - **Source**: BEP-043 (Core Type Methods, status: pending)
        - **Principle**: The stdlib deliberately covers the full JavaScript-equivalent surface (plus `Math`, namespaced onto the types) so that common operations — `min`/`max`/`clamp`, string `trim`/`includes`/`starts_with`/`split`/`lines`, array `find`/`includes`/`reduce`/`some`/`every`/`sort_by`/`retain`, map `get_or_insert`/`delete` — have one canonical spelling.
        - **Intended usage**: `x.clamp(0, 100)`, `arr.find((t) => ...)`, `m.get_or_insert(key, default)`, `s.trim().to_lower_case()` rather than hand-written equivalents. (Note: parts are pending — absence of a method is only slop if the method has actually shipped in the pinned toolchain.)
        - **Anti-patterns / slop signals**:
          - `if (a < b) { a } else { b }` inline where `.min()` exists; hand-rolled clamp chains.
          - Manual `for` loops implementing find/includes/reduce/filter over arrays.
          - `has` + `get` + `set` triplets reimplementing `get_or_insert`.
        - **Applicability trigger**: any chunk with element-wise loops or comparison ladders over strings, arrays, maps, ints, or floats.
        - **Detectability**: hybrid — the loop shapes are greppable patterns; deciding the stdlib method was available and equivalent needs the toolchain's actual method list.

        ### P-043-2: All BAML stdlib and idiomatic user API naming is snake_case
        - **Source**: BEP-043 (Core Type Methods, status: pending — decision recorded in comments)
        - **Principle**: The comment thread settles it explicitly: "lets just use snake_case for all our stdlib, even if it matches TS methods. We need to give all our stdlib api to llms anyway," with existing camelCase methods to be renamed (`toLowerCase` → `to_lower_case`).
        - **Intended usage**: user-defined functions and methods follow snake_case, matching the stdlib the LLM is shown.
        - **Anti-patterns / slop signals**:
          - camelCase user-defined function/method names — indicates the author is writing TypeScript-in-BAML rather than idiomatic BAML.
          - Mixed-convention files.
        - **Applicability trigger**: every declaration.
        - **Detectability**: static — pure regex on identifier casing.

        ### P-043-3: String `length` is byte-based O(1); index operations are fallible on non-ASCII — don't treat length as character count
        - **Source**: BEP-043 (Core Type Methods, status: pending)
        - **Principle**: The BEP explicitly chooses byte-length for `length` (O(1), consistent with other languages) at the cost of fallible indexing, noting non-ASCII characters "are more common in LLM-generated text." Character-level work should use the dedicated code-point APIs (`code_point_at`, `from_code_points`) or `to_utf8`.
        - **Intended usage**: use `length()` for size/emptiness checks and byte budgets; use code-point/char APIs when the unit of work is a character; expect and handle failures when slicing LLM output at arbitrary indices.
        - **Anti-patterns / slop signals**:
          - `substring(0, n)` truncation of LLM output with the assumption that `n` counts characters (silently wrong or failing on multibyte text).
          - `length()` used to count "characters" for user-facing limits on LLM-generated strings.
        - **Applicability trigger**: any chunk indexing, slicing, or length-limiting strings that can contain LLM output.
        - **Detectability**: semantic — the calls are greppable but the character-vs-byte intent needs LLM judgment.

        ### P-055-1 [DRAFT]: Ranges are types unified with pattern matching (`1..3` ≡ `1 | 2 | 3`), and runtime iteration goes through `int.range()` / `float.range()`
        - **Source**: BEP-055 (Range, status: draft)
        - **Principle**: BAML's range design (draft, not yet shipped) makes `1..3` an inclusive *type* subtyping into `int` so ranges compose with the existing literal-union pattern-matching semantics, while runtime-valued ranges use static methods (`int.range(min, max, step)`, `float.range` with a *required* step because stepless float iteration is meaningless).
        - **Intended usage** (once shipped): `match (text.length()) { 1..8 => ..., 9..16 => ... }`; `for (let a in int.range(start, end))`.
        - **Anti-patterns / slop signals** (grade only against shipped surface):
          - Once ranges exist: chained boolean guards (`if n >= 1 && n <= 8`) in match arms where a range pattern applies; hand-rolled counter `while` loops where `int.range` applies.
          - Today (pre-ship): using manual loops is currently correct — a reviewer must check the pinned toolchain before applying this principle.
          - Float iteration without an explicit step — a hand-rolled `while (f <= max) { f += step }` with an accumulating-error step is the hazard the required-step API addresses.
        - **Applicability trigger**: any chunk with numeric interval matching or counted iteration.
        - **Detectability**: static once shipped (guard-chain and counter-loop patterns are greppable); toolchain-availability check required first.

        ### P-029-1 [PENDING]: Immutability is opt-in, deep, and belongs in the type system — not in docs, defensive copies, or parallel class hierarchies
        - **Source**: BEP-029 (Mutability Semantics, status: pending)
        - **Principle**: BAML defaults to mutability (familiar, low prototyping friction) but plans compile-time `readonly` references with *deep* semantics — a readonly reference yields only readonly child references — explicitly rejecting TypeScript-style shallow `Readonly<T>` and the verbose recursive-readonly workarounds it forces. Contracts should be verified by language semantics, "not something to be relied on" via documentation.
        - **Intended usage** (once shipped): `function get(self, key: string) -> readonly T` for shared cached values; readonly parameters on functions that promise not to mutate.
        - **Anti-patterns / slop signals**:
          - Parallel `FooReadonly`/`ImmutableFoo` class hierarchies duplicating every field — the exact verbosity the deep design exists to eliminate.
          - Pervasive defensive `deepcopy` of arguments/returns to simulate immutability contracts (pre-ship this is a mitigating pattern, not slop; post-ship it fights the design).
          - Doc-comment-only mutation contracts ("do not modify the returned value") on shared state with no mechanism.
        - **Applicability trigger**: any chunk sharing mutable non-primitive values across function/module boundaries (caches, registries, shared config).
        - **Detectability**: semantic — parallel-hierarchy and deepcopy-everywhere patterns need LLM recognition; pending status means this grades intent hygiene, not API misuse.

        ### P-029-2 [PENDING]: When mutable and readonly variants of an accessor are both needed, prefer explicit Rust-style `*_readonly` method pairs over overloading
        - **Source**: BEP-029 (Mutability Semantics, status: pending — shepherd's stated preference)
        - **Principle**: The BEP prefers separate explicit variants (like Rust's `get`/`get_mut`), rejecting arbitrary function overloading because compile/runtime dispatch over castable types "produces a lot of edge cases," makes interfaces less coherent, and complicates lookup. `T | Readonly<T>` unions should simplify to `Readonly<T>`; genuinely needing the distinction calls for explicit newtype wrappers.
        - **Intended usage** (once shipped): `function get(self, ...) -> T?` plus `function get_const(readonly self, ...) -> readonly T?`; never two same-named functions differing only in mutability.
        - **Anti-patterns / slop signals**:
          - Attempted overload-style same-name duplicates.
          - Union types crafted to distinguish mutable vs readonly variants of the same class at runtime.
        - **Applicability trigger**: any class exposing both mutating and non-mutating access to inner state, once readonly ships.
        - **Detectability**: static for name collisions; semantic for union-based mutability discrimination. Low grading weight until implemented.

        ### P-005-1 [DRAFT — explicitly unvetted]: The host boundary is snapshot-by-value today; hosts must reassign returned values, and BAML code should not pretend to mutate host state in place
        - **Source**: BEP-005 (Values vs References Across the Host Boundary, status: draft/exploratory; the document is flagged as an AI-written draft not to be taken seriously)
        - **Principle**: The one settled fact: values crossing the host boundary are copies — a BAML function mutating a parameter and returning it does *not* mutate the host's object; correct host usage is `r = client.add_item(r, item)`. The proposed `ref<T>`/`adopt()`/`snapshot()` handle model is unresolved design, not implemented surface.
        - **Intended usage**: under snapshot semantics, write boundary-crossing BAML functions in functional style — take a value, return the new value — and name them honestly.
        - **Anti-patterns / slop signals**:
          - Exported BAML functions that mutate a parameter and return nothing (`-> void`/`null`) — under snapshot semantics the mutation is invisible to the host, a silent no-op bug.
          - Function names implying in-place host mutation (`add_item`, `update_x`) that actually require the caller to capture the return value, with no documentation of that contract.
        - **Applicability trigger**: any BAML function intended to be called from a host language that mutates a non-primitive parameter.
        - **Detectability**: hybrid — mutate-param-return-void signatures are greppable; whether host code depends on the mutation needs cross-language semantic review. Use only the settled snapshot-copy behavior; treat code implementing the proposed `ref<T>`/`adopt` API as speculative (not gradeable).
        # Batch: Standard library surface (BEPs 45, 46, 47, 48, 49, 52, 60, 32, 37, 61)

        Batch philosophy: These BEPs show a stdlib built on three commitments: (1) domain APIs live in small, flat, searchable `baml.<domain>` namespaces with real typed values (never bare strings) while the top level stays reserved for near-universal primitives; (2) the surface is strict and explicit by default — explicit byte/text boundaries, explicit conversions into JSON/prompts, no sniffing or implicit coercion — with dangerous or lossy behavior exposed only as narrow, named opt-ins; and (3) a shared cross-namespace vocabulary (`parse`/`decode<T>`/`stringify`, `try_*`/`is_valid`, cardinality splits, structured `*Error` kinds, handle classes, callback-scoped resources) means learning one data stdlib teaches all of them. A codebase "uses BAML as intended" when it reaches for these typed, namespaced primitives instead of hand-rolling them with strings, and adopts the same vocabulary conventions in its own code.

        ---

        ## Meta-principles (cross-BEP)

        ### P-META-1: Namespaced domain stdlib; top level reserved for universals
        - **Source**: BEP-037 (implemented); also BEP-045 (accepted), BEP-046 (accepted), BEP-060 (implemented), BEP-047 (proposed), BEP-048 (proposed), BEP-052 (pending)
        - **Principle**: Only primitives that appear in nearly every BAML program (tests, asserts) live at the top level; everything else gets a flat, searchable domain namespace (`baml.fs`, `baml.uuid`, `baml.csv`, `baml.re`). Namespaces are one level deep and named for what users search for (`baml.base64`, not `baml.encoding.base64`).
        - **Intended usage**: Call `baml.uuid.v7()`, `baml.csv.read<T>(path)`, `baml.glob.new(pattern)` — the qualified path is the API. User code organizing its own helpers should mirror this: domain-grouped, flat, not deeply nested.
        - **Anti-patterns / slop signals**:
          - Wrapper functions that exist only to shorten `baml.<ns>.f()` to a bare local name with no added logic.
          - Reimplementing a namespaced stdlib capability at file scope (e.g. a local `uuid_v4()` helper).
        - **Applicability trigger**: Any chunk defining utility functions whose names shadow/duplicate stdlib namespace members.
        - **Detectability**: hybrid — grep for local function names matching stdlib surface (`parse_uuid`, `base64_encode`, `glob`, `read_csv`); LLM judges whether they add value or just alias.

        ### P-META-2: Typed domain values, never stringly-typed identifiers
        - **Source**: BEP-045 (accepted); also BEP-052 (pending), BEP-047 (proposed), BEP-060 (implemented)
        - **Principle**: Domain values get a real type (`Uuid`, `Regex`, decoded classes) rather than `string`. Returning strings would lose the typed parse boundary, host-codegen mapping, and methods — "most of the value of a typed language."
        - **Intended usage**: `class Job { id baml.uuid.Uuid }`; parse text into the type at the boundary (`baml.uuid.parse(raw)`), pass the typed value everywhere after; LLM output fields typed as `baml.uuid.Uuid` / `baml.re.Regex` so the runtime validates on parse.
        - **Anti-patterns / slop signals**:
          - Fields/params holding UUIDs, regex patterns, or table rows as raw `string` when the typed form exists.
          - Validating a UUID with a hand-written regex instead of `baml.uuid.is_valid` / `parse`.
          - Repeatedly re-parsing the same string instead of parsing once into the type.
        - **Applicability trigger**: Any class field or function signature whose name suggests a domain value (`*_id`, `pattern`, `uuid`, `timestamp`) typed as `string`.
        - **Detectability**: hybrid — static grep for `string`-typed fields with domain-suggestive names; semantic check on whether the typed alternative applies.

        ### P-META-3: The parse / try_parse / is_valid triad — and when NOT to use it
        - **Source**: BEP-045 (accepted); also BEP-046 (accepted), BEP-060 (implemented, records the exception)
        - **Principle**: Single-value parsers ship as a triad: throwing `parse` (typed error), null-returning `try_parse`, and boolean `is_valid`, with all three sharing one grammar. BEP-060 deliberately breaks the convention for CSV/SQL: `try_decode` is *wrong* when failure carries positional diagnostics — swallowing `CsvError { line: 1048201 }` into `null` discards exactly what the user needs.
        - **Intended usage**: `let id = baml.uuid.try_parse(raw) else { throw "invalid UUID" }` for boring failures; `parse` + `catch` when the error object matters; `catch { e => null }` as the one-liner where null-on-failure of a rich-error API is genuinely wanted.
        - **Anti-patterns / slop signals**:
          - Calling throwing `parse` wrapped in a broad catch that discards the error, where `try_parse` was the designed spelling.
          - Calling `try_parse(x) != null` where `is_valid(x)` exists.
          - Catch-and-null around `baml.csv.decode<T>` / `db.query<T>` in bulk paths, throwing away line/column diagnostics on large inputs.
        - **Applicability trigger**: Any call to a stdlib parse/decode function, or any catch block whose sole body is `null`/ignore.
        - **Detectability**: static for the spelling mismatches (grep `catch` around `parse`); semantic for whether discarding diagnostics is acceptable in context.

        ### P-META-4: One shared data-stdlib vocabulary: `parse` (untyped) / `decode<T>` (typed) / `stringify` (to text), plus the cardinality split
        - **Source**: BEP-060 (implemented); also BEP-047 (proposed), referencing BEP-038's `baml.json`
        - **Principle**: Verbs are consistent across data namespaces so learning one teaches all: `parse` → untyped form, `decode<T>` → typed values, `stringify` → text; and cardinality is split into distinct methods (`query`/`decode` any, `*_optional` zero-or-one, `*_one` exactly-one, `scalar`) rather than one overloaded verb — this "avoids common off-by-one bugs where a query expected one row but accidentally accepts many."
        - **Intended usage**: `db.query_one<User>(...)` when exactly one row is expected; `baml.csv.decode_one<Kpi>(raw)` for an LLM asked for one row; never `query<T>(...)[0]`.
        - **Anti-patterns / slop signals**:
          - `query<T>(...)` followed by `[0]` or a manual length check where `query_one`/`query_optional` encodes the intent.
          - `decode<T>` result indexed `[0]` where `decode_one<T>` exists.
          - User-defined helpers that use different verbs for the same concepts (`load`, `fromString`, `toText`) instead of the ecosystem vocabulary.
        - **Applicability trigger**: Any call to a `query*`/`decode*` family followed by indexing/length checks; any user API doing typed/untyped conversion.
        - **Detectability**: static — grep for `query<`/`decode<` immediately indexed or length-checked.

        ### P-META-5: Structured errors — a `kind` enum inside an error class when position matters, bare enum when it doesn't; one error identity per domain
        - **Source**: BEP-060 (implemented); also BEP-047, BEP-046, BEP-045, BEP-032/037
        - **Principle**: Error shape is proportional to diagnostic need: `Base64Error`/`UuidError` are bare enums (failure is boring); `CsvError`/`SqlError` are classes with a `kind` enum plus position/context fields ("CSV errors without positions are useless"). And a domain keeps one error identity everywhere: all `baml.fs` operations — including the CSV reader's file IO — throw `root.errors.Io`, so one catch arm works across the language.
        - **Intended usage**: `catch { e: baml.csv.CsvError => log_bad_row(e.line, e.message) }`; portable handling on `e.kind`, precise handling on provider `code`.
        - **Anti-patterns / slop signals**:
          - Catching structured errors and logging only a generic message, ignoring `kind`/`line`/`column`.
          - String-matching on `e.message` where `kind` or `code` exists.
          - Wrapping `root.errors.Io` into a custom error type, splitting the domain's error identity.
        - **Applicability trigger**: Any `catch` block over stdlib error types.
        - **Detectability**: hybrid — grep catch arms; semantic judgment on whether available structure is used.

        ### P-META-6: Handle classes for compile-once / stream resources; plain data classes otherwise
        - **Source**: BEP-037 (implemented); also BEP-060, BEP-052, BEP-032
        - **Principle**: When a resource has expensive construction or a cursor (compiled glob/regex, CSV reader/writer, file), it is an opaque handle class so the cost is paid once and reused; when data is free (`DirEntry`, `CsvError`, options), it's a plain class. Handles are not JSON-serializable and have no value equality.
        - **Intended usage**: `let g = baml.glob.new("**/*.ts")` once, then many `g.scan()`/`g.match()` calls; compile a `Regex` once outside a loop; one `CsvReader` per stream.
        - **Anti-patterns / slop signals**:
          - Constructing a `Glob`/`Regex`/`CsvReader`/`Database` inside a loop body that could hoist it (recompiling per iteration — exactly the cost the handle design exists to avoid).
          - Attempting to serialize, compare, or store handles as data.
        - **Applicability trigger**: Any loop containing a stdlib constructor call (`baml.glob.new`, `Regex.new`, `baml.sql.connect`, `baml.csv.open` when the same source is reopened).
        - **Detectability**: static — constructor calls lexically inside `for`/`while` bodies are grep-able; hoistability needs light semantic judgment.

        ### P-META-7: Nothing enters JSON or prompts implicitly; conversions are explicit and canonical
        - **Source**: BEP-046 (accepted); also BEP-045, BEP-060
        - **Principle**: `uint8array` is deliberately not JSON-serializable and never auto-renders in prompts — the user writes `bytes.to_base64()`. Tables enter prompts via explicit `baml.csv.to_markdown<T>(rows, max_rows)` (with truncation so a 100k-row table can't blow up a prompt). UUIDs serialize/render as their canonical string.
        - **Intended usage**: `prompt: \`The file bytes are base64: ${file.bytes().to_base64()}\``; `RankLeads(baml.csv.to_markdown<Lead>(top))`.
        - **Anti-patterns / slop signals**:
          - Hand-building Markdown/ASCII tables for prompt context with string concatenation or `${for}` loops where `to_markdown` exists.
          - Interpolating unbounded row collections into prompts with no truncation.
          - Hand-rolled hex/byte-stringification to smuggle bytes into JSON.
        - **Applicability trigger**: Prompt bodies interpolating collections or binary data; JSON payload construction involving bytes.
        - **Detectability**: hybrid — grep prompt literals for loop-built table markup (`|`-rows inside `${for}`); LLM confirms `to_markdown` would fit.

        ### P-META-8: Strict by default, no sniffing; every relaxation is a narrow, named option
        - **Source**: BEP-060 (implemented); also BEP-047, BEP-046
        - **Principle**: Defaults are strict and lossless (RFC 4180 strict, `ragged: "strict"`, `on_error: "throw"`, UTF-8 strict, no `dynamicTyping`, no dialect sniffer, no default null vocabulary, strict base64 alphabet separation); anything lenient is an explicit named knob (`"utf8-lossy"`, `on_error: "skip"`, `null_values: [...]`), never a vague "lenient" flag or a silent heuristic. Schema (a BAML class) beats any sniffer; locale conversion belongs above the parser (`string` field + explicit convert).
        - **Intended usage**: `baml.csv.decode<Kpi>(raw, options = ReaderOptions { skip_lines: 1, trim: "all", on_error: "skip" })` — each piece of dirt handled by a named option.
        - **Anti-patterns / slop signals**:
          - Pre-cleaning input with regex/string surgery (stripping quotes, splitting on commas) before handing it to the stdlib parser, instead of using the named options.
          - Turning on lenient options (`on_error: "skip"`, `utf8-lossy`) habitually where strict would have caught real data bugs, with no use of `skipped()`/`on_skip` diagnostics.
          - Post-hoc locale mangling that a `string`-typed field + explicit convert was designed for.
        - **Applicability trigger**: Any code path preparing text for or configuring a stdlib parser.
        - **Detectability**: semantic — needs judgment on whether preprocessing duplicates an option.

        ### P-META-9: Small closed option sets are literal string unions; open/domain concepts are enums with readable names
        - **Source**: BEP-046 (accepted); also BEP-060, BEP-045 (readable `UuidFormat.Hyphenated` over C#'s opaque `N`/`D`/`B` — "tokens are cheap")
        - **Principle**: Optional-argument modes use literal unions (`padding: "include" | "omit"`, `trim: "none" | "headers" | ...`) to keep call sites short; genuine domain enumerations (`UuidFormat`, `SqlErrorKind`) are enums with fully spelled-out, readable variant names — never single-letter or abbreviated codes.
        - **Intended usage**: `bytes.to_base64(padding = "omit")`; `id.format(UuidFormat.Urn)`. User-defined APIs should follow the same split, and spell names out (`v7_at_milliseconds`, not `v7_at_ms`).
        - **Anti-patterns / slop signals**:
          - Declaring a two-variant enum for a function's optional mode parameter.
          - Cryptic abbreviated identifiers in user API surface where the stdlib precedent spells things out.
        - **Applicability trigger**: User-defined function signatures with mode-style parameters.
        - **Detectability**: static — enums with ≤3 variants used only as a single function's parameter type.

        ### P-META-10: Options objects over boolean flags for extensible operations
        - **Source**: BEP-037 (implemented); also BEP-060, BEP-047
        - **Principle**: Operations likely to grow knobs take an options class (`MkdirOptions { recursive }`, `ScanOptions`, `ReaderOptions`, `ConnectOptions`) with defaulted fields, so fields can be added without signature breaks — explicitly a bet against bare positional booleans.
        - **Intended usage**: `baml.fs.mkdir(path, MkdirOptions { recursive: true })`; `g2.scan(ScanOptions { cwd: "./data", absolute: true })`.
        - **Anti-patterns / slop signals**:
          - User-defined functions accumulating 3+ positional bool/scalar config params instead of an options class.
        - **Applicability trigger**: User function definitions with multiple configuration parameters.
        - **Detectability**: static — count trailing bool/optional params in signatures.

        ---

        ## Per-BEP principles

        ### P-045-1: v7 for database/orderable keys, v4 for random identifiers
        - **Source**: BEP-045 (UUID Standard Library, status: accepted)
        - **Principle**: The stdlib standardizes exactly two generation paths because they cover the real use cases: `v7()` (time-ordered, RFC-9562-recommended) for database primary keys and logs, `v4()` for random public identifiers. `v7_at_milliseconds` exists for deterministic tests and migrations.
        - **Intended usage**: `let user_id = baml.uuid.v7()` for persisted keys; `let trace_id = baml.uuid.v4()`; `v7_at_milliseconds(ts)` in tests needing determinism.
        - **Anti-patterns / slop signals**:
          - Hand-rolling IDs from timestamps + random suffixes (reimplementing v7 badly).
          - Using `v4()` for keys where sortable/DB-local `v7()` was the designed fit, or vice versa without reason.
          - Asking the host language to allocate IDs the BAML workflow needed.
        - **Applicability trigger**: Any code generating identifiers (fields named `id`, `*_id`, `trace_id`, `key`).
        - **Detectability**: hybrid — grep for id-construction via string concat/random; semantic for v4-vs-v7 fit.

        ### P-045-2: UUIDs are identifiers, not cryptographic secrets
        - **Source**: BEP-045 (UUID Standard Library, status: accepted)
        - **Principle**: UUID deliberately lives in `baml.uuid`, not `baml.crypto`, precisely to avoid the implication that UUIDs are tokens. A UUIDv4 has fixed version/variant bits and "should not be accepted as a secret just because it came from a CSPRNG"; security tokens belong to a future `baml.crypto.random_bytes()` / `token_urlsafe()`.
        - **Intended usage**: UUIDs for correlation IDs, DB keys, JSON API ids. Secrets/tokens from a genuine random-bytes source (`baml.random.SystemRandom` per BEP-048) encoded appropriately.
        - **Anti-patterns / slop signals**:
          - `baml.uuid.v4()` used as an API key, session token, password-reset token, or HMAC key.
        - **Applicability trigger**: UUID generation flowing into variables/fields named `token`, `secret`, `api_key`, `session`.
        - **Detectability**: hybrid — grep for `uuid.v4` near token-ish names; semantic confirmation of security use.

        ### P-045-3: Parse at the text boundary; canonicalize formatting through the type
        - **Source**: BEP-045 (UUID Standard Library, status: accepted)
        - **Principle**: Values entering from text get explicit validation (`parse`/`try_parse` accept hyphenated/simple/URN/braced, case-insensitive); formatting always emits canonical lowercase via `to_string()` or a readable `format(UuidFormat.X)`. The 16 bytes are the representation; string forms are adapters.
        - **Intended usage**: `let id = baml.uuid.try_parse(raw_id) else { throw "invalid UUID" }`; render with `to_string()`; `bytes()` for the RFC-order 16 bytes.
        - **Anti-patterns / slop signals**:
          - Lower/upper-casing, hyphen-stripping, or regex-normalizing UUID strings by hand instead of round-tripping through `parse`+`to_string`/`format`.
          - Comparing UUIDs as strings with case juggling instead of comparing `Uuid` values.
        - **Applicability trigger**: String manipulation applied to UUID-shaped values.
        - **Detectability**: hybrid — grep for `.replace("-"`, case conversions near uuid names.

        ### P-046-1: Base64 encodes bytes, not text — the UTF-8 step is explicit
        - **Source**: BEP-046 (Base64 and Base64URL Standard Library, status: accepted)
        - **Principle**: There is deliberately no `string.to_base64()`: a direct string API would silently choose a text encoding (JavaScript's `btoa` problem). Text opts into bytes explicitly, one method longer.
        - **Intended usage**: `"hello".to_utf8().to_base64()`; `string.from_utf8(baml.base64.decode(encoded))`; bytes-first paths use `bytes.to_base64()` directly.
        - **Anti-patterns / slop signals**:
          - Any hand-rolled base64 encoder/decoder (lookup tables, bit-twiddling).
          - Helper functions that hide the `to_utf8()` step behind a stringly wrapper, recreating the `btoa` footgun.
        - **Applicability trigger**: Any code producing or consuming base64 text.
        - **Detectability**: static — grep for base64 alphabet constants or manual 6-bit packing; wrapper detection is grep + light judgment.

        ### P-046-2: Standard Base64 and Base64URL are distinct variants — never mixed, correct padding defaults
        - **Source**: BEP-046 (Base64 and Base64URL Standard Library, status: accepted)
        - **Principle**: "Base64URL is not just 'Base64 in a URL'" — the decoders strictly reject the other alphabet to catch protocol mistakes early; padding is the forgiving axis (defaults: standard=padded, url=unpadded, decode=`"allow"`), alphabet is not.
        - **Intended usage**: `bytes.to_base64url()` for JWT-like/URL contexts; `baml.base64.decode_url(jwt_part)`; override padding only when a protocol demands.
        - **Anti-patterns / slop signals**:
          - `decode()` on JWT segments or URL params (wrong variant — will fail on `-`/`_`), or character-substitution hacks (`.replace("+", "-")`) to convert between alphabets by hand.
          - Manually appending/stripping `=` padding instead of using the padding options.
        - **Applicability trigger**: Base64 calls in URL/JWT/header contexts; string replaces on base64 text.
        - **Detectability**: static — grep `replace` of `+`/`/`/`=` near base64 calls; variant-context fit is semantic.

        ### P-047-1 [PROPOSED]: Values go in named parameters, never concatenated into SQL text
        - **Source**: BEP-047 (SQL Standard Library, status: proposed)
        - **Principle**: SQL stays visible and portable with `:name` placeholders bound from a map; the runtime rewrites to native placeholders. Concatenating untrusted values into SQL is the explicitly documented "Wrong" path.
        - **Intended usage**: `db.query<User>("SELECT * FROM users WHERE email = :email", {"email": email})`.
        - **Anti-patterns / slop signals**:
          - `"... WHERE x = '" + value + "'"` — string-built SQL with interpolated values.
          - Provider-native placeholders (`$1`, `?`, `@p0`) written in BAML source instead of `:name`.
        - **Applicability trigger**: Any string passed to `query*`/`execute*` built with `+` or interpolation.
        - **Detectability**: static — SQL-shaped strings with concatenation/interpolation of non-literal values.

        ### P-047-2 [PROPOSED]: Dynamic identifiers via match-allowlist, not parameters or raw input
        - **Source**: BEP-047 (SQL Standard Library, status: proposed)
        - **Principle**: Parameters bind values only — never table/column names, operators, or fragments. The designed pattern for dynamic identifiers is an exhaustive `match` allowlist that throws on unknown input, then concatenation of the *trusted* result.
        - **Intended usage**: `let sort_column = match (sort) { "created" => "created_at", "email" => "email", _ => throw "invalid sort" }` then `"... ORDER BY " + sort_column`.
        - **Anti-patterns / slop signals**:
          - Interpolating a user-provided/LLM-provided identifier directly into SQL without an allowlist match.
          - Attempting to bind an identifier as a `:param`.
        - **Applicability trigger**: SQL strings whose concatenated parts are identifiers.
        - **Detectability**: hybrid — static finds identifier concatenation; semantic verifies an allowlist guards it.

        ### P-047-3 [PROPOSED]: Callback transactions — commit on return, rollback on throw
        - **Source**: BEP-047 (SQL Standard Library, status: proposed)
        - **Principle**: `db.transaction((tx) -> { ... })` is the only transaction shape because manual `begin`/`commit`/`rollback` "make every error path a footgun" under typed `throws` — a throw before `commit()` silently leaks the transaction. Savepoints nest the same way.
        - **Intended usage**: All multi-statement atomic work inside the callback; return the result from the callback; optional-work in `tx.savepoint((sp) -> {...}) catch {...}`.
        - **Anti-patterns / slop signals**:
          - Sequences of related `db.execute` calls (insert + update pairs) outside any transaction where atomicity clearly matters.
          - Emulating transactions with manual compensating deletes.
        - **Applicability trigger**: Multiple write statements against the same `Database` in one logical operation.
        - **Detectability**: semantic — atomicity need requires judgment; the multiple-execute pattern is grep-able.

        ### P-047-4 [PROPOSED]: One long-lived `Database` handle; direct SQL, no homegrown ORM layer as default
        - **Source**: BEP-047 (SQL Standard Library, status: proposed)
        - **Principle**: `connect()` returns a data-source/pool to be created once and reused; SQL remains visible in source with typed row decoding (`query<T>` matches selected columns to class fields, SQL aliases for renaming) rather than ORM machinery.
        - **Intended usage**: `let db = baml.sql.connect(env.get_or_panic("DATABASE_URL"))` once (with `defer { db.close() }`); `SELECT id, name AS display_name` to fit a BAML class; `query_rows` + `get<T>` only for exploratory/dynamic shapes.
        - **Anti-patterns / slop signals**:
          - `connect()` inside per-request/per-loop code.
          - Using `query_rows` + manual field extraction everywhere when a class + `query<T>` fits.
          - Bare filename passed to `baml.sql.connect` (design requires explicit `baml.sql.sqlite.open`).
        - **Applicability trigger**: Any `baml.sql` usage.
        - **Detectability**: hybrid — connect-in-loop and query_rows-dominance are grep-able; ORM-ness is semantic.

        ### P-048-1 [PROPOSED]: Secure by default — system CSPRNG unless an `Rng` is explicitly passed
        - **Source**: BEP-048 (Randomness, status: proposed)
        - **Principle**: Randomness must be secure-by-default: functions take an optional `rng: baml.random.Rng = baml.random.SystemRandom.get()` parameter, defaulting to the system source, with seeded PRNGs (`Xoshiro256PlusPlus` fast, `ChaCha20` crypto-grade) only when explicitly constructed and passed.
        - **Intended usage**: `int.random()` for the common case; `int.random(0, n, rng = my_seeded_rng)` in deterministic tests, threading one `Rng` object through rather than mutating global state.
        - **Anti-patterns / slop signals**:
          - Deriving "randomness" from timestamps, hashes of inputs, or UUID string slices instead of the random namespace.
          - Defaulting a security-sensitive path to a seeded/fast PRNG.
          - Calling `Rng.random_int()` directly in application code (documented as an implementation detail — use the type's `random` methods).
        - **Applicability trigger**: Any random-value generation; any test needing determinism.
        - **Detectability**: hybrid — timestamp/hash-based pseudo-randomness needs semantic recognition; direct `random_int` calls are grep-able.

        ### P-048-2 [PROPOSED]: Random constructors live on the types, not in `baml.random`
        - **Source**: BEP-048 (Randomness, status: proposed)
        - **Principle**: `baml.random` contains only randomness *sources*; producing random values of a type is that type's method (`int.random(...)`, `uint8array.random(len)`, `array.choice(...)`), each accepting the optional `rng`.
        - **Intended usage**: `xs.choice()` for a random element; `uint8array.random(32)` for a seed.
        - **Anti-patterns / slop signals**:
          - Hand-rolling range mapping / element selection from raw bytes (`rng.random(8)` then modulo arithmetic) where a type-level constructor exists.
        - **Applicability trigger**: Code converting raw random bytes into typed values.
        - **Detectability**: static — `Rng.random(` calls followed by manual arithmetic.

        ### P-049-1: One expression language — native `${}` interpolation, never a second template language
        - **Source**: BEP-049 (String Interpolation, status: implemented)
        - **Principle**: Jinja is replaced because two expression languages mean two scoping rules, two truthiness rules, and tooling that stops at the prompt boundary. `${expr}` is a full BAML block expression (statements + trailing expr; unit renders `""`, so `${let x = ...}` declares template-locals) — one language across code and prompts.
        - **Intended usage**: Backtick strings with `${user.name}`, `${items.map(|x| x * 2).join(", ")}`, `${let widgets = items.filter(...)}` for template locals.
        - **Anti-patterns / slop signals**:
          - New code written with Jinja markers (`{{ }}`, `{% %}`, `|filter` pipelines) in `#"..."#` strings — legacy syntax under active deprecation.
          - Precomputing every interpolated value into locals *above* the string when inline expressions were the designed ergonomics (mild), or conversely giant multi-statement logic crammed into `${}` blocks.
        - **Applicability trigger**: Every prompt/string literal.
        - **Detectability**: static — grep `{{`, `{%`, `#"` in new code.
        - **NOTE for graders**: check the pinned toolchain — on 0.12.x-era toolchains the Jinja `#"..."#`/`{{ ctx.output_format }}` forms are still the shipped surface; grade legacy-syntax slop only on toolchains where `${}` interpolation has landed.

        ### P-049-2: Block control flow (`${for}`/`${if}`) so source shape mirrors rendered output
        - **Source**: BEP-049 (String Interpolation, status: implemented)
        - **Principle**: The decision "the rest of the design hangs on": TypeScript-style `.map(...).join("\n")` chains hide output structure behind closure machinery; block tags keep the template looking like its output. Whitespace rule: "to add a newline in the output, add a newline in the source"; `.join()` remains the right tool for single-value-with-separator.
        - **Intended usage**: Nested `${for}` blocks for structured prompt sections; `${names.join(" ")}` for inline separator joins; `${if (cond)}...${else}...${endif}` for conditional sections.
        - **Anti-patterns / slop signals**:
          - Multi-line output structure built via nested `.map().join("\n")` inside a single `${}` — the exact TS anti-pattern the BEP quotes.
          - String accumulation (`s += ...` in a loop) to build a prompt where a backtick template with block tags fits.
          - Conversely, a `${for}` used where a one-line `.join()` is the designed tool.
        - **Applicability trigger**: Any template producing repeated or conditional multi-line structure.
        - **Detectability**: static — `.map(` + `.join("\n")` inside `${}`; loop-appended string builders feeding prompts.

        ### P-049-3: `"..."` is inert, backticks interpolate — pick the form by intent
        - **Source**: BEP-049 (String Interpolation, status: implemented)
        - **Principle**: Two forms with distinct meaning: `"..."` never interpolates (model names, identifiers, content with literal `${}` like TS snippets); `` `...` `` interpolates, handles multi-line with auto-dedent, and the multi-backtick ladder handles dense-backtick content. Escaping (`\${`, `` \` ``) is for mixed cases only.
        - **Intended usage**: `let model = "openai/gpt-4o"`; backticks for anything interpolated or multi-line.
        - **Anti-patterns / slop signals**:
          - Backtick strings containing zero interpolation *and* heavy `\${`/`` \` `` escaping — the inert form or the ladder was the designed answer.
          - Manual `\n`-concatenation chains to build multi-line text instead of an auto-dedented backtick block.
        - **Applicability trigger**: Every string literal.
        - **Detectability**: static — escape-density in backtick literals; `+ "\n" +` chains.

        ### P-049-4: Formatting via method chains; loop metadata via the language, not magic
        - **Source**: BEP-049 (String Interpolation, status: implemented)
        - **Principle**: There is deliberately no format mini-DSL (`:.3f`) and no `loop.index`/`loop.first` vocabulary: formatting is ordinary methods (`.toFixed(3)`, `.padStart(5, "0")`, `.format("%Y-%m-%d")`) that compose and get LSP/type-checking; loop indices come from `.enumerate()` with destructuring.
        - **Intended usage**: `${pi.toFixed(3)}`; `${for (let [item, i] in names.enumerate())}${if (i > 0)}, ${endif}${item}${endfor}`.
        - **Anti-patterns / slop signals**:
          - Maintaining a manual counter variable mutated inside a template loop instead of `.enumerate()`.
          - Hand-rolled padding/rounding helpers duplicating core string/number methods.
        - **Applicability trigger**: Templates with indices/counters or numeric/date formatting.
        - **Detectability**: static — counter variables incremented inside `${for}` bodies.

        ### P-049-5: Plain functions replace `template_string`; tagged templates (not string munging) for domain processing
        - **Source**: BEP-049 (String Interpolation, status: implemented)
        - **Principle**: `template_string` is removed — a reusable prompt fragment is an ordinary `fn` returning a backtick string, called inline as `${Foo(x)}`. For consumers needing literals and values separately (SQL params, ChatML roles), the mechanism is tagged templates, where the tag controls evaluation timing — never flat-string post-processing. `role(...)`/`ctx.*` are lambda parameters (the `_.role` namespace is deprecated).
        - **Intended usage**: `fn SystemPrompt(role: string) -> string { \`You are a ${role} assistant.\` }`; `${SystemPrompt("data analyst")}` inside prompts; `${role("system")}` / `${ctx.output_format}` in prompt bodies.
        - **Anti-patterns / slop signals**:
          - New `template_string` declarations (on toolchains where the replacement has shipped).
          - `_.role(x)` / `_.chat(x)` in new code (deprecated alias for `role(x)`).
          - Post-hoc parsing/splitting of a rendered flat string to recover structure (roles, params) that a tag would have preserved.
        - **Applicability trigger**: Reusable prompt fragments; role markers; any string post-processing that reverse-engineers structure.
        - **Detectability**: static — grep `template_string`, `_.role`, `_.chat`.

        ### P-052-1 [PENDING]: Linear-time regex by default; compiled `Regex` objects, safety over feature-count
        - **Source**: BEP-052 (Regex, status: pending)
        - **Principle**: `baml.re` deliberately uses a ReDoS-resistant linear-time engine (Rust `regex`, no backreferences/lookbehind) because a major use case is executing *LLM-generated* patterns over LLM-supplied haystacks — catastrophic backtracking is a real outage class (Cloudflare 2019). Flags go inline (`(?i)`), and since `Regex` is stateless, sharing/caching one compiled instance is the intended reuse model.
        - **Intended usage**: `let regex = baml.re.Regex.new("#[0-9a-zA-Z]{6}")` once; `regex.test(s)` / `exec` / `match_all` / `split`; LLM functions returning `baml.re.Regex` so deserialization type-checks generated patterns.
        - **Anti-patterns / slop signals**:
          - Hand-rolled scanning/matching state machines over strings where a `Regex` fits.
          - Writing patterns that require backreferences/lookbehind and working around the engine with fragile multi-pass hacks instead of restructuring.
          - Treating LLM-returned patterns as raw strings and interpolating them un-typed instead of typing the return as `Regex`.
        - **Applicability trigger**: Any pattern-matching/extraction over text; LLM functions producing patterns.
        - **Detectability**: hybrid — manual scanners need semantic judgment; `Regex.new` in loops is grep-able (see P-META-6).

        ### P-032-1: Bun-style read verbs, explicit open modes, one polymorphic `write`
        - **Source**: BEP-032 (Filesystem API, status: implemented)
        - **Principle**: The fs surface is `baml.fs.file(path, mode)` with Node-style modes (`"r"` read-only, `"r+"` read-write for random access via `seek`), Bun-style `text()`/`bytes()` for whole-file reads, and a single one-shot `baml.fs.write(path, data)` taking `string | uint8array` that always creates/truncates and auto-creates parent directories.
        - **Intended usage**: `baml.fs.file(p, "r").text()` for reads; `baml.fs.write(p, data)` for one-shot writes (no manual parent `mkdir` needed); `"r+"` + `seek` only for genuine random-access I/O.
        - **Anti-patterns / slop signals**:
          - `mkdir` + open + `File.write` sequences to do what one `baml.fs.write` call does.
          - Opening `"r+"` and seeking for what is a simple whole-file read/write.
          - Explicit `close()` ceremony treated as required (it's a documented no-op; GC handles cleanup) — not wrong, but cargo-culted resource management signals unfamiliarity.
        - **Applicability trigger**: Any filesystem I/O.
        - **Detectability**: static — mkdir-then-write sequences; `"r+"` usage without a subsequent `seek`.

        ### P-037-1: `Glob.scan` for pattern-based file finding; `read_dir` is single-level with free type info
        - **Source**: BEP-037 (Filesystem Directory Operations & Glob, status: implemented)
        - **Principle**: Recursive traversal was deliberately *excluded* from `read_dir` ("recursive traversal has fundamentally different semantics… `Glob.scan` already covers that use case"). `read_dir` returns `DirEntry[]` with free `is_dir`/`is_file`/`is_symlink` flags; pattern-based tree search is `baml.glob.new(pattern).scan(root)` with `ScanOptions`. `Glob.match` is pure string matching, no I/O.
        - **Intended usage**: `baml.glob.new("**/*.json").scan(ScanOptions { cwd: "./data" })` for tree searches; `read_dir` + `entry.is_dir` for one level; `mkdir(path, MkdirOptions { recursive: true })` — idempotent, `mkdir -p` semantics.
        - **Anti-patterns / slop signals**:
          - Hand-written recursive `read_dir` walkers with manual extension filtering — reimplementing `Glob.scan`.
          - Manual suffix/prefix string checks (`name.ends_with(".ts")`) across directory trees where a glob pattern was the designed tool.
          - Existence-check-then-mkdir dances instead of `recursive: true` idempotency.
        - **Applicability trigger**: Any directory traversal or file-discovery code.
        - **Detectability**: static — recursive functions calling `read_dir`; `exists` immediately before `mkdir`.

        ### P-060-1: Streaming reader is the core; one-shots are sugar — pick by input size, same options either way
        - **Source**: BEP-060 (CSV Standard Library, status: implemented)
        - **Principle**: The handle (`CsvReader` implementing `baml.iter.Iterator`) is the primitive; `read<T>`/`decode<T>` are wrappers sharing one option vocabulary, so scaling from a 5-line script to a 10 GB job is "a verb swap." If the file might not fit in memory, `open` it.
        - **Intended usage**: `baml.csv.read<Lead>("./testdata/leads.csv")` for fixtures/eval sets; `for (let t in baml.csv.open("10gb.csv").rows<Txn>())` for large files — constant memory.
        - **Anti-patterns / slop signals**:
          - `read<T>`/`decode<T>` (full materialization) on inputs that are plausibly large, followed by a single-pass loop.
          - `open(...).rows<T>().collect()` ceremony on small fixtures where `read<T>` is the one-call spelling.
          - Any hand-rolled `split(",")` / line-splitting CSV parsing at all — "wrong the first time a field contains a comma or a newline"; this is the headline reason the namespace exists.
        - **Applicability trigger**: Any CSV consumption or production.
        - **Detectability**: hybrid — `split(",")` on file/LLM content is pure grep; eager-vs-streaming fit is semantic.

        ### P-060-2: Per-record error recovery through the designed channels — never lose the good rows, never lose the diagnostics
        - **Source**: BEP-060 (CSV Standard Library, status: implemented)
        - **Principle**: A bad quote on row 1,000,000 must not forfeit the other 999,999 rows *and* must not vanish silently. `next()` throws per record and the reader resumes; loop-`catch` gives observe-and-skip in three lines; `on_error: "skip"` is the named silent-skip option, always accompanied by `skipped()`/`skipped_count()`/`on_skip` diagnostics. IO errors stay `root.errors.Io` (stream-fatal); errors travel on the throws channel, never in-band.
        - **Intended usage**: `for (let rec in r) { ingest(rec) } catch { e: baml.csv.CsvError => { bad += 1 } }`; or `on_error: "skip"` plus reporting `r.skipped_count()`.
        - **Anti-patterns / slop signals**:
          - Wrapping the whole read in one try/catch and aborting (or returning partial silently) on the first bad record.
          - `on_error: "skip"` with the skip diagnostics never consulted anywhere — silent data loss by configuration.
          - Pre-filtering "bad-looking" lines with string heuristics before parsing.
        - **Applicability trigger**: CSV reads over external/LLM/dirty data.
        - **Detectability**: hybrid — grep `on_error: "skip"` and check for any `skipped`/`on_skip` reference in scope; abort-on-first-error shape is semantic.

        ### P-060-3: Handle LLM-emitted CSV with the designed idiom, and schema-typed decode over string sniffing
        - **Source**: BEP-060 (CSV Standard Library, status: implemented)
        - **Principle**: LLM CSV output (a headline workload) has a documented recipe — `skip_lines` for the opening fence, `on_error: "skip"` for the closing fence, `trim: "all"` for cell padding, `decode_one<T>` when exactly one row was requested — and types come from the declared class, never value sniffing (`"08904"` stays a string unless a field says otherwise). Null mapping is explicit via `null_values`; dialect recipes (Excel, TSV) are documented option combinations, not modes.
        - **Intended usage**: `baml.csv.decode<Kpi>(raw, options = ReaderOptions { skip_lines: 1, trim: "all", on_error: "skip" })` for fenced LLM output; `WriterOptions { terminator: "crlf", bom: true, sanitize_formulas: true }` for spreadsheet-bound exports.
        - **Anti-patterns / slop signals**:
          - Regex/string surgery to strip ``` fences and whitespace from LLM CSV before parsing.
          - Decoding everything to `string[][]` via `parse` and hand-converting cells where `decode<T>` with a class fits.
          - Home-grown null vocabularies (`if cell == "N/A"` checks) instead of `null_values`.
          - Spreadsheet exports without considering `sanitize_formulas` (CSV-injection guard) in user-facing export paths.
        - **Applicability trigger**: Code between an LLM function returning tabular text and its consumption; CSV export paths.
        - **Detectability**: hybrid — fence-stripping regexes near `baml.csv` calls are grep-able; the rest is semantic.

        ### P-060-4: `to_string()` is display; format serialization is a namespace function
        - **Source**: BEP-060 (CSV Standard Library, status: implemented); also BEP-045 (`to_string` = canonical display)
        - **Principle**: `to_string()` is the universal *display* protocol; CSV is a table format needing collection scope + dialect, so "render as CSV" is deliberately `baml.csv.stringify<T>(rows, options)` — there is no universal `to_csv()`, and the buffer writer's accessor is `text()` (mirroring `File.text()`), not `to_string()`, to avoid conflating display with serialization.
        - **Intended usage**: `baml.csv.stringify<T>(rows)` / `write<T>(path, rows)` for output; `to_string()` only for logs/debug display.
        - **Anti-patterns / slop signals**:
          - Building CSV lines by joining `to_string()` values with commas (no quoting, no dialect — corrupt on the first embedded comma).
          - User types overloading `to_string()` to emit wire formats.
        - **Applicability trigger**: Any code emitting delimited text.
        - **Detectability**: static — `.join(",")` feeding file writes or returns typed as CSV.

        ### P-061-1 [DRAFT]: Complementary operations share one naming convention — don't mix ecosystems pairwise
        - **Source**: BEP-061 (Filesystem API Coherence, status: draft — question posed, no option yet accepted)
        - **Principle**: The BEP's settled claim is negative and still checkable: pairing names from different ecosystems for complementary operations (Unix `mkdir` with Rust `remove_dir`) breaks the coherent mental model; a naming decision should commit to one convention rather than growing the hybrid ad hoc. Which convention wins is undecided — do not grade code against any specific choice.
        - **Intended usage**: For user-defined API families: name create/remove/read counterparts from the same convention family; when extending, match the existing dominant convention of the surrounding surface (BAML stdlib convention is snake_case).
        - **Anti-patterns / slop signals**:
          - A user module exposing e.g. `mkdirTemp` next to `remove_directory_recursive` next to `rmCache` — mixed conventions for sibling operations.
          - camelCase function names in BAML code (stdlib is uniformly snake_case).
        - **Applicability trigger**: Any module defining families of paired/complementary operations.
        - **Detectability**: static — casing is pure grep; convention-mixing within a module is grep + light judgment.

        **Coverage note:** BEP-047 and BEP-048 are *proposed* and BEP-052 is *pending* — their principles describe intended design direction; code predating their implementation can't be penalized for absence of the APIs, only for contradicting the recorded rationale (e.g. SQL string concatenation, insecure randomness). BEP-061 is a draft that records an unresolved naming question; only its meta-claim (P-061-1) is gradeable. BEP-049 explicitly deprecates Jinja/`#"..."#`/`template_string`/`_.role` — those legacy forms are concrete slop signals in new code *on toolchains where the replacement has shipped*.
        # Batch: LLM machinery & testing (BEPs 9, 58, 59, 23, 36, 40, 56, 30)

        Summary of batch philosophy: These eight BEPs converge on a single stance — the LLM-specific machinery (streaming loops, retries, mocks, tool schemas, judges, error envelopes) should be expressed through a small number of first-class, derivable, composable primitives rather than hand-rolled by users, because hand-rolled versions drift, duplicate, and leak. The recurring moves are: derive the second artifact from the first (schema from function, partial type from type, sync/async bindings from one declaration), keep orchestration and policy at the caller/config layer rather than inlined in bodies, and reuse the ordinary function/expression language instead of inventing DSLs (judges are functions, runners are functions, mock matchers are lambdas). A codebase "fights" this design whenever it reimplements one of these primitives with weaker general-purpose code — manual retry loops, parallel hand-written schemas, string-matched errors, callback plumbing around streams.

        ---

        ### P-009-1: Consume parsed partials by iterating the stream, not via callbacks
        - **Source**: BEP-009 (Streaming, status: implemented)
        - **Principle**: Parsed partials are delivered exclusively through stream iteration (`stream.next()` in BAML, `async for partial in stream` in Python/TS); there is deliberately no `on_partial` callback because "the loop IS the callback" and adding one would be redundant plumbing.
        - **Intended usage**:
          ```python
          stream = b.stream.ExtractReceiptInfo(receipt)
          async for partial in stream:
              render(partial)          # Partial[T] — all fields Optional
          final = await stream.get_final_response()
          ```
          In BAML: `while (true) { let p = stream.next(); if (p == null) { break; } ... }` then `stream.final()`.
        - **Anti-patterns / slop signals**:
          - Registering ad-hoc callbacks / queues / event emitters around a BAML stream to deliver partials elsewhere instead of consuming the iterator directly.
          - Calling the non-streaming form and polling/re-calling to fake progressive output.
          - Ignoring `get_final_response()` / `stream.final()` and treating the last partial as the validated final value (partials are permissive-parsed; the final is strictly parsed).
        - **Applicability trigger**: Any host or BAML code that touches `b.stream.*` or builds a real-time/progressive UI over an LLM function.
        - **Detectability**: static — grep for `b.stream` usage without a final-response call, or partial-delivery plumbing (queues/callbacks) wrapped around the iterator.

        ### P-009-2: `on_tick` is the one user-facing hook, and only for raw SSE access
        - **Source**: BEP-009 (Streaming, status: implemented)
        - **Principle**: `on_tick` exists solely to give power users the raw provider SSE events (thinking-token deltas, tool-call deltas) that the parser doesn't surface. It receives batches of `SseEvent`s directly — the legacy hidden-collector workaround was explicitly removed.
        - **Intended usage**:
          ```python
          def on_tick(events):
              for e in events:
                  data = json.loads(e.data)
                  if "thinking" in data.get("delta", {}): ...
          b.stream.Fn(x, baml_options={"on_tick": on_tick})
          ```
        - **Anti-patterns / slop signals**:
          - Using `on_tick` to accumulate content and re-parse it (reimplementing `emit_partial`/the accumulator by hand).
          - Creating a Collector purely to read raw SSE responses per tick (the legacy indirection the rewrite removed).
          - Relying on per-event timing/ordering of tick invocations — delivery is batched by design and chunk boundaries are network-dependent.
        - **Applicability trigger**: Code passing `on_tick` in `baml_options`, or code digging into raw SSE / provider deltas.
        - **Detectability**: hybrid — statically find `on_tick` handlers; LLM judgment to decide whether the handler is raw-event access (fine) or a shadow partial-parser (slop).

        ### P-009-3: Retry/fallback/round-robin live in client configuration, shared by streaming and non-streaming
        - **Source**: BEP-009 (Streaming, status: implemented)
        - **Principle**: Orchestration (retry policies, fallback chains, round-robin) is written once and shared identically between streaming and non-streaming paths; the design's whole point was eliminating duplicated orchestration. Users express resilience through client config, not per-call-site loops.
        - **Intended usage**: Declare `retry_policy`, fallback, or round-robin clients on the BAML client; call the function normally. On total failure the runtime panics with "All orchestration steps failed".
        - **Anti-patterns / slop signals**:
          - Hand-written retry loops in host code or BAML around an LLM function call when a `retry_policy`/fallback client would express the same thing.
          - Separate retry logic for the streaming call path vs the non-streaming path of the same function.
          - Catching the terminal orchestration failure and retrying the whole function again in an outer loop (double orchestration).
        - **Applicability trigger**: Any loop or try/except-retry wrapped around a BAML LLM function invocation.
        - **Detectability**: static — retry loops around `b.Fn(...)` calls are grep/AST-findable; check whether the client already declares (or could declare) the equivalent policy.

        ### P-009-4: Control streaming granularity with `@stream.done` / `@stream.with_state` attributes, not consumer-side filtering
        - **Source**: BEP-009 (Streaming, status: implemented)
        - **Principle**: The type system is the place to say "this field/object should appear atomically" or "carry completion state" — the SAP uses these attributes to decide what to emit, so consumers never see half-formed values they'd otherwise have to filter.
        - **Intended usage**:
          ```baml
          class ReceiptItem { name string  quantity int  @@stream.done }
          class ReceiptInfo { store_name string @stream.done  notes string @stream.with_state }
          ```
        - **Anti-patterns / slop signals**:
          - Host code null-checking / length-checking / debouncing partial fields to avoid rendering incomplete values, where `@stream.done` on the type would do it.
          - Manually tracking "is this field finished yet" state that `@stream.with_state` provides.
        - **Applicability trigger**: Streaming consumers with validation/completeness logic on partials; BAML types used as streaming return types.
        - **Detectability**: hybrid — attributes are grep-able in .baml; judging whether consumer-side checks duplicate them needs semantic review.

        ### P-009-5: End streams early with cancellation, not by draining
        - **Source**: BEP-009 (Streaming, status: implemented)
        - **Principle**: Early cancellation is a first-class streaming use case: `stream.close()` in BAML or an `AbortController` in host code stops the SSE task and the VM at the next safepoint, saving tokens and latency.
        - **Intended usage**:
          ```baml
          while (true) { let p = stream.next(); if (p == null) { break; }
            if (p.items.length() >= 10) { stream.close(); break; } }
          ```
        - **Anti-patterns / slop signals**:
          - Breaking out of the consumer loop without closing/aborting, leaving the LLM call running to completion.
          - Discarding partials client-side after a condition is met while still paying for the full generation.
        - **Applicability trigger**: Streaming consumers with early-exit conditions (`break` before stream end).
        - **Detectability**: static — find `break`/early-return in stream loops without a `stream.close()` / `controller.abort()`.

        ### P-058-1: Mock by naming the callable reference, inside a scope — never by restructuring code for testability
        - **Source**: BEP-058 (Mock, status: accepted)
        - **Principle**: `baml.mock.new(callable)` + `baml.mock.scope(m, body)` exists so tests can replace any function (including sysops like `baml.http.fetch`) for a bounded dynamic extent with zero teardown, without string paths, dependency injection, codegen doubles, or proxies. The compiler resolves the target's identity; effects auto-undo at scope exit (even on unwind).
        - **Intended usage**:
          ```baml
          let m = baml.mock.new(baml.http.fetch);
          m.replace((req: Request) -> Response { fake_response });
          baml.mock.scope(m, () -> void { let r = my_pipeline(); ... });
          ```
        - **Anti-patterns / slop signals**:
          - Threading fake clients / "http provider" parameters through function signatures purely so tests can inject a stub (DI restructuring the design explicitly rejects).
          - Global flags like `if (TEST_MODE) { return canned }` inside production functions.
          - Mocks installed without a `scope` (or manual "reset" bookkeeping) — global mocking is a stated non-goal.
          - Mocking from generated host-language clients — host-language exposure is a non-goal; mock in BAML tests.
        - **Applicability trigger**: BAML test code that needs to suppress side effects (HTTP, clock, randomness) several layers deep; production code shaped around injectability.
        - **Detectability**: hybrid — `baml.mock` usage is grep-able; detecting DI-for-testability contortions in production signatures needs LLM judgment.

        ### P-058-2: The replacement lambda is the matcher; conditional stubbing needs no DSL
        - **Source**: BEP-058 (Mock, status: accepted)
        - **Principle**: BEP-058 explicitly rejects a `when(x).thenReturn(...)` matcher DSL and argument-recording assertions: the replacement is a full-power typed function, so conditional behavior is an `if` inside it and argument capture is closure capture.
        - **Intended usage**: Write branching directly in the replacement (`m.replace((x: int) -> int { if (x > 0) { 1 } else { -1 } })`); capture args by closing over a list the replacement appends to.
        - **Anti-patterns / slop signals**:
          - Building a homegrown matcher/expectation framework on top of `Mock` (call-order recorders, fluent stub builders).
          - Simulating `assertCalledWith` via fragile global state instead of closure capture in the replacement.
        - **Applicability trigger**: Test helper libraries or utilities wrapping `baml.mock`.
        - **Detectability**: semantic — reviewer judges whether mock helpers reintroduce a second stubbing DSL.

        ### P-058-3: Use spy mode and live `super` instead of hand-wrapping real functions
        - **Source**: BEP-058 (Mock, status: accepted)
        - **Principle**: Omitting `.replace` makes a pure spy (real function runs, `call_count` recorded); `m.super()` is a live callable to the implementation one layer down, so wrappers/partial mocks delegate correctly even when the chain shifts. `call_count` resets per scope so counts are meaningful per test block, and (per comment-thread decision) spawned tasks share the same mock and the same count — reliable after awaiting the spawn.
        - **Intended usage**: `a.replace(() -> string { "A:" + a.super() })` for wrap-and-delegate; a `scope` with no `replace` to just count calls.
        - **Anti-patterns / slop signals**:
          - Manually wrapping a function in a counting shim (a second function that increments a counter then calls the real one) to verify invocation.
          - Re-calling the mocked target by name inside a replacement (recursion-guarded, but the intent should be `super`).
          - Asserting on `call_count` for spawned work before awaiting the spawn's future (known race per design).
        - **Applicability trigger**: Tests that verify "was X called" or wrap real behavior.
        - **Detectability**: static for counting-shim patterns and by-name recursion; the spawn/await ordering check is hybrid.

        ### P-059-1: The function is the tool's source of truth — schema and dispatch are derived, never written twice
        - **Source**: BEP-059 (Functions as Tools, status: **draft** — principles are directional, but the anti-pattern it targets is well-established)
        - **Principle**: Tool calling should not require a hand-written class mirroring a function's parameters plus glue that turns the filled class back into a call — that pair drifts (add a parameter and the schema silently lies). The compiler-generated `f$tool` companion (fields = params, `execute()` = body) is the intended shape; it repeats the derivation move of BEP-006 (streaming types) and BEP-012 (`$render_prompt`).
        - **Intended usage**:
          ```baml
          function add(a: int, b: int) -> int { a + b }
          function PickMath(q: string) -> add$tool | subtract$tool { client ... prompt #"... {{ ctx.output_format }}"# }
          let answer = PickMath("2 plus 2").execute()
          ```
        - **Anti-patterns / slop signals**:
          - A `class AddArgs { a int  b int }` maintained in parallel with `function add(a, b)` plus a `match`/if-chain that unpacks fields into the call — the exact boilerplate the BEP derives away. (Pre-`$tool` availability this is tolerable but should at least be co-located and clearly mirrored; drift between the class and function is hard slop.)
          - Tool-name string dispatch (`if name == "add" ...`) where a union of typed tool classes would carry the discrimination.
          - Hand-written per-provider JSON Schema for tools alongside BAML types.
        - **Applicability trigger**: Any BAML agent code where an LLM output selects among actions / returns "which tool + arguments".
        - **Detectability**: hybrid — parallel arg-class + dispatch-glue is AST-visible; judging drift/mirroring quality is semantic.

        ### P-059-2: The model's schema must be a strict subset of the call — host context is never LLM-fillable
        - **Source**: BEP-059 (Functions as Tools, status: **draft**)
        - **Principle**: Real tools mix LLM-decided inputs with host-supplied context (DB handles, auth tokens, HTTP clients) that the model must never invent. The design marks the cut (`*` separator: params after it become `execute` arguments, hidden from the model); the invariant — the model never sees or fills host parameters — holds regardless of syntax.
        - **Intended usage**: `function get_weather(city: string, *, http: HttpClient) -> string`; the model fills `city`, the host injects `http` at `t.execute(http)`.
        - **Anti-patterns / slop signals**:
          - Tool-argument classes exposing secrets, connection handles, user IDs, or auth material as LLM-fillable fields.
          - Prompts asking the model to echo back host context so it "round-trips" into the tool call.
        - **Applicability trigger**: Any type used as an LLM output that later parameterizes real side-effecting code.
        - **Detectability**: hybrid — field names like `token`/`api_key`/`db` in LLM output types are grep-able red flags; whether a field is truly host context needs judgment.

        ### P-023-1: LLM evaluation is just tests — judges are functions, datasets are functions, evals are testsets; no parallel eval harness
        - **Source**: BEP-023 (Test and asserts, status: implemented)
        - **Principle**: BAML deliberately has no scorer/dataset/eval keywords: an LLM-as-judge is a regular BAML function whose score feeds an `assert`, a dataset is a function returning an array, an eval is a `testset` with a `PassRate` runner. The existing primitives compose to express any evaluation pattern, and judges stay playground-renderable.
        - **Intended usage**:
          ```baml
          test "translate quality" {
            let result = Translate("French", "hello")
            let score = JudgeQuality("hello", result)
            assert.is_true(score.value >= 0.7)
          }
          ```
        - **Anti-patterns / slop signals**:
          - Quality evaluation living in a separate host-language script (Python eval harness looping over cases, calling the BAML function, scoring by hand) when the same shape fits `testset` + `for` + judge function.
          - Judge prompts embedded as raw strings in host code instead of as BAML functions (losing type-checking and prompt preview).
          - A bespoke "scorer registry" or eval config format alongside the test suite.
        - **Applicability trigger**: Any code measuring LLM output quality — accuracy checks, judge calls, pass-rate computation.
        - **Detectability**: hybrid — host-side eval loops over BAML functions are findable; deciding they duplicate testset capability is semantic.

        ### P-023-2: Parameterize with `testset` + loops over data; never clone test blocks or hide N cases in one test
        - **Source**: BEP-023 (Test and asserts, status: implemented)
        - **Principle**: `testset` bodies execute at collection time precisely so tests can be generated from data (arrays, CSVs, even LLM-generated cases) and each case appears as its own addressable, filterable, individually-reported test in the tree before anything runs.
        - **Intended usage**:
          ```baml
          testset "translations" {
            let cases = [...]
            for (let case in cases) {
              test "translate " + case.language { ... assert.contains(result, case.expected) }
            }
          }
          ```
        - **Anti-patterns / slop signals**:
          - Dozens of copy-pasted near-identical `test` blocks differing only in literals (the exact problem the BEP opens with).
          - One giant `test` that loops over all cases internally — a single failure hides which case broke and defeats filtering, per-case reporting, and PassRate.
          - Lingering legacy declarative `functions [...] / args {...}` tests in a codebase otherwise on the new syntax (mechanical `baml format` migration exists).
          - Implementation-style identifier names (`test "TestParseURL1"`) instead of behavior-describing strings.
        - **Applicability trigger**: Any group of ≥3 tests over the same function, or any test containing a data loop with per-item asserts.
        - **Detectability**: static — repetitive test blocks and for-loop-with-asserts-inside-one-test are AST/grep detectable.

        ### P-023-3: Handle nondeterminism with runners — Quorum for quality distributions, Retry for infrastructure, PassRate for suites
        - **Source**: BEP-023 (Test and asserts, status: implemented)
        - **Principle**: A single run of a nondeterministic LLM function is a sample, not a proof. The `with` clause attaches statistical execution policy without touching the test body — and the BEP insists Quorum (N runs, M must pass: measures a quality distribution) and Retry (stop on first success: absorbs transient failures) are *not the same thing*. `PassRate(0.x)` on a testset is the standard ML "threshold over a collection" pattern. Runners are ordinary functions, so custom policies need no framework.
        - **Intended usage**: `test "..." with testing.Quorum(5, 3) { ... }`; `testset "eval" with testing.PassRate(0.7) { ... }`; both compose (per-test Quorum inside a PassRate testset).
        - **Anti-patterns / slop signals**:
          - Hand-rolled repetition loops inside test bodies (`for i in 0..5 { ... count passes ... }`) reimplementing Quorum.
          - Using Retry semantics (pass on any success) to mask genuine quality nondeterminism — inflates apparent quality; the BEP names this conflation explicitly.
          - CI that runs a flaky LLM test once and treats a single red as failure, or blanket-retries the whole suite at the CI-config level.
          - Computing suite pass rates in an external script instead of `PassRate` on the testset.
        - **Applicability trigger**: Any test whose body invokes an LLM-backed function; any suite with flaky-test mitigation.
        - **Detectability**: hybrid — missing `with` on LLM-calling tests and in-body repetition loops are static; Retry-vs-Quorum misuse needs judgment about what's being retried.

        ### P-023-4: Tests are self-contained — setup is a visible function call, never a hidden hook
        - **Source**: BEP-023 (Test and asserts, status: implemented)
        - **Principle**: BAML rejects `before_each`/`after_each`/`after_all` by design: a reader must understand a test top-to-bottom without reconstructing hidden context. Shared setup is `let` bindings in the testset body (which run once at collection — the "before_all") or an explicit helper function called at the top of each test.
        - **Intended usage**: `function fresh_test_db() -> TestDB { ... }` called on the first line of each test; shared constants as testset-level `let`s.
        - **Anti-patterns / slop signals**:
          - Simulated lifecycle hooks: mutable module state initialized by a "setup test" that other tests depend on, or order-dependent tests (parallel-by-default breaks these anyway — sequencing needs explicit `testing.Sequential()`).
          - Setup/teardown smeared across helper state that a test body doesn't visibly invoke.
          - Assertion helpers that bypass `assert.*` (e.g. returning booleans that nothing checks, or panicking manually with unstructured messages).
        - **Applicability trigger**: Testsets with shared mutable state or tests whose correctness depends on execution order.
        - **Detectability**: hybrid — order-dependence and phantom-setup tests need semantic review; non-`assert.*` checking is grep-able.

        ### P-036-1: Encode quality criteria as tests so they double as optimization objectives
        - **Source**: BEP-036 (baml optimize, status: proposed)
        - **Principle**: `baml-cli optimize` uses the tests that invoke a function as GEPA's benchmark. Tests are therefore not just regression gates but the machine-readable definition of "good output" — a function without meaningful asserting tests is unoptimizable and its quality bar is undocumented.
        - **Intended usage**: For each LLM function, maintain a testset whose asserts capture the real acceptance criteria (including tricky cases like "age belongs to the object, not the subject"), discoverable by direct call or via `--via` helpers.
        - **Anti-patterns / slop signals**:
          - LLM functions with zero tests, or tests that call the function but assert nothing beyond `not_null` (no criterion to optimize against or regress against).
          - Quality criteria living only in prose comments/READMEs rather than executable asserts.
        - **Applicability trigger**: Every LLM-backed `function` in the codebase — check for asserting test coverage.
        - **Detectability**: static — cross-reference LLM functions against tests that call them and inspect assert density.

        ### P-036-2: Prompts must specify output format idiomatically via `{{ ctx.output_format }}`
        - **Source**: BEP-036 (baml optimize, status: proposed)
        - **Principle**: The BEP's worked example treats a prompt lacking output-format specification as a "fundamental error" that makes all tests fail, and names "include `{{ ctx.output_format }}`" as the idiomatic-to-BAML fix the optimizer should apply — the type-derived format block, not hand-written JSON instructions.
        - **Intended usage**: Structured-output prompts end with `{{ ctx.output_format }}`; the schema text is derived from the return type (tunable via `description`/`alias` on fields).
        - **Anti-patterns / slop signals**:
          - Prompts hand-describing the expected JSON shape in prose ("respond with a JSON object with keys name, age...") instead of / duplicating `{{ ctx.output_format }}` — a shadow schema that drifts from the return type.
          - Structured-return LLM functions with no output-format directive at all.
        - **Applicability trigger**: Any LLM function whose return type is a class/enum/union.
        - **Detectability**: static — grep prompts of structured-return functions for `ctx.output_format` and for hand-written JSON-shape prose.

        ### P-036-3: Shared prompt components (type descriptions, template strings) couple functions — factor with attribution in mind
        - **Source**: BEP-036 (baml optimize, status: proposed)
        - **Principle**: A prompt is assembled from the function's prompt text, template strings, and the output type's `description`/`alias` fields; because those can be shared, tuning one function can silently de-optimize another. The BEP flags a real tension between factoring prompt text and keeping prompts attributable/optimizable per function.
        - **Intended usage**: Reuse template strings and type descriptions deliberately, aware that they're part of every sharing function's optimization surface; keep function-specific instructions in the function's own prompt.
        - **Anti-patterns / slop signals**:
          - One mega `template_string` embedding behavior-critical instructions for many unrelated functions (any tweak is a blast radius).
          - Conversely, copy-pasting identical prompt blocks across functions instead of a template string when they genuinely must stay in lockstep.
        - **Applicability trigger**: `template_string` definitions and `@description` fields referenced from multiple LLM functions.
        - **Detectability**: static for the sharing graph; semantic for whether the coupling is intentional.

        ### P-040-1: Cross-cutting execution policy composes as flat `with` middleware, not inline loops or nested wrappers
        - **Source**: BEP-040 (Middleware Blocks, status: **draft** — treat as directional; the inline-retry-loop anti-pattern it motivates is valid today, the `run` syntax is not yet shipped)
        - **Principle**: Retry, timeout, timing, fallback, and circuit-breaking are cross-cutting concerns that should wrap a block declaratively (`run "name" with withRetry(3), withTimeout(...) { body }`) rather than being written as inline loops per call site or nested HOFs that bury the body three levels deep. The transform-then-execute model (middleware = `Params -> Params`, no `next()`) keeps composition ordering explicit and meaningful (`withRetry, withTiming` times the total; reversed, each attempt).
        - **Intended usage**: Stdlib `baml.withs.*` for `run`, `baml.future.*` for `spawn`; definition-site `with` on a function when the author owns the policy, call-site `with` when the caller does.
        - **Anti-patterns / slop signals** (mark: from a draft — grade softly):
          - The same hand-written retry/timeout loop appearing at multiple call sites (the BEP's own "today" example).
          - `spawn`-and-immediately-`await` purely to get middleware on synchronous work — pays green-thread overhead and falsely signals concurrency.
          - Deeply nested HOF wrapping (`withTiming(withTimeout(withRetry(...)))`) where the flat `with` chain exists.
        - **Applicability trigger**: Repeated resilience/observability boilerplate around flaky IO, LLM calls, or external APIs in BAML code.
        - **Detectability**: static — repeated retry-loop shapes and spawn-await-immediately are pattern-matchable.

        ### P-040-2: Resource budgets (cost, tokens) are scoped, caller-set policy — not baked into individual functions
        - **Source**: BEP-040 (Middleware Blocks, status: **draft**)
        - **Principle**: Agent budgets apply to a *scope* of work spanning many LLM calls, and the right limit differs per call site (background job $0.50, interactive request $5.00) — so budgets belong to the caller as middleware (`cost_limit`, `token_limit`) wrapping the block, with metered functions unaware of the metering.
        - **Intended usage**: `run "agent-turn" with baml.withs.cost_limit(dollars = 5.00) { plan; research; draft; refine }` — budget errors (`CostLimitExceeded`) thrown when the cap hits.
        - **Anti-patterns / slop signals**:
          - Hard-coded spend/token caps inside individual LLM-calling functions, forcing every caller into one policy.
          - Manual cost accounting threaded through function parameters and return values (a hand-rolled meter).
        - **Applicability trigger**: Agent loops or pipelines with any cost/token limiting logic.
        - **Detectability**: semantic — requires judging where budget policy lives relative to who should own it.

        ### P-056-1: Host code catches `BamlError`/`BamlPanic` wrappers and inspects the typed `.value` payload — never string-matches messages
        - **Source**: BEP-056 (Errors across bridges, status: implemented)
        - **Principle**: BAML `throws T` cannot become a typed host exception (Python can't subclass both `BaseException` and `pydantic.BaseModel`, can't raise a BaseModel, has no checked-exception typing), so the design is a uniform envelope: functions raise `BamlError` carrying the thrown BAML value as `.value` (codegen'd under normal rules), and runtime panics raise `BamlPanic`. Error identity is `isinstance(e.value, SomeErrorType)`.
        - **Intended usage**:
          ```python
          from baml_sdk.baml import BamlError, BamlPanic
          try:
              extract_resume()
          except BamlError as e:
              if isinstance(e.value, baml.errors.InvalidArgument): ...
          ```
        - **Anti-patterns / slop signals**:
          - Matching on `str(e)` / message substrings to classify BAML failures.
          - Catching the retired hardcoded types (`BamlInvalidArgumentError`, `BamlClientError`, `BamlCancelledError`) in new code — replaced by `.value` payload checks.
          - Expecting `except MyBamlErrorType:` to work directly (BAML error classes are pydantic models, not exceptions).
        - **Applicability trigger**: Any host-language try/except around generated BAML function calls.
        - **Detectability**: static — exception-handler patterns around `baml_sdk` calls are grep/AST checkable.

        ### P-056-2: Panics (including cancellation) are `BaseException`-level and must not be swallowed by broad `except Exception`
        - **Source**: BEP-056 (Errors across bridges, status: implemented)
        - **Principle**: `BamlPanic` deliberately subclasses `BaseException` (like `asyncio.CancelledError`, `SystemExit`) so that `except Exception:` does not accidentally absorb panics or cancellation; cancellation is classified as a panic (`baml.panics.Cancelled` in `p.value`), invalid-argument as an ordinary error, and `baml.sys.exit(n)` becomes a real `os._exit(n)`.
        - **Intended usage**: Handle cancellation via `except BamlPanic as p: if isinstance(p.value, baml.panics.Cancelled): ...`; let panics propagate by default.
        - **Anti-patterns / slop signals**:
          - `except Exception:` around BAML calls with the expectation it handles cancellation/abort (it won't — and shouldn't).
          - Catching `BamlPanic` broadly and continuing as if it were a recoverable domain error.
        - **Applicability trigger**: Host code using abort controllers / cancellation, or blanket exception handlers around BAML calls.
        - **Detectability**: static — broad `except` clauses around `baml_sdk` calls plus cancellation usage are pattern-findable.

        ### P-030-1: Use the generated `baml_sdk` bindings as-is — idiomatic per host language, with paired sync/`_async` variants
        - **Source**: BEP-030 (Python Codegen Rules, status: proposed)
        - **Principle**: Codegen's guiding rule is "most language-idiomatic option per host" (explicitly learning from protobuf's mistake of degrading Python for cross-language uniformity). Every BAML function gets both `f()` and `f_async()`; user symbols land at the `baml_sdk` root, stdlib under `baml_sdk.baml`, vendor under `baml_sdk.vendor`, stream types under `baml_sdk.stream_types`.
        - **Intended usage**: `from baml_sdk.lorem import extract_resume, extract_resume_async`; pick the variant matching your caller's concurrency model; consume `stream_types` partials as the streaming counterparts.
        - **Anti-patterns / slop signals**:
          - Hand-written wrappers that re-expose generated functions (a shadow client layer), or `asyncio.run()` / event-loop shims wrapping the sync variant when `_async` exists (and vice versa: `run_in_executor` around sync calls instead of using `_async`).
          - Hand-maintained host-side dataclasses mirroring BAML types instead of the generated ones.
          - BAML namespaces named `vendor`, `baml`, or `stream_types` (banned — they collide with codegen layout), or user symbols relying on `__` in names (collision-warned).
        - **Applicability trigger**: Any host code importing from `baml_sdk` / the generated client; BAML namespace declarations.
        - **Detectability**: static — imports, wrapper layers, and banned namespace names are grep-able.

        ### P-030-2: Use the companion (modular) API for request-level control instead of reconstructing provider calls
        - **Source**: BEP-030 (Python Codegen Rules, status: proposed; companion mechanism per BEP-012)
        - **Principle**: LLM-backed functions get compiler-generated companions — `f__build_request()`, `$render_prompt`, `$parse`, and `f_stream()` — precisely so hosts needing lower-level control (inspect the prompt, send the HTTP request themselves, parse a captured response) use derived, always-in-sync artifacts rather than reimplementing prompt rendering or provider request construction.
        - **Intended usage**: `req = extract_resume__build_request(args)` then send via your own HTTP stack; `extract_resume_stream()` for the streaming API; feed captured JSON back through the parse companion for token-free deterministic tests.
        - **Anti-patterns / slop signals**:
          - Host code assembling provider request bodies / re-rendering Jinja prompts by hand for a function that has `__build_request` / `$render_prompt` companions.
          - Re-parsing raw LLM text with ad-hoc JSON handling instead of the function's parse companion (bypasses SAP and the type's coercion rules).
        - **Applicability trigger**: Host code doing anything with prompts, raw requests, or raw LLM responses for BAML-defined functions.
        - **Detectability**: hybrid — hand-built provider payloads next to a BAML function are findable statically; confirming the companion covers the need is semantic.
        # Batch: Observability, execution & tooling (BEPs 50, 53, 54, 26, 27, 28, 35)

        Batch philosophy: This batch's dominant theme is *pulling concerns that other ecosystems leave to external infrastructure into the typed language itself* — metrics bind to functions in-source (BEP-50), CLIs derive from signatures (BEP-27), debug formatting auto-derives from class structure (BEP-26) — so that renames, type changes, and drift become compile errors instead of silent breakage. A second recurring commitment is *keeping core values plain and clean*: return types are never polluted with trace IDs, caught error values stay bare (catch-as-match), and audience (user vs. developer output) is chosen explicitly at the call site rather than inferred. Third, the BEPs consistently prefer *one mechanism over two*: one run verb, one interpolation grammar, one Id API for live and retroactive tracing, and a strict compile-time/runtime split for attributes.

        Status note: BEP-27 and BEP-28 are **implemented** (strongest grading signal); BEP-26 is **pending**; BEP-50, 53, 54 are **proposed** (grade only if the codebase uses or reimplements these features); BEP-35 is **superseded** and meta-process only — no principles.

        ---

        ### P-050-1 [PROPOSED]: Metrics are language-level, co-located with the function they measure
        - **Source**: BEP-50 (Metrics and Tracing IDs, status: proposed)
        - **Principle**: Evaluation of non-deterministic functions belongs in the language, not in external eval scripts, YAML configs, or dashboard rules — because external bindings break silently on rename, while a `metric` block is type-checked against the function's signature and versioned in git.
        - **Intended usage**: Declare a `metric <fn_name> { ... }` block next to the function; metric functions reference `input`/`output`/`error` built-ins and declared data fields (`expected: Resume`), all compile-checked.
        - **Anti-patterns / slop signals**:
          - Scoring/eval logic implemented as ordinary BAML functions that take output+expected and are wired up by hand in test bodies, when a `metric` block exists in the toolchain.
          - Metric names or function names passed around as raw strings (rename-unsafe binding).
          - Evaluation living entirely in host-language scripts or config files outside the BAML source.
        - **Applicability trigger**: Any code chunk that computes quality/score/eval values for an LLM function's outputs, or any test that manually threads expected values into scorer functions.
        - **Detectability**: Hybrid — grep can find `metric` blocks vs. hand-rolled `*_score`/`eval_*` functions; LLM judgment needed to account for the BEP being unshipped.

        ### P-050-2 [PROPOSED]: Trace identity is opt-in at the call site and never pollutes return types
        - **Source**: BEP-50 (Metrics and Tracing IDs, status: proposed)
        - **Principle**: `$id` is a keyword-only system parameter (`$` prefix = system, zero collision risk); `resume.$id` does not exist because embedding trace identity in return values would pollute every data type with a hidden field. Types stay clean; tracing is a call-site decision.
        - **Intended usage**:
          ```baml
          let id = boundary.id()              // LocalId, single-use
          let resume = extract_resume(doc, $id = id)
          id.set_expected(ground_truth)
          let f1 = await id.get_f1()
          ```
        - **Anti-patterns / slop signals**:
          - Adding a `trace_id`/`id: string` field to domain classes or return types to carry trace identity.
          - Threading a trace ID as an ordinary positional/named parameter through the call graph instead of using `$id`.
          - Reusing one `LocalId` across two function calls (a runtime error by design — makes trace attribution ambiguous).
          - Using the wildcard API (`id.set("name", val)`, `id.get<float>("name")`) permanently for stable, known metrics — wildcard is for experimentation; graduated metrics should be declared fields with typed accessors.
        - **Applicability trigger**: Any code that creates, passes, or stores trace/correlation identity around LLM function calls.
        - **Detectability**: Hybrid — static grep for `trace_id`-shaped fields on domain classes and string-based `id.set(`/`id.get<` calls; semantic judgment for whether wildcard use is exploratory or entrenched.

        ### P-050-3 [PROPOSED]: Express metric timing, composition, and sampling as data dependencies, not control flow or config
        - **Source**: BEP-50 (Metrics and Tracing IDs, status: proposed)
        - **Principle**: When a metric runs is derived entirely from its parameter names (the reactive DAG): a metric fires when its inputs exist. Shared expensive computation (LLM-as-judge) is a DAG node that runs once and fans out; sampling is a `signal: boundary.UploadSignal` field the platform fills — not annotations, `@spawn`, or imperative rate checks.
        - **Intended usage**:
          ```baml
          metric extract_resume {
            signal: boundary.UploadSignal
            function judge(input, output, signal) -> QualityScore { JudgeQuality(input, output) }
            function quality(judge) -> float { judge.value }        // judge runs ONCE, fans out
            function f1(precision, recall) -> float { ... }
          }
          ```
        - **Anti-patterns / slop signals**:
          - Calling the same LLM judge function separately for each derived score (identical prompt, duplicated cost) instead of sharing one DAG node.
          - Imperative sampling inside function bodies (`if random() < 0.1 { run_judge(...) }`) instead of a signal dependency.
          - Hand-rolled scheduling/spawn logic to make metrics "run in the background" — background execution is the only mode, with failure isolation by default.
          - Bundling unrelated scores into one judge return type just to save calls in a hacky way — prefer honest field names like `judge.quality` / `judge.faithfulness`.
        - **Applicability trigger**: Metric blocks, or any orchestration that runs LLM judges/scorers over outputs with sampling or shared sub-computations.
        - **Detectability**: Semantic — an LLM reviewer must trace whether judge calls are duplicated and whether gating is expressed as data vs. control flow.

        ### P-050-4 [PROPOSED, accepted in comments]: Batch assertions use `after` blocks, not single-dimensional pass-rate hacks
        - **Source**: BEP-50 (Metrics and Tracing IDs, status: proposed; the `after` block was explicitly accepted in comments)
        - **Principle**: `testing.PassRate` was "a single-dimensional batch assertion disguised as a runner"; an `after` block on a testset is arbitrary code that runs after all sibling tests and can express any multi-dimensional batch condition (hard gates + soft gates simultaneously), with shared testset scope.
        - **Intended usage**:
          ```baml
          testset "eval" {
            let ids: extract_resume.Id[] = []
            for case in cases { test "case " + case.id { ...; ids.push(id) } }
            after {
              for id in ids { assert.is_true(await id.get_faithfulness() >= 0.9) }  // hard gate
              assert.is_true(pass_count / ids.len >= 0.8)                            // soft gate
            }
          }
          ```
        - **Anti-patterns / slop signals**:
          - Using or reimplementing `testing.PassRate`-style single-threshold aggregation when multi-dimensional conditions are wanted.
          - Aggregating batch statistics inside individual `test` bodies via shared mutable counters and asserting in the "last" test (order-dependent, fragile).
          - Duplicating per-test assertions in the batch block when they belong in the tests.
        - **Applicability trigger**: Testsets that evaluate LLM functions over many cases and need aggregate thresholds.
        - **Detectability**: Static-leaning hybrid — grep for `PassRate` and for `after` blocks; semantic check for counter-hacks emulating batch assertions.

        ### P-053-1 [PROPOSED]: Semantic identity must be invariant to non-semantic edits
        - **Source**: BEP-53 (Language Semantic Identity and Runtime Trace Model, status: proposed)
        - **Principle**: Code-version identity is a change-classification system, not a hash of text: comments, docstrings, formatting, source spans, local slot numbers, and `HashMap` iteration order must never affect semantic version signals; source snapshots (what the user sees) and semantic identity (what behavior groups on) are separate artifacts.
        - **Intended usage**: Any code computing "did this function change?" derives identity from resolved, typed structure (signature, body semantics, config) and keeps a separate `source_snapshot_id` for display; a docstring-only edit yields `source_snapshot changed, semantic lanes unchanged`.
        - **Anti-patterns / slop signals**:
          - Hashing raw source text or raw AST (including spans/line numbers) as a version identity.
          - Identity that changes when files/maps are reordered or when arena/local IDs shift.
          - Hashing environment variable *values* (only referenced names/config paths may participate).
          - Hashing runtime values (arguments, outputs, LLM responses) into code identity.
        - **Applicability trigger**: Any code implementing versioning, caching keys, trace grouping, or change detection over BAML definitions.
        - **Detectability**: Hybrid — static inspection of what feeds the hash input; semantic review to confirm span/runtime data can't leak in.

        ### P-053-2 [PROPOSED]: Distinguish interface vs. implementation and direct vs. effective change signals
        - **Source**: BEP-53 (Language Semantic Identity and Runtime Trace Model, status: proposed)
        - **Principle**: Four lanes (`direct_interface`, `effective_interface`, `direct_implementation`, `effective_implementation`) exist because each preserves a product distinction — one project hash can only say "something changed"; direct-only misses that `ExtractResume` is affected when returned type `Resume` changes through `Contact`. Effective lanes must be computed from *typed dependency edges*, not textual includes.
        - **Intended usage**: A helper-body edit changes the helper's `direct_implementation` and callers' `effective_implementation` only; the consumer-facing contract is explanatory sentences ("prompt changed while schema held constant"), with hashes as opaque signals.
        - **Anti-patterns / slop signals**:
          - A single per-function hash used to answer both "did the contract change?" and "did behavior change?"
          - Treating the hash value itself as the product API instead of the change classification.
          - Computing "affected functions" by grepping source text instead of walking the dependency graph.
          - Two independent hash implementations (one canonical computation path is mandated), or `DefaultHasher`/`u64` as a durable cross-deployment identity instead of a versioned canonical serialization + full-width hash.
        - **Applicability trigger**: Code implementing version fingerprints, eval comparison keys, rollout/cache-invalidation logic, or change explanations.
        - **Detectability**: Semantic — requires reading the hashing/versioning design; a static check can flag `DefaultHasher`/`u64` in durable-identity paths.

        ### P-053-3 [PROPOSED]: Runtime trace identity is separate from semantic identity, and traces join to code via stable keys
        - **Source**: BEP-53 (Language Semantic Identity and Runtime Trace Model, status: proposed)
        - **Principle**: `trace_id` (one root invocation tree), `span_id` (one timed operation within it), and `event_id` answer "what happened"; `definition_key` + semantic lanes answer "which code version" — and the runtime must not recompute semantic hashes on the hot path, only emit stable keys and snapshot references.
        - **Intended usage**: Trace events carry `{trace_id, span_id, event_id, source_snapshot_id, definition_key, payload}`; grouping "everything in this user call" uses `trace_id`; nested operations/retries/spawned futures use spans.
        - **Anti-patterns / slop signals**:
          - Conflating trace IDs with code-version IDs (e.g., grouping eval runs by trace ID, or keying caches on runtime IDs).
          - Computing semantic fingerprints per-invocation at runtime.
          - Only emitting span IDs and reconstructing trees by walking parent pointers everywhere, or only trace IDs with no intra-tree structure.
        - **Applicability trigger**: Instrumentation/telemetry emission code and anything that joins traces back to source versions.
        - **Detectability**: Hybrid — static grep for trace-event shapes; semantic judgment on whether identities are conflated.

        ### P-054-1 [PROPOSED, syntax contested]: Keep compile-time directives and runtime metadata in strictly separate mechanisms
        - **Source**: BEP-54 (Compiler Attributes, status: proposed; syntax adoption contested in comments)
        - **Principle**: Compiler attributes (proposed `#[attr(...)]`) are directives to the compiler/linter/tooling that do not persist to runtime; BAML's existing `@`/`@@` annotations are runtime metadata on types/fields. "Maintaining a strict differentiation in which behaviors belong in which syntax will be critical to avoid confusion."
        - **Intended usage**: Tooling/compiler hints (hidden items, deprecation, lint allow/deny, must-use, codegen exclusion) go through the compiler-attribute mechanism; prompt-facing metadata stays with the runtime annotation system.
        - **Anti-patterns / slop signals**:
          - Magic comments as compiler directives in user code (the stdlib's `//baml:mut_vm` style is explicitly called a hack to be replaced).
          - Hardcoded name lists in compiler/tooling code standing in for per-item declarations.
          - Using runtime `@annotation(...)` syntax to trigger compile-time-only behavior, or vice versa.
          - Premature invention of a Rust-style attribute system where a narrow hint would do (the syntax is contested).
        - **Applicability trigger**: Any code adding compiler/tooling hints, magic comments, or metadata syntax to BAML sources or the compiler.
        - **Detectability**: Static-leaning hybrid — grep for magic-comment patterns (`//baml:`) and hardcoded path-match lists; semantic call on scope.

        ### P-026-1 [PENDING]: Two output channels — `to_string` for users, `to_debug` for developers — chosen by the author at the call site
        - **Source**: BEP-26 (print, status: pending)
        - **Principle**: Display and debug are distinct audiences a single channel cannot serve; every type has both methods (`to_string` defaults to delegating to `to_debug`), and the *author* picks the audience — `print`/`eprint` render via `to_debug`, interpolation and `+` render via `to_string`. The language never infers audience from type.
        - **Intended usage**: Write a hand-written `to_string` only when there's a genuine user-facing form; rely on auto-derived structural `to_debug` for inspection; force debug form explicitly with `${user.to_debug()}` when needed.
        - **Anti-patterns / slop signals**:
          - Hand-writing `to_debug` that enumerates fields (drifts when fields are added — a staleness lint exists for exactly this; auto-derivation cannot drift).
          - A single hand-rolled stringify method used for both user output and debugging.
          - Emitting user-facing text via `print` (developer channel) or building developer dumps via interpolated `to_string`.
          - Print-shaped `+` concatenation chains where variadic `print("label", obj)` is the idiom.
        - **Applicability trigger**: Any class defining string-conversion methods; any logging/printing code.
        - **Detectability**: Hybrid — static: hand-written `to_debug` bodies, `+`-chain prints; semantic: whether the audience choice matches context.

        ### P-026-2 [PENDING]: JSON is not a debug format
        - **Source**: BEP-26 (print, status: pending)
        - **Principle**: JSON discards class identity, type tags, and null fields and throws on cycles — exactly the information a debugger needs; auto-derived `to_debug` is structural, type-tagged (`User { name: "Ada" }`), and cycle-aware, so "there is no reason to reach for JSON to inspect a value."
        - **Intended usage**: `print(obj)` or `obj.to_debug()` for inspection and logs; JSON serialization is a separate concern reserved for wire formats.
        - **Anti-patterns / slop signals**:
          - Serializing values to JSON purely to log or debug them.
          - Building custom struct-dump helpers that re-derive what auto `to_debug` already produces.
          - Using debug output as a wire format.
        - **Applicability trigger**: Logging, error reporting, and debugging code paths that stringify structured values.
        - **Detectability**: Static-leaning hybrid — grep for JSON-encode calls feeding logs/prints; semantic call on whether the destination is a wire consumer or a human.

        ### P-026-3 [PENDING]: Interpolation holes are ordinary BAML expressions; formatting is methods, not a mini-DSL
        - **Source**: BEP-26 (print, status: pending)
        - **Principle**: `${expr}` accepts any BAML expression with no `!conversion` or `format_spec` grammar — method calls (`pi.to_fixed(2)`, `user.to_debug()`) are more composable and discoverable than a format mini-language, which is "the part of those systems users complain about."
        - **Intended usage**: `` `pi: ${pi.to_fixed(2)}` ``; `` `names: ${users.map((u) -> { u.name }).join(", ")}` ``.
        - **Anti-patterns / slop signals**:
          - Reimplementing format-spec behavior via ad-hoc helper mini-languages (e.g., a `format(value, "0.2f")` helper parsing spec strings).
          - Avoiding interpolation in favor of long `+` concatenation chains.
        - **Applicability trigger**: String-building code in backtick literals or format helpers.
        - **Detectability**: Static — grep for spec-string-parsing helpers and concat chains.

        ### P-026-4 [PENDING]: Prompt strings and interpolated strings are deliberately separate systems — don't build prompts by string assembly
        - **Source**: BEP-26 (print, status: pending)
        - **Principle**: Backtick `${...}` strings (eager, `to_string`-rendered) and prompt strings (Jinja/prompt pipeline, lazy, schema-aware) are syntactically and semantically distinct; defining `to_string` on a class does not and must not affect prompt rendering, because prompts are tied to the LLM contract (e.g. `{{ ctx.output_format }}`).
        - **Intended usage**: LLM prompts live in `prompt` blocks using the prompt pipeline, letting it render values schema-aware; general strings use backticks.
        - **Anti-patterns / slop signals**:
          - Constructing prompt text via backtick interpolation/concatenation and passing the finished string into an LLM function, bypassing the prompt pipeline.
          - Pre-stringifying structured values (`.to_string()`/`.to_debug()`/JSON) before injecting them into a prompt, instead of letting the pipeline render them.
          - Expecting `to_string` overrides to change how a class appears in prompts.
        - **Applicability trigger**: Any LLM function definition or code that assembles prompt content.
        - **Detectability**: Hybrid — static: string-typed "prompt" parameters built by concatenation; semantic: whether pipeline rendering was meaningfully bypassed.

        ### P-027-1: The signature is the CLI schema — never write argv-parsing boilerplate for a fixed flag list
        - **Source**: BEP-27 (Standalone Execution, status: implemented)
        - **Principle**: Any function with typed parameters gets its CLI derived automatically from its signature ("the signature already says what the parameters are, and the runtime can read it"); forcing manual argv parsing before a function can be invoked is friction with no payoff.
        - **Intended usage**:
          ```baml
          function main(input: string, model: string = "gpt-4o", verbose: bool = false) -> int { ... }
          // baml run -- --input "hi" --model claude-sonnet-4-6 --verbose=true
          // baml run --function llm.Summarize -- --text "hi" --max_words 30
          ```
          Parameterless `main()` + `baml.argv` is the explicit escape hatch *only* for shapes auto-CLI can't express (subcommands, positional routing, env fallbacks).
        - **Anti-patterns / slop signals**:
          - Parameterless `main()` hand-parsing `baml.argv` into a fixed set of named flags that a typed signature would express directly.
          - Wrapper `main` functions written solely to invoke another function (`--function` / `-e` already do this).
          - Structured payloads flattened into dozens of scalar flags instead of `--json-args`.
        - **Applicability trigger**: Any `main` function, script entry point, or CLI-shaped BAML program.
        - **Detectability**: Static — a parameterless `main` whose body is a straight sequence of flag lookups is visible in the AST; the escape-hatch legitimacy check (subcommands present?) is a small semantic step.

        ### P-027-2: Exit codes are explicit (`baml.sys.exit`), and return values go to stdout for composition
        - **Source**: BEP-27 (Standalone Execution, status: implemented)
        - **Principle**: Overloading `int` returns as exit codes forces functions to fight the harness and distorts data models, so exit code is fully decoupled: success exits 0, `baml.sys.exit(code)` sets anything else. The target's return value is serialized to stdout (`debug` for humans, `json` for pipelines) so BAML composes with `jq`-style tooling.
        - **Intended usage**: Return the actual result value and let output routing serialize it; call `baml.sys.exit(7)` when a specific shell code is needed; pipe with `--output-format json`.
        - **Anti-patterns / slop signals**:
          - Returning `int` from `main` purely to signal an exit status (it goes to stdout, not the shell).
          - `print`-ing the final result and returning nothing, breaking `--output-format json` pipelines and forcing downstream consumers to scrape logs.
          - Hand-serializing results to JSON inside the function when the harness's output format already does it.
        - **Applicability trigger**: Entry-point functions and any BAML program meant to be run or piped from a shell/CI/agent loop.
        - **Detectability**: Static — grep for `int`-returning mains with status-code-like literals, terminal `print` of results, and manual JSON dumps at exits.

        ### P-027-3: Configuration and target wiring fail at load time, not at runtime; file-path invocation is hermetic
        - **Source**: BEP-27 (Standalone Execution, status: implemented)
        - **Principle**: `baml.toml` `[scripts]` bodies, target resolution, and namespace collisions are all validated when the project loads, failing loudly with `file:line`; and `baml run <file.baml>` is the *only* context-free mode — so the same file runs identically on any machine.
        - **Intended usage**: Repeated invocations are named in `[scripts]`; scripts pre-bind defaults and users override after `--` with last-wins merging; scratch files stay standalone.
        - **Anti-patterns / slop signals**:
          - Wrapping `baml run` invocations in shell scripts/Makefiles to do what `[scripts]` entries express (losing load-time type-checking of flag names and targets).
          - Standalone-looking `.baml` files that secretly assume project context (they won't get it — hermetic mode never loads the project), or project entry points structured so they only work via file-path invocation.
        - **Applicability trigger**: `baml.toml` script tables, repo automation invoking BAML, and standalone `.baml` files.
        - **Detectability**: Static — inspect `baml.toml` and shell wrappers; hermeticity violations need a light semantic check.

        ### P-028-1: Caught errors stay plain values; the stack trace is an opt-in second binding, never global state
        - **Source**: BEP-28 (Stack Trace API, status: implemented; comments converged on `catch (e, trace)`)
        - **Principle**: BAML's `catch` is pattern-matched, not wrapper-typed: `e` is the raw thrown value, preserving the catch-as-match mental model, and the trace arrives via an optional second binding so programs that don't need it pay zero cost. Global/thread-local error context (the "`errno` pattern") is explicitly called out as an antipattern to avoid.
        - **Intended usage**:
          ```
          f() catch (e, trace) { AppError => { print(e); print(trace) } }
          f() catch (e) { AppError => { ... } }       // no trace binding, no capture overhead
          ```
        - **Anti-patterns / slop signals**:
          - Wrapper error classes hand-built to carry stack traces alongside payloads (reimplementing the rejected wrapper-type option in userland).
          - Any "last error"/global error-context mechanism read after the fact (clobbered by intervening catches).
          - Binding `(e, trace)` everywhere and never using `trace` (paying capture cost for nothing — the opt-in exists to avoid this).
          - Side tables mapping error values to metadata by hash (collides on rethrow/nested catches of equal values).
        - **Applicability trigger**: Every `catch` block, and any error-type or error-utility definitions.
        - **Detectability**: Static-leaning hybrid — unused `trace` bindings and wrapper-error classes are grep/AST-visible; whether a wrapper class is trace-carrying slop vs. legitimate domain error needs LLM judgment.

        ### P-028-2: Match error types explicitly; panics are not silently swallowed by wildcards
        - **Source**: BEP-28 (Stack Trace API, status: implemented — documenting the implemented catch semantics)
        - **Principle**: The compiler infers all throwable types from the callee and catch is non-exhaustive by default (unhandled errors rethrow automatically); panics (`DivisionByZero`, `IndexOutOfBounds`, …) are deliberately excluded from `_ =>` so that catching them is always a named, explicit act.
        - **Intended usage**: Write arms for the specific error types you can handle and let everything else rethrow; reach for `catch_all` / `catch_all_panics` only when exhaustive handling is genuinely intended.
        - **Anti-patterns / slop signals**:
          - Reflexive `catch_all` / `catch_all_panics` (or `_ =>` swallow-arms) around every call — fighting the rethrow-by-default design.
          - Encoding error kinds as strings and string-matching in a single arm instead of using the inferred typed arms.
        - **Applicability trigger**: Any `catch` expression in the codebase.
        - **Detectability**: Static for the mechanics (grep `catch_all`, bare `_ =>` with empty bodies); semantic for whether exhaustive catching is justified at that site.

        *(BEP-35, "A git interface to BEPs", status: superseded — meta-process proposal; no code-quality principles.)*
    "##
}
