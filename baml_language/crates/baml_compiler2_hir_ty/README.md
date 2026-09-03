# baml_compiler2_hir_ty: rust-analyzer-style type inference

Status: shipped through S13 + S10 + S12 + I1 + I2 + I3 (S0 harness, S1 body-owner ID, S4a interned
repr, S4 declaration lowering, S4b oracle entry, S5 table, S6 core exprs,
S7 bidirectional checking, S8 calls/constructors/fields, S9 lambdas +
`resolve_value_path` consolidation + function values, S11 method calls:
`method_resolution` receiver->class table, receiver-pinned class
generics, `class_self_ty` through the builtin bridge, type-qualified
method paths; S13 finalize: minimum-upper meets, all-equal lowers,
local Infer-to-Error erasure - no `Infer` reaches a result - and
post-substitution union re-canonicalization with null-last
presentation), plus operator dispatch through the `baml.ops` interfaces
(decision 4; bitwise on the hack table until the stdlib grows its
interfaces). 33 of 34 spec fixtures green, including the
spec-ahead-of-TIR wins: canonicalized bool-join, expression-position `_`
holes, equality-regime generic resolution + coherent disagreement
verdicts [B-932], `??` informing its right operand [B-1135, both
positions], builtin `baml.Array`/`baml.Map` bridged structural [B-1080],
reduce-seed widening through method generics [B-1134/B-742/B-267.1],
push-arg empty-literal adoption [B-940], order-independent empty
literals in generic args [B-1085], unconstrained holes erased to
LOCAL errors [B-236 unchecked half]. S10a (patterns): the rustc
usefulness port lifted from TIR verbatim (tests included), the lowering
walk re-authored per TIR's per-shape table, validated against the
92-function legacy corpus (pattern_corpus.rs verdict tables); generic
destructures adopt args from the scrutinee [B-919], union-member
claiming requires provable overlap so a rigid arm never covers `null`
[B-633 live half], non-exhaustive matches type as Error. S10b (flow
narrowing, the settled eager-forward design - no CFG, BAML control flow
is structured): `flow` overlay (`BindingId -> Ty`, cloned at branches,
capture-guarded), `CondFacts` with walk-time De Morgan [B-688],
divergence-aware branch merge subsuming early-return narrowing, loop
havoc + entry-join [B-735], assignments checked against DECLARED with
narrow-on-assign [B-618], match residual accumulation and else-side
subtraction gated on `consumes_matched` [B-774, B-1069]. S12 (throws):
effect inference IS type inference on the error channel - omitted
clauses infer from throw sites + callee effects via the crate's first
salsa query (`callable_throws`, `cycle_initial = never` fixpoint over
mutual recursion), effect VARIABLES in the table default unconstrained
to `never` (BAML's only defaulting rule; value vars stay ruling-2
errors), lambdas own their channel, `catch` subtracts handled arms on
the error channel and propagates the residual, declared clauses are
the contract every contribution checks against - including rigid-var
clauses (B-1082's rule: defer through bounds, never skip). The
`throws !error` sentinel is gone from every signature render. I1 (impl registry):
`impls.rs` re-authored interned-native from TIR's impl_rules - impls
normalize to the free shape (in-body = `implement<T> I for C<T>`),
one-directional pattern matching with the fact-poor `AliasOnlyFacts`
equality (the termination argument), blanket bounds re-enter with the
depth-16 budget, realizedness is a gated contract not an assertion,
and bounds are conjunctive end to end (TIR's single-bound asymmetry
not replicated). `Facts::implements_interface` and
`interface_requires` answer for real - `C <: I`, blanket coverage,
requires-widening, and canonical union absorption (`C | I == I`) all
light up; `ops.rs` is now a thin facade over the registry, so
user-defined and in-class operator impls dispatch like the stdlib's.
I2 (param env): declared
bounds enter the frame (`function_generic_bounds` mirrors the frame's
ParamTy identities: class prefix, interface Self-bound + params +
assoc slots, own params), `Facts` carries them and `type_var_bound`
answers - a rigid `T extends I` proves `T <: I` (join-observable),
operators dispatch through CARRIED bounds yielding the Output pin or
the symbolic projection, and `T.Output` projections LOWER to real
nodes (interface determined from the unique declaring bound; reduction
stays I5). The spec's bounded-add example is green verbatim. I3 (interface members):
`lookup_interface_member` resolves fields and methods on existential
and rigid-bounded receivers - root-wins tiering over the fuel-bounded
`requires` closure, `Self` instantiated per receiver kind, associated
slots take the reference's pins or the symbolic projection, and the
one-Self rule gates existential dispatch (STRICTER than TIR, which
permits `a.eq(b)` on two existentials at type level - tir: fails).
The I2 frame work made `Self` a real path resolution (the S4-era
always-Error guard now defers to the frame). In-body impl methods
reach concrete receivers via the class method list; out-of-body impl
members join with the symbolic resolvers (I4 remainder). I4
(obligations): rust-analyzer's fulfillment semantics over BAML's facts -
`Implements`/`Operator` obligations REGISTER during the walk when
information is missing (never guess), discharge at finish INTERLEAVED
with bound resolution to fixpoint (stall-on-ambiguity, retried; the
ground-subset rule for a class's bounds breaks the `?A`/`?O` operator
deadlock), still-stalled obligations fail CLOSED through finalize.
Call-site bounds check on generic functions/methods; `a + b` on an
unsolved generic chains obligations instead of the reduce-interior
sentinel. I5 (associated types): the `project`/`associated_type_bound`
facts are live - reduction in rustc's `project.rs` candidate order
(param-env first: qualifier pin, then carried bounds ELABORATED through
the requires closure, disagreement = ambiguity = Opaque; then the
base's own reference; then impl candidates via the registry with
binding-else-default, rustc's `leaf_def`), defaults lowered ONCE per
`(interface, assoc)` with symbolic `Self` in the interface frame and
realized by the shared positional instantiation, declared assoc bounds
(`type Item extends J`) instantiated as rustc's `explicit_item_bounds`
for still-rigid projections, and the pin gate in impl matching is exact
(default-filled, fail-closed). One documented spec DIVERGENCE from
rustc: a written interface reference fills omitted defaulted members
(spec: "associated types with defaults may be omitted and will use
said defaults"), so a bare `T extends Iterator` pins `Error = never`
through the default - exactly what makes the spec's `first` example
sound (`throws never` verifies; the fixture is green verbatim, TIR
fails it). Dotted projection paths (`T.Item`, `Self.Item`, chained
`T.Item.Sub` through the previous member's bound) lower via the frame;
`interface_scope_bounds` is the one interface param env every
interface-scoped lowering shares. Partial throws clauses (`throws T |
_`, spec Functions rule 3) ride along: the open slot suspends the
contract check and the surface callers see is declared-union-inferred.
I6 (`Self` + inheritance): interface default-method bodies
type as ordinary inference roots with NO new machinery - the I2 frame
(`Self` at slot 0), the I3 bound-driven member lookup, and the I5
projections compose (`self.get()` inside a default body types
`(Self as I).Item`, the spec's universal reading). What I6 adds:
concrete receivers resolve INHERITED members through the impls they
match - `impls_for_type` enumeration + the rust-analyzer trait-impl
candidate tier in the member ladder (class-inherent first, then impl
providers with root-wins across `requires`, ambiguity fails closed;
candidates whose implemented head still carries undetermined impl
params are skipped fail-safe pending probe machinery). `Self` in
`requires` targets realizes for real: targets lower in the full
interface frame and instantiate against the elaboration SUBJECT
(rustc's super-predicate instantiation; a `requires Source<Item =
Self.Item>` pin becomes `(T as Sink).Item` on a rigid `T`), with the
subject threaded through the whole closure - `direct_requires_closure`
and `interface_requires` take it explicitly; the fact boundary with no
better subject uses the existential itself (rustc's `dyn A: B` shape).
And finalize gained post-substitution projection NORMALIZATION
(rustc's instantiate-then-normalize): every oracle-determinable
projection reduces at the result boundary, so `s.pair()` on a concrete
receiver renders `int[]`, not `(IntStore as Store).Item[]` - targeted,
not full canonicalization, which would expand nominal aliases renders
keep. TIR divergences pinned: `Self` left unrealized through TIR's
requires closure (errors a sound program), and the unreduced-default
throws-contract error on stdlib `collect`. I7 (coherence): `coherence.rs` lifts TIR's overlap
engine + walk (reference, never a dependency) - symmetric first-order
equality unification with both impls' params renamed to disjoint
unification variables (rustc's overlap check with fresh inference
vars), three-valued `Overlap` with Kleene conjunction, ACI union
covering via a budgeted MRV+backtracking search (the NP-hard ceiling
degrades to `Unknown`, never a guess), occurs check, depth backstop
for distinct recursive aliases, bound refutation at PRINCIPAL ground
witnesses through the I1 registry, the E0138 subject gate on
alias-expanded heads, and per-package + dependency-closure checking
whose completeness rests on the orphan rule (knowability). The orphan
rule itself (E0139, RFC-2451 covered) ships alongside. Both surface as
LOCATION-KEYED tracked queries (`package_coherence_violations` /
`package_orphan_violations`) - span-free, unlike TIR's span-carrying
query, so whitespace edits do not invalidate; S17 renders. The lifted
engine keeps TIR's full unit corpus (3-SAT reduction, pigeonhole,
collapse/covering cases) plus new orphan tests; a differential suite
in `baml_tests` runs BOTH engines' coherence over identical sources
and they agree pair-for-pair (spans compared by containment - TIR
anchors in-body violations on the interface-target name; ours is the
block). Marked carry-over: `cover`'s registry-blind conservatism for
concrete-vs-interface membership (refining it through the live
registry changes accepted programs - its own deliberate step).
75 fixtures green, 1 pending pin: B-1075 pairwise bitwise (stdlib
interfaces, outside this branch).

`baml_language/TYPE_SYSTEM.md` is the correctness authority. It is
prescriptive: where the current TIR implementation disagrees with it, the spec
wins, and the disagreement is encoded as an `#[ignore]`d test in the spec
corpus (see Testing below). The reference architecture is rust-analyzer's
`hir-ty` crate (post next-solver migration); design decisions below cite it.

## Principles

1. Layering mirrors rust-analyzer (`hir-def -> hir-ty -> hir`). This crate
   depends on ast/hir/ppir/baml_type and never on
   tir/mir/emit.
2. Leaf crate until cutover (S16). Until then its only consumer is the test
   harness in `baml_tests`, and every existing compiler snapshot stays
   byte-identical. A slice that changes a `03_ppir`/`04_tir`/`04_5_mir`/
   codegen snapshot has leaked.
3. Slice-local correctness. Each slice's tests assert only what that slice
   implements. Constructs the engine does not handle yet infer to an error
   sentinel with no diagnostics claimed, so fixtures always run end to end.
   No slice reaches forward (e.g. calls stub obligations as always-ambiguous
   until the obligation slice lands; rust-analyzer's probe machinery
   tolerates exactly this).
4. Build on the existing type algebra. `baml_type::normalize` (subset
   subtyping, union canonicalization, mu-binders, the fail-safe `TypeContext`
   fact trait) is kept and consumed, not rebuilt. (CORRECTION 2026-08-08:
   an earlier revision claimed `baml_type_runtime::InferenceConstraints`
   would be consumed too - it was not; the solver was built fresh in
   `infer/unify.rs` + `sub()`, and this crate has no baml_type_runtime
   dependency.) This crate's impl registry and param env became the
   `TypeContext` impl, which is how interface facts flow into every
   subtype check.

## What BAML needs that Rust does not (and vice versa)

Deltas that shape the design, from TYPE_SYSTEM.md:

- Set-theoretic subset subtyping (`never <: T <: unknown`, literals below
  their concrete types, union-member covariance). Joins at control-flow merge
  points form canonicalized unions instead of Rust's LUB coercion.
- ACI union algebra: `1 | int == int`, `true | false == bool`. Equality is
  semantic, not syntactic.
- Literal freshness and widening at binding sites; no int/float defaulting
  (`int` and `float` are unrelated concrete types).
- Invariant generics; contravariant function params, covariant return and
  throws.
- Throws-effect inference (omitted `throws`, partial `throws T | _`,
  synthetic `__effect_param_N` generics).
- Flow-sensitive narrowing (`is`, `if let`, null checks, guard clauses).
- Not ported from rust-analyzer: autoderef/autoref, match ergonomics and
  binding modes, the mutability pass, capture-kind escalation, int/float
  fallback, the diverging-fallback graph. All serve references/borrows or
  numeric defaulting, which BAML does not have.

## Slices

Phase 0: foundations. Phase 1: engine, inside the leaf crate. Phase 2:
cutover. "Tested by" is the merge gate for the slice.

| #   | Slice                                                                | Tested by                                              |
| --- | -------------------------------------------------------------------- | ------------------------------------------------------ |
| S0  | Harness + engine stub: `//^ ty` checks, infer-dump snapshots, corpus | corpus red in `fixtures/pending/`, asserted to fail    |
| S1  | HIR: unified body-owner ID + `body(owner)` queries                   | unit tests; downstream snapshots byte-identical        |
| S2  | DONE - inference fully span-free (lambda-scope map replaced the last span join) | corpus green under the structural join         |
| S3  | DONE - `infer_function_body`/`infer_let_body`/`function_signature` tracked (PartialEq cutoff; per-loc dispatcher, cycle seeds) | IncrementalTestDb: signature firewall scenarios |
| S4a | Interned recursive `Ty` (`baml_type::interned`): global hash-cons pool, `TyKind` with handle children, O(1) `TypeFlags`, plain conversions | round-trip / sharing / flags / ordering / eviction unit tests |
| S4  | Decl lowering: `TypeRef -> Ty`, generic frames, item-sig queries     | lowering unit tests + differential vs TIR lowering     |
| S4b | Normalize port: `NormalTy::from_interned` entry (or fork) so subtyping is native to the interned repr | ported normalize test suite; verdict parity vs plain entry |
| S5  | InferenceTable: vars, unify, occurs, snapshot/rollback; ctx skeleton | table unit tests; harness runs end to end              |
| S6  | Core exprs: literals+freshness, let/locals, blocks, `_` holes        | simple-tier fixtures; wildcard-hole spec tests         |
| S7  | Expectation + canonicalizing union-join + Diverges/never             | coercion/never-tier fixtures                           |
| S8  | Calls: fresh vars per site, variance-aware solve, 2-pass args        | call fixtures; invariant-position rejection tests      |
| S9  | Lambdas: expectation-driven params, child scope; function VALUES outside call position; consolidate callee/path resolution into one `resolve_value_path` entry (r-a's `infer/path.rs` shape) | lambda fixtures; TIR differential                      |
| S10 | SHIPPED (S10a patterns + exhaustiveness, S10b flow narrowing): lifted `exhaustiveness.rs`, `infer/pat.rs` walk, `infer/flow.rs` CondFacts/merge/loop/assign machinery; B-919/B-633/B-688/B-774/B-735/B-618/B-1069 all fixed with tir: fails fixtures | patterns + narrowing fixtures; pattern_corpus verdict tables |
| S11 | Member resolution (CORE SHIPPED, before S10/S2/S3): receiver->owning-class table incl. builtin classes (alias-transparent), receiver-pinned class generics + turbofish/fresh method generics, self via `class_self_ty`, bound vs UFCS/static calls, methods as values. Remaining: none (probe/confirm and `?.` shipped) | method fixtures; B-1136 sub-issue fixtures (obligations stubbed) |
| I1  | SHIPPED: `impls.rs` registry (free-shape normalization, blanket matching + bound re-entry, fact-poor match equality, conjunctive bounds); facts wired (implements_interface, interface_requires); ops.rs = facade. Orphan check E0139 joins S17 diagnostics | existential/blanket/requires/user-operator fixtures, join-absorption observable |
| I2  | SHIPPED: param env - frame-keyed bound conjunctions into LowerCtx/Facts, type_var_bound live, carried-bound operator dispatch, projection lowering. Concreteness gating (`is_bounded_arg_admissible`) + call-site bound verification join I4's obligations | bounded-var join absorption, spec's bounded-add verbatim |
| I3  | SHIPPED: interface fields + methods on existential/rigid receivers (root-wins over requires closure, per-receiver Self, one-Self gate); in-body impl methods via class list. Out-of-body impl members + unions -> I4 symbolic resolvers | existential/bounded method + field fixtures; one-Self rejection pin |
| I4  | SHIPPED: the obligation worklist (Implements + Operator kinds; register-and-fulfill, stall-on-ambiguity, fixpoint interleave with ground-subset bound resolution, fail-closed). Probe = table snapshot/rollback when candidate selection needs it. Out-of-body impl members + union receivers + symbolic resolvers remain here | operator-deferral fixtures (single + chained); reduce interior sentinel gone |
| I5  | DONE - projection reduction (rustc candidate order), assoc defaults + declared bounds, exact pin gate, partial throws | spec `first` example verbatim; `partial_throws_clause` |
| I6  | DONE - default bodies as roots (frames composed), concrete-receiver impl tier, `Self`-in-requires realization, finalize projection normalization | `interface_default_method_body`, `requires_self_pins_realize`, `stdlib_iterator_collect` |
| S12 | SHIPPED: callable_throws salsa fixpoint; effect vars default never; lambda channels; catch residual on the error channel; declared-clause contracts incl. rigid vars (B-1082). `throws T \| _` partials join with obligations (I4) | throws + catch fixtures |
| S13 | SHIPPED: finalize - resolve-all to fixpoint (minimum-upper meets, all-equal-lowers agreement), local Infer-to-Error erasure (rulings 2/3; r-a's replace-with-error discipline), post-substitution union re-canonicalization (null-last at this crate's boundary). Diagnostics land with S17 | `[]`-inference fixtures; no-infer-leak invariant |
| I7  | DONE - overlap engine + walk lifted (location-keyed queries), orphan rule; unit corpus carried + differential suite vs TIR | `coherence.rs` tests; `type_spec/coherence.rs` differential |
| S15 | Parity: stdlib corpus (`__baml_std__`) + full differential sweep     | every fixture diffed; divergence list = spec fixes only|
| S16 | Cutover: `ScopeInference` facade, dep inversion, delete TIR paths    | full CI matrix; snapshot diffs reviewed per feature    |
| S17 | Diagnostics: split `Unknown`/`Error`/`Unknown`; mismatch map  | diagnostic_errors-tier snapshots                       |

Ordering notes:

- S7's cluster (Expectation, union-join accumulator, Diverges, never
  propagation, expression read-ness) is entangled by design and lands as one
  unit; rust-analyzer's experience is that retrofitting any member of that
  cluster means touching every infer call site.
- I1 slots after S8: existential assignment and bound checks first appear at
  check sites. I7 needs nothing from the inference engine and can land any
  time after I4; it is sequenced late because it is pure migration.
- S12 sits after S8 (effect polymorphism rides generic instantiation) and
  S10 (catch residuals reuse pattern set-subtraction), before S13 (the
  `never` default for unconstrained effect vars is a finalization rule).

## Throws inference design (S12)

Throws is a slot in `Ty::Function`, so effect inference is type inference
pointed at the error channel:

- Omitted `throws` and the `_` in `throws T | _` lower to fresh effect vars.
- `throw e` contributes a lower bound; calls contribute instantiated callee
  throws; bounds merge through the same canonicalizing union-join as match
  arms. `throw` itself types as `never`.
- `catch` is narrowing on the error channel: residual = incoming minus arm
  coverage, via the pattern set-subtraction machinery.
- Synthetic `__effect_param_N` generics are ordinary type parameters to the
  call slice; covariant solving handles propagation.
- Cross-function propagation is on-demand salsa (`callable_throws(owner)`),
  with `cycle_initial = never` iterating mutual recursion to fixpoint,
  replacing the eager package-wide pre-pass.
- Finalization: an effect var with no bounds resolves to `never` (BAML's only
  defaulting rule). Interface method signatures must declare `throws`, so no
  effect vars cross the virtual-dispatch boundary.

## Decisions

1. SETTLED - Ty representation: recursive hash-consed interning, implemented
   in `baml_type::interned` (S4a). One-word handles, children are handles
   (shallow pool operations, automatic substructure sharing), `TypeFlags`
   cached at intern time, structural `Ord` with a pointer fast path,
   entries evicted on last drop. The pool is global, NOT salsa: types must
   outlive any database (runtime, serialization, FFI) - the same reason
   rust-analyzer interns its types outside salsa. It mirrors the plain
   master enum (exhaustive conversions make drift a compile error), with
   spec-driven deltas: `Infer` gains an optional `InferVar`; TIR's internal
   recovery sentinel is deliberately unrepresentable - the plain enum's
   `Unknown` (the new engine has exactly one error sentinel, `Error`,
   rust-analyzer style; TIR's other two uses of `Unknown` become
   `Expectation::None` and fresh
   infer vars); the top type is `Unknown` in both. The plain enum and
   all downstream consumers are untouched; TIR is never migrated (it is
   deleted at cutover); MIR/emit/runtime and the family axes migrate to
   this representation at cutover. Until S4b lands, subtype checks
   materialize via `to_plain` (cheap at BAML type sizes).
2. SETTLED - Constraint system, per the 2026-07-10 unified-inference
   investigation (doc-inference.md; its section 7 rulings are adopted
   verbatim). Eager `Eq` unification (occurs-checked union-find) plus `Sub`
   constraints that DECOMPOSE BY HEAD: invariant constructors decay
   `Sub` to `Eq` of arguments (killing most subtyping depth), var-headed
   cases record lower/upper bounds in per-root
   `VarData { lowers, uppers, known, obligations }`, and ground cases check
   via canonical `normalize::is_subtype`. Obligations
   (`Implements`/`Projects`/`Concrete`) sit on a worklist retried on each
   resolution event; ONE step budget threads through the whole solve
   (fail closed, "annotate here" - the coherence discipline). Joins happen
   only at syntactic join SITES (container literals, branch arms, throws
   accumulation), arriving at vars as single pre-joined bounds; var
   RESOLUTION is Rust-parity equality - all lower bounds must be equal
   after fresh-literal widening (`pair(1, 2)` gives `T = int`;
   `pair(1, "a")` errors), else `meet(uppers)`, else defaulting, else
   error. Defaulting rounds: (1) fresh-literal widening, (2) throws vars
   with no lowers become `never`, (3) nothing else is silent - an
   unconstrained var is a hard error recorded as `Error` (never the top
   type). Two reversibility knobs stay centralized: `resolve_var` (the
   join-vs-equality policy) and `finalize_var` (the unresolved-var
   policy). Inspection sites (member access, calls, scrutinees,
   narrowing) force resolution rustc-`structurally_resolve` style - this
   is what replaces the Evolving* mutation interception, and it works
   through aliases and fields because identity lives in the union-find.
   Section 7 rulings adopted: no join for generic params; unconstrained
   `let a = []` is an error; unresolved type args are hard errors; `_`
   expands to every expression-position type slot (declaration signatures
   stay excluded). NOTE: rulings 2 and 3 assert diagnostics, which the
   fixture harness cannot express yet - an expected-diagnostic fixture
   class arrives with the S17 harness extension.
3. SETTLED - Query shape: `infer_body(owner)` keyed by the S1 body-owner
   ID (`Function(FunctionLoc) | Let(LetLoc)`, rust-analyzer's
   `DefWithBodyId` shape; lambdas stay inside their owner's body and
   table per #4282), with the lambda-projection pattern preserved and
   cycle-recovery parity with `infer_scope_types`. Like rust-analyzer's
   extra roots (`for_signature` for signature-embedded const exprs), a
   SECOND inference root joins later for parameter default expressions
   (their own arena; TIR's `DefaultParameterInference`) - same result
   shape, different entry point; the body-owner enum is not widened for
   it.

4. SETTLED (ruling, 2026-07-31) - Operators go through interfaces. A
   dispatching operator IS its interface dispatch: `a + b` types as
   `Implements(lhs, baml.ops.Add<rhs>)` with the impl's `Output` as the
   result (`ops.rs` registry, the operator-shaped seed of I1's full impl
   registry); there is NO builtin-operator table in the type system, and
   rewriting primitive cases to single instructions is MIR's job at
   lowering, invisible to inference. The non-dispatching operators are
   type algebra, deliberately: `&&`/`||` are short-circuit control flow
   (not overloadable, as in Rust), `==`/`!=` are structural equality over
   `Concrete` (comparison.baml's design), `??` is null-algebra
   (remove-null + canonical-unwrap/join), and `!` is bool. Ordered
   comparisons are Compare-gated (the obligation lands with I4). ONE
   exception, marked HACK in `infer.rs::bitwise_hack_table`: the five
   bitwise operators use a hardcoded table mirroring TIR's
   `infer_bitwise`, because the stdlib has no `baml.ops` bitwise
   interfaces yet - when `ns_ops` grows them (and TIR switches off its
   own table), the hack table is deleted and they route through
   `dispatch_operator` like everything else.

## Testing

- `baml_tests::type_spec` is the harness (this repo's analog of
  rust-analyzer's `check_types` + `check_infer` in
  `crates/hir-ty/src/tests.rs`), and it runs against THIS crate's engine --
  never TIR. `//^^^ ty` caret annotations are checked against exact source
  ranges, bidirectionally (a mismatch fails; fixtures with zero annotations
  fail).
- A test is a `.baml` file, not a Rust function: the runner in
  `type_spec/fixtures.rs` picks up every file under `type_spec/fixtures/`
  (must pass) and `type_spec/fixtures/pending/` (must FAIL; starts with a
  `// pending: <slice> <reason>` directive). The corpus starts fully red in
  `pending/`; when a slice turns a fixture green the runner prompts its
  promotion into `fixtures/`. This is the acceptance mechanism for every
  engine slice.
- Every fixture also gets an insta snapshot of the `check_infer`-style dump
  (`start..end 'text': ty` per inferred node, from
  `harness::render_infer`). Empty dump = engine infers nothing there yet;
  coverage growth is visible in snapshot review as slices land.
- TIR appears only in the S15 differential sweep, run separately over the
  same fixture corpus; the `04_tir` project tier remains the dump-style
  regression net for the OLD engine until cutover deletes it.
- Differential mode: while both engines exist, fixtures run against both and
  diff; intentional divergences must match the spec-divergence list.
- The stdlib (`crates/baml_builtins2/baml_std`) is the scale gate (S15):
  it exercises generics, interfaces, associated types, and effects at scale.
- Incrementality claims are proven in `baml_tests::incremental` scenarios
  (exact salsa WillExecute counts), not asserted in prose.

## S15.5 - Principledness audit (2026-08-08)

Three-reviewer sweep + verification pass over the crate; every remaining
conflict in the S15 ledger was already ruled, so this list is internal
quality, not differential state. STATUS 2026-08-08: ALL TIERS CLOSED
(A bugs fixed pin-first; B inconsistencies fixed or evidence-reverted;
C consolidations landed through 49ecc19d3). Remaining S16 rides: the
shared algebra's Error compat-vs-identity split; the requires-cycle
diagnostic (S17).

### A - verified bugs (pinned in fixtures/pending/)
- A1 `remove_null` (infer.rs) matches Null/Union on `resolve_completely`
  alone: aliased nullables break every `?.` link, `??`, and null flow
  facts. Fix: `structurally_resolve` at entry; peel-after-resolve at the
  chain links.
- A2 lambda expectation deduction uses `shallow_resolve`: an alias-typed
  function annotation gives params `!error`. Fix: structurally_resolve
  the expectation.
- A3 `dispatch_operator`/`operand_members` never expand aliases; also
  await (false mismatch on aliased Future), spawn body/baml.spawn.Params
  (silent wrong future value), obligation subjects (alias -> permanent
  stall), `sub()` decomposition arms (alias skips invariant arms),
  upcast targets, `expectation_shape` (bounded vars don't adopt).
- A4 scrutinee forcing: `infer_match` forces occurring vars; `if let`,
  `while let`, `is`, let-destructure, and `Is`-facts do not (latent -
  probed, no observable divergence yet; fix for consistency).
- A5 (downgraded to B after probing): plain-union operands dispatch
  fine; the poison-to-top in `dispatch_operator`/`field_access` union
  arms stays theoretical. Alias-typed obligation subjects stall
  UNOBSERVABLY today (bounds silently unchecked - surfaces at S17).

### B - inconsistencies (one pass over the union/freshness layer)
- `union_of` syntactic fallback does not collapse singletons
  (`Union([?0])` can never unify - unify's union arm is positional).
- Freshness: `remove_null`/`?.`-boundary use `union_of` (erases
  freshness) where the control-flow rule wants `join`; `join`'s remark
  upgrades RIGID literals to fresh (contradicts the non-widening-witness
  policy); `subtract_narrow` strips overlay freshness.
- `register_call_bounds` fires on 4 of 10 instantiation sites; 3 sites
  bypass `fresh_generic_arg` (benign today - class/impl frames carry no
  effect params - but a footgun).
- And/Or is the only fold arm that skips `resolve_completely`.
- `finalize_ty` skips union re-canonicalization whenever the type
  carries any error (poison-to-top of the canonicalization pass);
  `finish()`'s throws bypass finalize entirely on the declared arm.
- `constant_equality` fold hardcodes Fresh (TIR parity - document).

### C - centralization debt
- Interface-representation triangle: ~30 conversion sites between
  InterfaceRef/InterfaceTarget/plain Interface (byte-identical `as_ref`
  closure twice in lower.rs; bindings-substitution block x5). One
  From/TryFrom set + `InterfaceTarget::{as_ref, existential, realized}`.
- Callee ladder has two spellings (Path vs MemberAccess) sharing three
  drifted roads; "real static outranks from_json" enforced by two
  orderings in two functions.
- Requires-closure head-match: 5 sites, 4 equivalence relations; every
  caller re-prepends the root the helper excludes. (The name-only root
  filter is NOT a bug: `requires` cycles are rejected BY NAME upstream
  - TIR: "interface requires cycle: RsnFeed -> RsnFeed" - so same-name
  different-args requires are illegal programs. hir_ty accepts them
  today for lack of the diagnostic: an S17 item.)
- Dead: `exhaustiveness::check_irrefutable` (zero callers). Redundant:
  `member_callee`'s expand_alias_ty after structurally_resolve. Dupes:
  function-local INT_MIN/INT_MAX x2; `format_float` re-formats.
