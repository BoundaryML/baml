# baml_compiler2_hir_ty: rust-analyzer-style type inference

Status: slice S0 (harness) in progress. No inference engine exists yet.

`baml_language/TYPE_SYSTEM.md` is the correctness authority. It is
prescriptive: where the current TIR implementation disagrees with it, the spec
wins, and the disagreement is encoded as an `#[ignore]`d test in the spec
corpus (see Testing below). The reference architecture is rust-analyzer's
`hir-ty` crate (post next-solver migration); design decisions below cite it.

## Principles

1. Layering mirrors rust-analyzer (`hir-def -> hir-ty -> hir`). This crate
   depends on ast/hir/ppir/baml_type/baml_type_runtime and never on
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
   fact trait) and `baml_type_runtime::InferenceConstraints` (variance-aware
   solving) are kept and consumed, not rebuilt. This crate's impl registry and
   param env eventually become the `TypeContext` impl, which is how interface
   facts flow into every subtype check.

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
| S2  | HIR: span-free body `TypeRefStore` + source map                      | parity with spanned annotations; snapshots identical   |
| S3  | HIR: per-body semantic-index projections (PartialEq cutoff)          | IncrementalTestDb: comment edit re-runs nothing        |
| S4  | Decl lowering: `TypeRef -> Ty`, generic frames, item-sig queries     | lowering unit tests + differential vs TIR lowering     |
| S5  | InferenceTable: vars, unify, occurs, snapshot/rollback; ctx skeleton | table unit tests; harness runs end to end              |
| S6  | Core exprs: literals+freshness, let/locals, blocks, `_` holes        | simple-tier fixtures; wildcard-hole spec tests         |
| S7  | Expectation + canonicalizing union-join + Diverges/never             | coercion/never-tier fixtures                           |
| S8  | Calls: fresh vars per site, variance-aware solve, 2-pass args        | call fixtures; invariant-position rejection tests      |
| S9  | Lambdas: expectation-driven params, child scope                      | lambda fixtures; TIR differential                      |
| S10 | Patterns + narrowing; exhaustiveness reused via `PatCtx`             | patterns-tier fixtures                                 |
| S11 | Member resolution: probe/confirm; fields, methods, `?.`              | method-resolution-tier fixtures (obligations stubbed)  |
| I1  | Impl registry + nominal lookup; orphan check (E0139)                 | `C <: I` fixtures; orphan diagnostic parity vs TIR     |
| I2  | ParamEnv: `T extends I` bounds; concreteness rules                   | generic-fn fixtures calling bound methods on `T`       |
| I3  | Interface members: virtual vs concrete pick, one-`Self` rule, fields | existential + bounded-param method fixtures            |
| I4  | Generic/blanket impls; obligation queue (probe vs register)          | blanket-impl fixtures; replaces S11 stub               |
| I5  | Associated types: projections, bindings, defaults, fuel              | traits-tier fixtures; stdlib `Iterator` shapes         |
| I6  | `Self` + interface default-method bodies as inference roots          | default-body fixtures                                  |
| S12 | Throws: effect vars in the table, `throws T \| _`, effect params     | throws fixtures; TIR differential                      |
| S13 | Finalize: resolve_all, widening, element vars replace `Evolving*`    | `[]`-inference fixtures; no-infer-leak invariant       |
| I7  | Coherence overlap moves in (existing ACI engine, unchanged)          | differential vs TIR on coherence diagnostic fixtures   |
| S15 | Parity: stdlib corpus (`__baml_std__`) + full differential sweep     | every fixture diffed; divergence list = spec fixes only|
| S16 | Cutover: `ScopeInference` facade, dep inversion, delete TIR paths    | full CI matrix; snapshot diffs reviewed per feature    |
| S17 | Diagnostics: split `Unknown`/`Error`/`BuiltinUnknown`; mismatch map  | diagnostic_errors-tier snapshots                       |

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

## Open decisions (settle before S5)

1. Ty interning. rust-analyzer uses word-sized interned types with a cached
   flags bitmask (`has_infer()` in O(1)) and a hot/stored split for salsa
   results, and reports that retrofitting is painful. BAML's `Ty` is a plain
   deep-cloned enum produced by the `ty_family!` transmute machinery.
   Options: (a) crate-local interned inference type converting to
   `baml_type::Ty` at boundaries; (b) plain `Ty` with the existing `Infer`
   variant, accept later retrofit cost.
2. Subtyping in the table. (a) rustc-style eq-unification with subsumption
   checks at check sites and recorded sub-obligations for deferred cases;
   (b) bounds-propagating vars (biunification-style lower/upper bound sets),
   more natural for subset subtyping. Shapes unify, Expectation, and the S7
   join machinery.
3. Query shape. `infer_body(owner)` keyed by the S1 body-owner ID, with the
   lambda-projection pattern preserved and cycle-recovery parity with
   `infer_scope_types`. Mostly settled; blocked on S1.

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
