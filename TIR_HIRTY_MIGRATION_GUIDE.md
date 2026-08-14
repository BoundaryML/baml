# TIR → `hir_ty` migration guide for the BEP-066 compiler port

Date: 2026-08-14

New-engine revision: `bbce7c0b68f71b72d4807a60317a10172a288db1`

Cutover: `a0f4605e8` (PR #4301)

Vanilla TIR baseline: `a0f4605e8^`

BEP-066 reference implementation: `antonio/s1-vm-bug`

## Scope, notation, and conclusion

This guide compares three different things which must not be conflated:

- **OLD** means the last vanilla TIR tree: `a0f4605e8^:<path>:<line>`.
- **BEP** means the behavior to port from the TIR-based BEP branch:
  `antonio/s1-vm-bug:<path>:<line>`.
- **NEW** means the clean canary tree at `bbce7c0b6`: `<path>:<line>`.
- **INV** means the companion conflict inventory at
  `/media/tony/WesternDigitalNvmeSsd/Code/baml-worktrees/green/PORT_INVENTORY.md:<line>`.
  It appeared during this research and was read only. Its 35 semantic facts are
  cross-referenced in section 4.

Within a citation group, a continuation such as `:240-365` means the same
revision and file as the immediately preceding full `file:line` citation.

The cutover is not a crate rename. TIR's mutable, per-scope builder was replaced
by per-body, span-free inference over interned types, an inference table, delayed
obligations, and durable side tables. MIR deliberately preserves a narrow
compatibility seam: it asks `hir_ty::infer_body` once, converts interned types to
plain types once, and then reads TIR-shaped provider tables
(`baml_language/crates/baml_compiler2_mir/src/inference_provider.rs:1-16`,
`:126-165`, `:182-204`, `:240-365`).

[PR #4301](https://github.com/BoundaryML/baml/pull/4301) describes the work as
a rust-analyzer-style, spec-fixture/differential migration through staged body
ownership, interned types, and a union-find inference table, followed by TIR
deletion rather than a line-for-line migration. The checked-in design ledger
corroborates those stages and explicitly records spec-ahead-of-TIR wins
(`baml_language/crates/baml_compiler2_hir_ty/README.md:1-22`,
`baml_language/crates/baml_compiler2_hir_ty/README.md:214-227`).

The practical outcome is:

1. **Reuse, do not duplicate, shared algebra.** The sealed reflection-kind
   subtype edge and deterministic Mint identity belong in
   `baml_type::normalize`. Both plain and interned subtype entry points converge
   on `NormalTy` (`baml_language/crates/baml_type/src/normalize.rs:2610-2650`,
   `:2822-2857`).
2. **Do not re-port wildcard holes as old TIR code.** The new engine already
   implements fresh inference variables for `_`, expression-position E0147,
   unresolved-hole E0147, and partial/open throws
   (`baml_language/crates/baml_compiler2_hir_ty/src/lower.rs:389-395`,
   `baml_language/crates/baml_compiler2_hir_ty/src/infer.rs:6384-6425`,
   `:6900-6928`, `:7086-7116`). Port the BEP tests and reconcile their exact
   desired outcomes.
3. **The runtime-type bridge is the material port.** Current canary has no
   first-class runtime type argument, scoped type binding, unreflect pattern,
   loc-free external target, `BindType`, or `RuntimeIsType`. The bridge must run
   AST/HIR → `BodyTypeRefs` → `InferenceResult/CallPlan` → MIR provider → MIR.
4. **Mounted/source-less package behavior is part of the same bridge.** The
   companion inventory found that canary's package interface is too narrow:
   `ExportedType` has no interface row and `ExportedFunction` lacks bounds and
   symbolic link/dispatch identity
   (`baml_language/crates/baml_compiler2_hir_ty/src/package_interface.rs:62-93`;
   `/media/tony/WesternDigitalNvmeSsd/Code/baml-worktrees/green/PORT_INVENTORY.md:187-194`).

Two evidence gaps are intentional rather than guessed around. PR #4301 refers
to a `doc-inference.md` design document which is not present in this checkout,
and the brief's
`thoughts/antonio/bep066-canary-4352-reconciliation-plan.md` is not present in
the `antonio/s1-vm-bug` tree. The merged PR description, the checked-in
`hir_ty/README.md`, the cutover diff, source, tests, and companion inventory are
therefore the authorities used here.

## 1. Architecture map

| Old TIR concept | Old evidence | New `hir_ty` home and migration consequence | New evidence |
|---|---|---|---|
| `builder.rs` threading | `ScopeInferenceBuilder` owned an `InferContext` plus a large set of mutable expression, pattern, resolution, call, coercion, and generic-binding tables. A builder was constructed and finished for each inference request (`OLD:baml_language/crates/baml_compiler2_tir/src/builder.rs:781-830`, `:997-1038`, `:1347-1395`). | `InferenceContext` is a per-`BodyOwnerId` walk containing the inference table, lowering context, flow state, obligations, pending diagnostics, and durable `InferenceResult` side tables. Add BEP per-body transient state here, but put anything MIR needs in `InferenceResult`/`CallPlan` before finish. Do not recreate `ScopeInferenceBuilder`. | `baml_language/crates/baml_compiler2_hir_ty/src/infer.rs:839-883`, `:923-986`, `:999-1162`, `:1280-1451` |
| `inference.rs` result/query and `infer_context.rs` | TIR returned `ScopeInference` with type, resolution, call, coercion, exhaustive-match, and diagnostic tables; the Salsa query was per semantic scope/lambda (`OLD:baml_language/crates/baml_compiler2_tir/src/inference.rs:815-862`, `OLD:baml_language/crates/baml_compiler2_tir/src/inference.rs:1304-1346`). `InferContext` was a run-local diagnostic sink with span-free `TirTypeError` payloads (`OLD:baml_language/crates/baml_compiler2_tir/src/infer_context.rs:1-8`, `OLD:baml_language/crates/baml_compiler2_tir/src/infer_context.rs:91-143`). | `infer_body` dispatches by function, let, or parameter-default owner and returns `InferenceResult`. `PendingDiag` is accumulated during inference and materialized after solving. BEP errors must become new engine-native pending/diagnostic variants; callers should not reconstruct facts from AST after inference. | `baml_language/crates/baml_compiler2_hir_ty/src/infer.rs:923-986`, `baml_language/crates/baml_compiler2_hir_ty/src/infer.rs:999-1162`, `baml_language/crates/baml_compiler2_hir_ty/src/infer.rs:7006-7116`; `baml_language/crates/baml_compiler2_hir_ty/src/diagnostics.rs:1-14`, `baml_language/crates/baml_compiler2_hir_ty/src/diagnostics.rs:116-194` |
| `lower_type_expr.rs` | A recursive lowering function accepted a generic environment and a `TypePosition`, resolved paths, and returned plain `Ty` plus diagnostics (`OLD:baml_language/crates/baml_compiler2_tir/src/lower_type_expr.rs:408-475`). BEP added an `ExtractionContract` position (`BEP:baml_language/crates/baml_compiler2_tir/src/lower_type_expr.rs:496-601`). | `LowerCtx` owns database, package, generic frame, facts, diagnostics, and interned-type lowering. Written body types are first collected into `BodyTypeRefs`, then `LowerCtx::lower_type_ref` produces interned `Ty`. Add extraction-contract semantics as an explicit lowering mode; add scoped generic bindings as an overlay rather than mutating the fixed declared frame. | `baml_language/crates/baml_compiler2_hir_ty/src/lower.rs:85-165`, `baml_language/crates/baml_compiler2_hir_ty/src/lower.rs:240-395`; `baml_language/crates/baml_compiler2_hir/src/body_type_refs.rs:23-61` |
| `resolve.rs` | TIR performed offset/span-sensitive name resolution on demand; `resolve_name_at` and path resolution combined lexical lookup, package lookup, and diagnostic context (`OLD:baml_language/crates/baml_compiler2_tir/src/resolve.rs:1-194`). | Lexical ownership and bindings are precomputed by HIR's semantic index; type paths resolve in `LowerCtx`, while value/callee/member paths resolve in `infer.rs` and `method_resolution.rs`. Results are recorded as `MemberResolution` or a per-segment `ResolvedPath` ladder. BEP shorthand and mounted lookup must preserve this split and ordinary-name-first shadowing. | `baml_language/crates/baml_compiler2_hir_ty/src/infer.rs:349-428`, `baml_language/crates/baml_compiler2_hir_ty/src/infer.rs:4882-4975`, `baml_language/crates/baml_compiler2_hir_ty/src/infer.rs:6788-6846`; `baml_language/crates/baml_compiler2_hir_ty/src/method_resolution.rs:565-660` |
| `throws_analysis.rs` / `throw_inference.rs` | TIR first computed transitive function throw sets with an eager call-graph pass, using declared clauses as firewalls, and separately walked inferred bodies to collect escaping throws (`OLD:baml_language/crates/baml_compiler2_tir/src/throw_inference.rs:1-4`, `OLD:baml_language/crates/baml_compiler2_tir/src/throw_inference.rs:98-151`, `OLD:baml_language/crates/baml_compiler2_tir/src/throw_inference.rs:224-330`; `OLD:baml_language/crates/baml_compiler2_tir/src/throws_analysis.rs:11-46`, `OLD:baml_language/crates/baml_compiler2_tir/src/throws_analysis.rs:155-177`). | Throws are now the callable's error channel. `callable_throws` is a cycle-seeded Salsa fixpoint; inference records throw contributions, catch subtracts handled cases, and finalization closes partial clauses. Structural throw facts use `reachable_excluding_lambdas`. New hidden BEP operands must become real HIR child edges so both inference and fact extraction see them. | `baml_language/crates/baml_compiler2_hir_ty/src/callable.rs:1-90`; `baml_language/crates/baml_compiler2_hir_ty/src/infer.rs:6428-6591`, `baml_language/crates/baml_compiler2_hir_ty/src/infer.rs:6736-6774`, `baml_language/crates/baml_compiler2_hir_ty/src/infer.rs:6900-6928`; `baml_language/crates/baml_compiler2_hir_ty/src/throw_facts.rs:525-552` |
| `normalize.rs` and subtyping | TIR's local `normalize.rs` chiefly expanded aliases with cycle detection; semantic equivalence/subtyping was already shared in `baml_type::normalize` (`OLD:baml_language/crates/baml_compiler2_tir/src/normalize.rs:1-12`). | The shared normalizer remains the semantic oracle. `NormalTy::from_interned` feeds the same canonical pipeline used by plain `Ty`; `is_subtype_interned` canonicalizes and calls `NormalTy::is_subtype_of`. BEP algebra changes belong once in shared `NormalTy`, never in a private `hir_ty` subtype switch. Inference variables are handled around the oracle by `InferenceContext::sub`. | `baml_language/crates/baml_type/src/normalize.rs:68-115`, `baml_language/crates/baml_type/src/normalize.rs:210-304`, `baml_language/crates/baml_type/src/normalize.rs:2610-2650`, `baml_language/crates/baml_type/src/normalize.rs:2822-2857`; `baml_language/crates/baml_compiler2_hir_ty/src/infer.rs:2634-2704`, `baml_language/crates/baml_compiler2_hir_ty/src/infer.rs:2851-2858` |
| Interface/impl facts | TIR mixed interface lookup, candidate selection, substitutions, witness recording, and diagnostics into builder submodules. | `impls.rs` owns the registry/candidate relation, `interfaces.rs` and its submodules own interface facts/coherence/rules, `method_resolution.rs` owns member selection, `Facts` supplies the normalizer's fact boundary, and obligations delay proofs until types ground. Mounted equivalents must extend those same abstractions rather than bypassing them from `infer.rs`. | `baml_language/crates/baml_compiler2_hir_ty/src/impls.rs:460-646`; `baml_language/crates/baml_compiler2_hir_ty/src/infer/obligations.rs:1-24`, `:36-201`; `baml_language/crates/baml_compiler2_hir_ty/README.md:46-81` |
| MIR consumption | TIR MIR cached `ScopeInference` per scope, called `infer_scope_types`, and served direct `tir_*` lookups (`OLD:baml_language/crates/baml_compiler2_mir/src/lower.rs:1011-1018`, `OLD:baml_language/crates/baml_compiler2_mir/src/lower.rs:1334-1341`, `OLD:baml_language/crates/baml_compiler2_mir/src/lower.rs:1899-1914`, `OLD:baml_language/crates/baml_compiler2_mir/src/lower.rs:2206-2220`, `OLD:baml_language/crates/baml_compiler2_mir/src/lower.rs:2534-2603`). | MIR now constructs `ProviderTables` with `infer_body`, converts interned to plain once, and serves the existing accessors. Match polarity is deliberately inverted at the seam. Extend both the engine-native plan and provider plan for BEP runtime operands/targets, then lower only the provider's authoritative plan. | `baml_language/crates/baml_compiler2_mir/src/inference_provider.rs:1-16`, `baml_language/crates/baml_compiler2_mir/src/inference_provider.rs:81-103`, `baml_language/crates/baml_compiler2_mir/src/inference_provider.rs:126-165`, `baml_language/crates/baml_compiler2_mir/src/inference_provider.rs:182-204`, `baml_language/crates/baml_compiler2_mir/src/inference_provider.rs:240-365`; `baml_language/crates/baml_compiler2_mir/src/lower.rs:1481-1504`, `baml_language/crates/baml_compiler2_mir/src/lower.rs:2483-2559` |

The architectural rule for the port is therefore:

`syntax/HIR identity → interned inference facts → durable side table → one plain-type conversion → MIR`.

No stage after `hir_ty` should re-resolve a written type argument or infer whether
it was runtime-computed. That provenance must be explicit in the side table.

## 2. Type representation: plain `Ty` versus `baml_type::interned`

### Plain `Ty` in TIR and at runtime boundaries

TIR did not own a private recursive type algebra: its `ty.rs` re-exported the
plain shared `baml_type::Ty`
(`OLD:baml_language/crates/baml_compiler2_tir/src/ty.rs:1-18`). Plain `Ty` is an
owned recursive enum: composite constructors allocate `Vec`/`Box` children, and
derived equality, hashing, and ordering are structural
(`baml_language/crates/baml_type/src/lib.rs:455-529`). It also carries
compiler/recovery forms such as `Infer`, `Unknown`, and evolving
list/map variants. `validate_runtime` rejects forms which must not cross a
runtime boundary (`baml_language/crates/baml_type/src/lib.rs:589-597`).

This remains the appropriate family for serialized package interfaces, MIR,
VM-facing types, diagnostics, and BEP's stable Mint digest. It is not the right
representation for live inference variables.

### Interned `Ty` in `hir_ty`

`baml_type::interned::Ty` is a one-word handle into a global hash-cons pool.
Children are handles, flags are cached, and cloning is cheap
(`baml_language/crates/baml_type/src/interned.rs:1-34`,
`:47-62`, `:91-116`). Equality and `Hash` use the intern identity; deterministic
`Ord` is structural rather than pointer-address order
(`baml_language/crates/baml_type/src/interned.rs:170-200`).
`InterfaceRef::new` sorts associated-type pins, making their representation
canonical at construction (`baml_language/crates/baml_type/src/interned.rs:380-447`).

Important consequences:

- Construct with `Ty::intern(TyKind::...)` or provided constructors; do not
  assemble plain children and repeatedly convert.
- O(1) intern equality is valid for already-canonical identical nodes, but
  semantic equivalence still goes through normalization because unions,
  aliases, literals, and interface facts can make differently shaped nodes
  equivalent.
- Never use the interned handle's `Hash` for `MintId` or serialized identity.
  Its identity is process-local. BEP's digest explicitly canonicalizes the
  plain semantic form and uses a fixed byte-level hash
  (`BEP:baml_language/crates/baml_type/src/normalize.rs:475-504`).

Conversions are deliberately asymmetric. Plain → interned recursively interns
the tree; legacy recovery-only plain variants are rejected, and a plain
`Ty::Infer` becomes an anonymous hole
(`baml_language/crates/baml_type/src/interned.rs:650-737`). Interned → plain
cannot preserve a live solver variable's identity; results must be fully
resolved before `to_plain`
(`baml_language/crates/baml_type/src/interned.rs:740-835`).

### Normalization, canonical form, and comparison

The stable semantic sequence is:

1. Convert either family into `NormalTy` with a `TypeContext`.
2. Expand aliases and recursive binders under a fuel/cycle discipline.
3. Canonicalize unions/set algebra and erase non-semantic attributes.
4. Compare canonical forms for equivalence, or run `is_subtype_of` for the
   subset relation.

The public plain operations are at
`baml_language/crates/baml_type/src/normalize.rs:210-304` and `:457-472`.
The interned ingress and operations are at `:2610-2650` and `:2822-2882`.
This is why the kind-class head-category rule and `kind <: type` arm must be
added to `NormalTy` only once.

### Inference variables, written holes, and unification

Interned `TyKind::Infer(Option<InferVar>)` distinguishes an uninstantiated
written hole (`None`) from a table-owned variable (`Some`); `InferVar` is a
compact index (`baml_language/crates/baml_type/src/interned.rs:67-89`,
`:216-352`). `LowerCtx` preserves a written `_` as the variable-less form
(`baml_language/crates/baml_compiler2_hir_ty/src/lower.rs:389-395`).
Inference then instantiates that marker with a fresh table variable at permitted
annotation positions (`baml_language/crates/baml_compiler2_hir_ty/src/infer.rs:6333-6403`).
Thus, the replacement for “put `Ty::Infer` in a recursive plain tree and chase
it later” is:

`written TyKind::Infer(None) → fresh InferVar → table constraints/bounds → resolved interned Ty → plain boundary`.

`InferenceTable` stores variable values and lower/upper bounds, provides fresh
value/effect variables, path compression/resolution, snapshots and rollback,
occurs-checking, and structural unification
(`baml_language/crates/baml_compiler2_hir_ty/src/infer/unify.rs:46-152`,
`:199-302`, `:304-414`, `:416-610`). `InferenceContext::sub` handles variables
by updating table bounds and sends ground types to `is_subtype_interned`
(`baml_language/crates/baml_compiler2_hir_ty/src/infer.rs:2634-2704`,
`:2786-2858`). Interface/operator proof is a delayed obligation, not a
side-effect of plain-type unification
(`baml_language/crates/baml_compiler2_hir_ty/src/infer.rs:2904-2938`;
`baml_language/crates/baml_compiler2_hir_ty/src/infer/obligations.rs:92-201`).
Finish interleaves bound solving and obligations to a fixpoint, defaults
unconstrained effect variables to `never`, turns unresolved value variables
into local errors, and ensures no live infer variable reaches
`InferenceResult` (`baml_language/crates/baml_compiler2_hir_ty/src/infer.rs:6849-6928`,
`:7086-7116`; `baml_language/crates/baml_compiler2_hir_ty/README.md:35-44`).

## 3. Worked examples: three cutover Rosetta stones

### 3.1 Generic call bounds

**Before.** TIR allocated a mutable map from declared `ParamTy` to plain `Ty`,
inferred explicit/expected/argument evidence in phases, and then built a second
`bound_check_bindings` map to validate declared interface bounds immediately
(`OLD:baml_language/crates/baml_compiler2_tir/src/builder.rs:3775-3867`,
`:4060-4087`, `:4109-4166`). Generic solution and bound proof were therefore
interleaved inside the call builder.

**After.** `hir_ty` creates one fresh table variable per generic slot, lowers
written `_` to fresh variables, and performs two-pass argument checking
(`baml_language/crates/baml_compiler2_hir_ty/src/infer.rs:3915-3929`,
`:5579-5638`). `register_call_bounds` substitutes the whole instantiation into
each bound and registers an `Implements` obligation
(`baml_language/crates/baml_compiler2_hir_ty/src/infer.rs:5487-5536`).
The obligation stalls while its subject is ambiguous and is retried during the
finish fixpoint
(`baml_language/crates/baml_compiler2_hir_ty/src/infer/obligations.rs:92-201`,
`baml_language/crates/baml_compiler2_hir_ty/src/infer.rs:6849-6868`).
The final ordered instantiation is written into `CallPlan.type_args`
(`baml_language/crates/baml_compiler2_hir_ty/src/infer.rs:453-485`).

**Port pattern.** Runtime-computed generic slots must be represented explicitly
in that ordered instantiation plan. Keep their bounded/top static occurrence
type for ordinary inference, but do not register a normal static obligation
whose truth depends on the opaque runtime value. Mark the check as runtime
deferred. This follows the new “register facts, solve later, persist the plan”
architecture; copying TIR's local `HashMap` and early `continue` statements
would make solver ordering observable.

### 3.2 Interface witness and implementation resolution

**Before.** TIR's interface resolver enumerated matching impls for a concrete
receiver, selected or diagnosed ambiguity, selected an override/default method,
and recorded either concrete or virtual dispatch metadata
(`OLD:baml_language/crates/baml_compiler2_tir/src/builder/interface_resolution.rs:502-628`,
`:906-997`).

**After.** `impls.rs` builds and queries the interned-native candidate registry
(`baml_language/crates/baml_compiler2_hir_ty/src/impls.rs:460-540`,
`:557-646`). `method_resolution.rs` combines receiver facts, carried generic
bounds, interface member lookup, and impl candidates
(`baml_language/crates/baml_compiler2_hir_ty/src/method_resolution.rs:565-660`).
Inference records a location-keyed `InterfaceVirtualMethod` or
`InterfaceConcreteMethod` in `MemberResolution`
(`baml_language/crates/baml_compiler2_hir_ty/src/infer.rs:357-406`,
`:5844-5940`, `:6030-6065`); the MIR provider converts the enum variant-for-
variant (`baml_language/crates/baml_compiler2_mir/src/inference_provider.rs:368-413`).

**Port pattern.** A mounted package has no source `FunctionLoc`/`ImplLoc`, so
do not forge one. Add an exported interface row and a loc-free symbolic target
(free function, direct method, or interface slot), teach the same registry and
member-resolution tiers about it, record that target in `MemberResolution` and
`CallPlan`, and convert it at the provider boundary. BEP's old descriptor
already carries the needed owner/function generic frames, bounds, `self` mode,
and dispatch target (`BEP:baml_language/crates/baml_compiler2_tir/src/inference.rs:853-902`).

There is an intentional semantic delta to preserve as a decision point:
`hir_ty` enforces a stricter one-`Self` existential dispatch gate than TIR
(`baml_language/crates/baml_compiler2_hir_ty/README.md:65-71`). Mounted witness
resolution must follow the new rule unless the type-system owner explicitly
changes it.

### 3.3 Throws and match exhaustiveness

**Before.** TIR ran eager transitive throw inference, then a separate
post-inference escaping-throws traversal. Match results were stored positively
in `ScopeInference.exhaustive_matches`
(`OLD:baml_language/crates/baml_compiler2_tir/src/throw_inference.rs:1-4`,
`:224-330`; `OLD:baml_language/crates/baml_compiler2_tir/src/throws_analysis.rs:155-177`;
`OLD:baml_language/crates/baml_compiler2_tir/src/inference.rs:839-846`).

**After.** `callable_throws` uses a `never` cycle seed and Salsa convergence for
mutually recursive functions
(`baml_language/crates/baml_compiler2_hir_ty/src/callable.rs:50-90`).
The body walk records throw contributions directly, catch subtracts handled
types, and partial clauses combine the declared closed portion with inferred
residue (`baml_language/crates/baml_compiler2_hir_ty/src/infer.rs:6428-6591`,
`:6736-6774`, `:6900-6928`). Pattern usefulness was lifted, but
`InferenceResult` records **non-exhaustive** matches
(`baml_language/crates/baml_compiler2_hir_ty/src/infer.rs:839-855`); the MIR
provider reverses that polarity for its legacy accessor
(`baml_language/crates/baml_compiler2_mir/src/inference_provider.rs:148-165`).

**Port pattern.** `unreflect(e)` operands and `type T = unreflect(e)` values must
be included as HIR child expressions. Then ordinary expression inference and
`throw_facts::body_nodes` see their calls/throws without a BEP-specific second
walker. An unreflect pattern's synthetic usefulness node must be possible but
non-covering, because its runtime type is not statically known.

## 4. Landing spots for BEP-066 facts

### 4.1 Cross-check against the complete companion ledger

The companion inventory contains 35 facts (I-01..03, L-01..05, R-01, T-01,
B-01..22, N-01..03) at
`/media/tony/WesternDigitalNvmeSsd/Code/baml-worktrees/green/PORT_INVENTORY.md:33-185`.
The table below accounts for every one.
“Replay” means the BEP source remains conceptually valid across the cutover;
“re-express” means the old behavior is required but its ownership/representation
changed.

| Inventory facts | Disposition and exact new landing | How | Confidence |
|---|---|---|---|
| N-01 | Replay `canonical_digest` in shared `baml_type/src/normalize.rs`; VM/cache consumers remain outside inference. | Preserve canonical `NormalTy` input, fixed FNV-1a-64 token encoding, fixed-width big-endian numbers, and widened pointer-sized integers. Never hash intern handles. | High |
| N-02, N-03 | Replay in `NormalTy::head_category` and `NormalTy::is_subtype_of`. | Classify only the sealed builtin kind QNames as `Category::Type` and accept those classes, but no user class, below primitive `type`. The existing interned entry point reaches the same code. | High |
| I-01 | Re-express beside `callable.rs`/`package_interface.rs` and extend `infer.rs::{MemberResolution, CallPlan}` plus the MIR provider. | Use a serializable loc-free target enum (`Free`/`Method`/`Interface`), receiver mode, owner+call generic frames, bounds, and linkability. Do not synthesize source locations. | Medium-high |
| I-02, I-03 | Extend `PackageInterface::ExportedType` and `Facts`/`interfaces.rs`. | Add interface exports, alias/enum facts, associated names/defaults/`requires`, and a visited-set requires closure. Feed these facts to the same normalizer and projection machinery as local definitions. | Medium |
| L-01, L-02, L-03 | Re-express in `LowerCtx::resolve_type_definition/lower_path` through `PackageResolutionContext`. | Return an own/source-backed or foreign/exported definition; apply identical arity, enum-variant, pin, default, duplicate, and missing-binding validation. | Medium-high |
| L-04, R-01, B-20 | Re-express with one shared shadow-preserving package-prefix helper used by `lower.rs` and `infer.rs::resolve_value_path`. | Try local/real package resolution first. Only on failure reinterpret `reflect`, `type`, or `json` under accessible package `baml`, preserving the entire path. | High |
| L-05, B-09 | Add `TypePosition::ExtractionContract` and persist the result in `CallPlan`. | Enable it only for exact `baml.reflect.Package.get_function<F>`. Missing outer `throws` becomes the runtime wildcard; ordinary function types retain their diagnostic and `never` recovery. MIR must consume the solved plan rather than re-lower it as an ordinary type. | High |
| T-01, B-12, B-22 | Extend AST/HIR reachability and `BodyTypeRefs` child collection; reuse `throw_facts` and defaults traversal. | Make runtime type-argument operands and type-binding values ordinary reachable child nodes, excluding lambdas under the same rule as every other expression. Keep default-body `InferenceContext` state isolated. | High |
| B-01, B-10 | Extend per-body `InferenceContext` transient state and durable `InferenceResult::CallPlan`. | Build a plan once and enrich it in place: parameter bindings, full solved instantiation, owner split, per-slot static/runtime provenance, operand `ExprId`, deferred checks, runtime id, and loc-free target. Optional calls use the same plan. | High |
| B-02, B-13 | Add a scoped generic overlay shared by inference-time type lowering. | On `type T = unreflect(value)` validate `value <: type`, create a rigid synthetic `ParamTy` keyed by statement identity, push it at block entry, and erase/truncate it from escaping result/local types on both infer and check exits. The declared `LowerCtx` frame is fixed today, so this is new design. | Medium-low |
| B-03 | Add a callee-specific finalization exception in `infer_call`/`write_call_type_args`. | Only the `Session.eval` result slot may resolve an otherwise unbound variable to static top/`unknown`; keep E0147/error behavior for ordinary unbound generics. | High |
| B-04 | Enrich `ExportedFunction` and callee instantiation. | Include owner and function generic params/bounds, exclude synthetic effect params from user arity, seed receiver substitutions, and include mounted functions in unspecialized-generic checks. | Medium-high |
| B-05, B-06, B-07 | Change `BodyTypeRefs.expr_type_args` from bare `TypeRefId` slots to a static/runtime enum, then extend `instantiation_args`. | Static slots lower normally. Runtime slots infer their operand, require operand `<: type` (with error/pending cascade suppression), mark the slot runtime-checked, and use the parameter's first bound or static top as its compile-time occurrence type. A value-shaped bare slot emits the targeted “use `unreflect(...)`” error. | Medium-high |
| B-08 | Add explicit runtime-deferred bound/argument metadata to the call plan or obligation layer. | Always infer argument expressions. Skip only checks whose expected type or bound mentions a runtime slot; leave all-static checks and unrelated conjunctive bounds active. VM lowering receives the deferred gate. | Medium |
| B-11 | Add a narrow intrinsic branch after named LLM target resolution. | For uncontracted `render_prompt`, `build_request`, and `build_request_stream`, seed otherwise-unconstrained schema `T` from a non-generic target function's return type and mark its runtime layout. | Medium-high |
| B-14 | Add the guard to both inferred and expectation-driven branches in `infer_object`. | If the resolved class QName is a sealed reflection-kind class, infer field expressions for recovery, emit `CannotConstructReflectionKind`, and return error. | High |
| B-15, B-16 | Extend `PackageResolutionContext`, `method_resolution.rs`, and durable resolutions. | Resolve source-less functions/types/variants/UFCS methods, substitute receiver class args into fields, handle bound versus unbound `self`, and preserve direct-method/interface-slot targets. Ordinary/local resolution wins. | Medium-high |
| B-17 | Add exported callable linkability and diagnose in `infer_call`. | A mounted builtin without a loc-free link contract is reserved and reliably emits `MountedPackageCallUnsupported`, including optional calls. | High |
| B-18 | Gate streaming desugaring before committing the plan. | Reject any runtime-computed generic slot through `$stream`/`__make_stream`. | High |
| B-19 | Key `from_json` reconstruction from the richer plan. | Run the old static reconstruction only when every relevant type slot is static; do not fold runtime operands into it. | High |
| B-21 | Extend `infer/pat.rs` and AST/HIR pattern traversal. | Infer operand `<: type`, retain the scrutinee type, emit a distinct rigid synthetic DPat per occurrence, introduce no bindings, make it possible but non-covering, and reject it where a binding-shaped rest subpattern is required. | High |

The inventory also classifies the actual green merge: 39 conflicts are
mechanical/deleted legacy artifacts, 17 are semantic-port boundaries, and 32
are snapshots to regenerate only after semantics land
(`/media/tony/WesternDigitalNvmeSsd/Code/baml-worktrees/green/PORT_INVENTORY.md:219-228`,
`/media/tony/WesternDigitalNvmeSsd/Code/baml-worktrees/green/PORT_INVENTORY.md:282-347`).
The 17 semantic paths include `hir_ty` diagnostics/interfaces/package
interface, MIR/emit/project wiring, shared unify behavior, and the deleted TIR
source whose behavior must be extracted
(`/media/tony/WesternDigitalNvmeSsd/Code/baml-worktrees/green/PORT_INVENTORY.md:282-307`).

### 4.2 Sealed reflection `kind <: type`

**BEP behavior.** The branch defines a closed list of nine reflection kind
classes and a predicate over qualified names
(`BEP:baml_language/crates/baml_type/src/type_kind.rs:1-17`, `:46-84`).
It maps their normal-form head category to `Type` and adds the exact
`Class(kind) <: Type` arm
(`BEP:baml_language/crates/baml_type/src/normalize.rs:772-774`,
`:2035-2042`). Object construction is separately rejected in both inferred and
expected-type paths
(`BEP:baml_language/crates/baml_compiler2_tir/src/builder.rs:6567-6576`,
`:6725-6734`).

**Landing.**

- Replay `type_kind.rs` and both `NormalTy` arms. Current
  `is_subtype_interned` already converts to `NormalTy` and calls
  `is_subtype_of`, so adding a second rule in `hir_ty::sub` would create drift
  (`baml_language/crates/baml_type/src/normalize.rs:2822-2836`).
- Add the construction guard at the resolved-class point in
  `InferenceContext::infer_object`
  (`baml_language/crates/baml_compiler2_hir_ty/src/infer.rs:5641-5688`).
- Validate `unreflect` operands by calling normal `self.sub(operand, type)`,
  allowing kind values because of the shared edge; BEP itself changed from
  exact equality to subtype checking for this reason
  (`BEP:baml_language/crates/baml_compiler2_tir/src/builder.rs:2567-2590`).

Confidence: **high**. The only policy question is the authoritative nine-name
list; it must stay sealed.

### 4.3 `_` holes in `Future<int, _>` and `throws X | _`

This is not a blank-slate port.

Current AST lowering admits a top-level wildcard in a throws clause but rejects
holes in declaration positions
(`baml_language/crates/baml_compiler2_ast/src/lower_type_expr.rs:629-693`).
`hir_ty` lowers the admitted marker to `TyKind::Infer(None)`, creates a fresh
table variable when it instantiates body annotations, rejects prohibited
expression-position holes immediately with E0147, and emits E0147 for any
allowed hole still unsolved at finalization
(`baml_language/crates/baml_compiler2_hir_ty/src/lower.rs:389-395`,
`:1344-1354`; `baml_language/crates/baml_compiler2_hir_ty/src/infer.rs:6333-6425`,
`:7086-7116`). Open throws are split from the closed annotation and recombined
with inferred contributions (`baml_language/crates/baml_compiler2_hir_ty/src/infer.rs:1108-1130`,
`baml_language/crates/baml_compiler2_hir_ty/src/lower.rs:2446-2470`,
`baml_language/crates/baml_compiler2_hir_ty/src/infer.rs:6751-6774`,
`:6900-6928`).

The BEP behavior suite is specification evidence, not proof that old TIR
implemented the positives: the B-230/B-247 cases live at
`BEP:baml_language/crates/baml_tests/tests/wildcard_type_inference.rs:36-239`,
but important positive cases are `#[ignore]`d (`:38`, `:79`, `:91`, `:184`,
`:207`), and BEP TIR lowering still converts `TypeRefKind::Infer` to error
(`BEP:baml_language/crates/baml_compiler2_tir/src/lower_type_expr.rs:783-792`).

**Landing.** Preserve the current `hir_ty` mechanism. Port/unignore the behavior
tests in stages, assert that `Future<int, _>` is solved from use, assert that
`throws X | _` exposes declared ∪ inferred throws, and retain E0147 when no
constraint determines a hole. Do not transplant the old TIR rejection.

Confidence: **high** for architecture, **medium** for exact B-230/B-247 expected
outcomes until every formerly ignored case is reviewed against
`TYPE_SYSTEM.md`.

### 4.4 `unreflect(e)` and scoped runtime type values end to end

#### Front end: replayable, but absent from clean canary

BEP's AST changes `Call.type_args` from bare type expressions to
`TypeArg::{Static, Unreflect(ExprId)}` and adds `Pattern::Unreflect`
(`BEP:baml_language/crates/baml_compiler2_ast/src/ast.rs:192-201`,
`:896-901`, `:1235-1237`). Its parser recognizes `unreflect` contextually for a
whole generic slot and keeps the operand opaque to type parsing
(`BEP:baml_language/crates/baml_compiler_parser/src/parser.rs:6783-6810`,
`:7109-7126`). The HIR/body builder walks runtime operands
(`BEP:baml_language/crates/baml_compiler2_hir/src/builder.rs:612-615`,
`:681-694`).

Clean canary still has `Call.type_args: Vec<TypeExpr>` and no unreflect pattern
(`baml_language/crates/baml_compiler2_ast/src/ast.rs:871-876`,
`:1197-1209`). The companion merge establishes that these front-end changes
auto-merge semantically; its two parser/lexer conflicts are unrelated
(`/media/tony/WesternDigitalNvmeSsd/Code/baml-worktrees/green/PORT_INVENTORY.md:208-217`).
Therefore “survive” means **replay/merge without inference redesign**, not
“already present in this checkout.”

#### HIR bridge: first architectural change

Current `BodyTypeRefs.expr_type_args` stores only `TypeRefId` and blindly lowers
each expression call slot as a type
(`baml_language/crates/baml_compiler2_hir/src/body_type_refs.rs:23-47`,
`:102-113`). Replace the slot with an enum such as:

`Static(TypeRefId) | Runtime { operand: ExprId }`.

Also represent `Stmt::TypeBinding` and unreflect-pattern operands in canonical
HIR traversal. This is the first non-mechanical cutover bridge
(`/media/tony/WesternDigitalNvmeSsd/Code/baml-worktrees/green/PORT_INVENTORY.md:187-194`).

#### `hir_ty` inference and call planning

BEP TIR inferred every runtime operand, checked it below primitive `type`,
recorded the affected generic parameter, and chose its first declared bound or
static top/unknown as the occurrence type
(`BEP:baml_language/crates/baml_compiler2_tir/src/builder.rs:2567-2590`,
`:2690-2731`). It seeded those values before normal call phases
(`BEP:baml_language/crates/baml_compiler2_tir/src/builder.rs:4195-4238`), skipped only argument/bound checks dependent
on runtime parameters (`BEP:baml_language/crates/baml_compiler2_tir/src/builder.rs:4414-4428`), and kept runtime type
operands separate through call-plan construction
(`BEP:baml_language/crates/baml_compiler2_tir/src/builder.rs:4553-4625`).

Current `CallPlan` records parameter bindings, solved type args, owner offset,
one call-wide `explicit` bit, and runtime id—no per-slot provenance or operand
(`baml_language/crates/baml_compiler2_hir_ty/src/infer.rs:453-485`).
`instantiation_args` assumes every written slot is a `TypeRefId`
(`baml_language/crates/baml_compiler2_hir_ty/src/infer.rs:5579-5625`), and
`register_call_bounds` statically registers every bound
(`baml_language/crates/baml_compiler2_hir_ty/src/infer.rs:5487-5536`).

Extend the plan with per-slot information and a runtime-check/deferred-check
set. The exact runtime type value must **not** be modeled as a normal inference
variable which finalization is expected to solve: it is opaque until execution.
Its static occurrence type is the declared existential bound (or top when
unbounded), while the runtime operand and bound gate travel independently to
MIR. Merge plan writes rather than replacing them, so argument matching cannot
erase earlier runtime/extraction decisions.

#### Scoped binding and patterns

BEP's `type T = unreflect(value)` branch installs a rigid synthetic `ParamTy`
and manages block escape/truncation (`BEP:baml_language/crates/baml_compiler2_tir/src/builder.rs:7844-7898`). The new
`LowerCtx` has a fixed generic frame
(`baml_language/crates/baml_compiler2_hir_ty/src/lower.rs:85-119`), so the port
needs a block-scoped overlay owned by `InferenceContext`. At block exit,
substitute/erase an escaping scoped parameter in both result and locals before
truncating the overlay. This is the least mechanical compiler portion.

BEP unreflect patterns create distinct synthetic rigid types
(`BEP:baml_language/crates/baml_compiler2_tir/src/builder.rs:17160-17182`). Port this to
`baml_compiler2_hir_ty/src/infer/pat.rs`: validate the operand, preserve the
scrutinee's inferred type, produce no bindings, and feed usefulness a unique
possible-but-non-covering pattern.

#### MIR

BEP MIR has a type-binding statement, runtime type-check intrinsic, and
runtime-type-argument flag/operands
(`BEP:baml_language/crates/baml_compiler2_mir/src/ir.rs:301-313`,
`:415-431`, `:826-832`;
`BEP:baml_language/crates/baml_compiler2_mir/src/lower.rs:9465-9515`,
`:11639-11653`, `:13908-13919`). Current MIR has none of
`BindType`/`RuntimeIsType` and its intrinsic enum contains only `Log`
(`baml_language/crates/baml_compiler2_mir/src/ir.rs:290-295`).

Replay the IR/runtime pieces, but feed them from the expanded provider plan:

- runtime call slots lower their operand expressions as hidden runtime type
  operands in declared generic order;
- `TypeBinding` lowers to `BindType`;
- `Pattern::Unreflect` lowers to `RuntimeIsType`;
- loc-free targets lower to their symbolic runtime link rather than a source
  location;
- MIR never re-parses/re-lowers the written type-argument AST.

The current conversion loses explicit written type args on purpose because MIR
re-lowers them (`baml_language/crates/baml_compiler2_mir/src/inference_provider.rs:307-326`).
BEP invalidates that shortcut for mixed static/runtime lists. Make the provider
carry an authoritative per-slot plan and migrate static slots too, or very
carefully preserve the old path only for all-static calls.

Confidence: **high** on the pipeline and semantics, **medium** on the exact
`BodyTypeRefs`/`CallPlan` data shape, **medium-low** on scoped-frame lifetime.

### 4.5 Mounted packages, external calls, shorthands, and special intrinsics

These facts are easy to miss if the port is scoped only to the four headline
features. BEP extended TIR inference with a loc-free `ExternalCallable`
(`BEP:baml_language/crates/baml_compiler2_tir/src/inference.rs:853-902`) and
extended lowering/resolution/call building for source-less mounted definitions.
Its exported schema includes interface rows, associated metadata, generic-bound
conjunctions, and stable callable identity
(`BEP:baml_language/crates/baml_compiler2_tir/src/package_interface.rs:83-209`);
type lowering explicitly distinguishes `Own` from `Foreign` definitions and
mirrors local validation for foreign interface/class/enum/alias rows
(`BEP:baml_language/crates/baml_compiler2_tir/src/lower_type_expr.rs:313-398`,
`BEP:baml_language/crates/baml_compiler2_tir/src/lower_type_expr.rs:1225-1392`).
Current `MemberResolution` is entirely source-location based
(`baml_language/crates/baml_compiler2_hir_ty/src/infer.rs:349-406`), while
`PackageInterface::ExportedType` supports only class/enum/alias and exported
functions omit declared generic bounds and link/dispatch metadata
(`baml_language/crates/baml_compiler2_hir_ty/src/package_interface.rs:62-93`).
Canary does have a partial `PackageResolutionContext` for exported type/method
lookup (`baml_language/crates/baml_compiler2_hir_ty/src/package_interface.rs:828-940`,
`baml_language/crates/baml_compiler2_hir_ty/src/package_interface.rs:954-994`),
but `LowerCtx` still resolves dependency paths through source-backed
`package_items` (`baml_language/crates/baml_compiler2_hir_ty/src/lower.rs:942-984`,
`baml_language/crates/baml_compiler2_hir_ty/src/lower.rs:987-1017`). That
existing context is useful substrate, not a completed mounted-package port.

Required design:

- Extend serialized package interfaces with interfaces, associated types,
  defaults, `requires`, generic bounds, fields/method metadata, builtin
  linkability, and a symbolic target.
- Return a sum type for own/source-backed versus exported/source-less
  definitions from package resolution.
- Feed exported facts into `Facts`, `interfaces`, `impls`, projections, and
  `method_resolution` so local and mounted types share semantic rules.
- Preserve shadowing: ordinary local/package names win before the `baml.reflect`,
  `baml.type`, and `baml.json` shorthand fallback.
- Keep special behavior narrow: extraction-contract only for exact
  `Package.get_function`; LLM schema seeding only for the three named helpers;
  no runtime slots through streaming; `from_json` reconstruction only for an
  all-static plan; unsupported mounted builtins get their targeted error.
- Carry the loc-free target through `InferenceResult`, the provider, MIR, emit,
  and `baml_project` database/package wiring. The companion identifies those
  exact semantic conflict paths
  (`/media/tony/WesternDigitalNvmeSsd/Code/baml-worktrees/green/PORT_INVENTORY.md:284-295`).

Confidence: **medium-high** for lookup/call semantics; **medium** for the
serialized exported-interface schema and ownership of the target descriptor.
Those two data-model choices should be agreed with the `hir_ty` author before
parallel implementation.

### 4.6 Stable `MintId` canonical digest

BEP's `canonical_digest` contract says semantic equivalents share a deterministic
64-bit digest across processes and architectures, while explicitly stating it
is not an on-wire persistence format
(`BEP:baml_language/crates/baml_type/src/normalize.rs:475-504`). The VM caches a
spelled static type to the digest and allocates the corresponding runtime type
value (`BEP:baml_language/crates/bex_vm/src/vm.rs:850-858`, `:1392-1421`).

Replay this in shared `baml_type::normalize`. It is independent of the inference
cutover, except that using interned `Ty`'s pointer identity would silently break
the determinism contract. Test both equivalent spellings and repeatability
across fresh intern constructions. Confidence: **high**.

## 5. Diagnostics

`hir_ty/src/diagnostics.rs` was deliberately relocated from TIR with
engine-neutral, span-free payloads; source locations are attached later and
rendered through the shared diagnostic stack
(`baml_language/crates/baml_compiler2_hir_ty/src/diagnostics.rs:1-14`,
`:1959-2025`, `:2157-2168`). Inference accumulates `PendingDiag` while variables
are live and constructs final diagnostics only after resolution
(`baml_language/crates/baml_compiler2_hir_ty/src/infer.rs:7006-7116`).
LSP action code maps these diagnostics to shared IDs/messages
(`baml_language/crates/baml_lsp2_actions/src/check.rs:948-1025`); for example,
`CannotInferType` maps to the wildcard-not-allowed diagnostic
(`baml_language/crates/baml_lsp2_actions/src/check.rs:1349-1360`).

E0147 is already stable in the shared ID enum and mapping
(`baml_language/crates/baml_compiler_diagnostics/src/diagnostic.rs:199-200`,
`:481-486`). The cutover did not renumber the shared diagnostic ID enum; it
moved production from `TirTypeError` into `hir_ty` diagnostics.

BEP's shared factory is engine-neutral by construction:
`baml_compiler_diagnostics/src/runtime_type.rs` centralizes exact code/message
ownership and has direct factory tests
(`BEP:baml_language/crates/baml_compiler_diagnostics/src/runtime_type.rs:1-6`,
`:71-123`, `:130-164`). It is absent from clean canary only because BEP has not
landed, not because `hir_ty` replaced it. Replay it and the accompanying
`DiagnosticId` additions unchanged, then:

1. add `PendingDiag`/final diagnostic variants in `hir_ty` for
   `ComputedGenericArgumentRequiresUnreflect`,
   `CannotConstructReflectionKind`, runtime streaming restrictions, mounted
   unsupported calls, and related BEP cases;
2. map them in LSP/compiler diagnostics by calling the shared factory;
3. keep type information in interned form while pending, materializing plain
   render payloads only after finish;
4. assert code and full message parity at the shared factory and checker/LSP
   layers.

The green inventory confirms the shared factory auto-merges while
`hir_ty/src/diagnostics.rs` is a semantic port
(`/media/tony/WesternDigitalNvmeSsd/Code/baml-worktrees/green/PORT_INVENTORY.md:196-205`,
`/media/tony/WesternDigitalNvmeSsd/Code/baml-worktrees/green/PORT_INVENTORY.md:284-288`).
There is no cutover-driven E-code renumbering to emulate.

## 6. Test infrastructure

### What exists after cutover

The checked-in design describes the original differential strategy: every
type-spec fixture had caret assertions and an inference dump; TIR was run
separately over the same corpus until cutover
(`baml_language/crates/baml_compiler2_hir_ty/README.md:360-387`).
After TIR deletion the source has some stale “differential” naming, but
`DifferentialOutcome` now contains only the `hir_ty` outcome and the dump
renderer only renders that engine
(`baml_language/crates/baml_tests/src/type_spec/harness.rs:100-130`).
Do not promise a live TIR-vs-hir differential runner.

Current fixtures are under
`baml_language/crates/baml_tests/src/type_spec/fixtures/` and snapshots under
`baml_language/crates/baml_tests/src/type_spec/snapshots/`. The fixture registry
separates conforming from pending cases
(`baml_language/crates/baml_tests/src/type_spec/fixtures.rs:60-110`).
The harness infers per body owner and renders node/throws tables
(`baml_language/crates/baml_tests/src/type_spec/harness.rs:491-560`).
Additional nets are:

- `type_spec/tables.rs` for durable side-table invariants, especially member
  and call resolution;
- `type_spec/sweep.rs` for a whole-`baml_src` panic/error census after TIR
  retirement (`baml_language/crates/baml_tests/src/type_spec/sweep.rs:1-6`,
  `:48-128`);
- compiler/MIR/codegen snapshots and bytecode-format snapshots;
- shared normalizer and interned/plain parity unit tests;
- behavior-level Rust scenario suites in `baml_tests/tests`.

The green inventory's snapshot classification is authoritative for merge
cleanup: delete the 28 obsolete `04_tir` snapshots rather than regenerating
them, and regenerate/inspect only the 32 surviving expectations after semantic
completion
(`/media/tony/WesternDigitalNvmeSsd/Code/baml-worktrees/green/PORT_INVENTORY.md:242-280`,
`/media/tony/WesternDigitalNvmeSsd/Code/baml-worktrees/green/PORT_INVENTORY.md:309-347`).

### Required port coverage

1. **Shared type algebra:** all nine sealed kind classes subtype `type` through
   plain and interned entry points; arbitrary user classes do not; head-category
   disjointness agrees; unions canonicalize correctly.
2. **Mint identity:** equivalent spelling/alias/union order produces the same
   digest; repeated and fresh-intern runs agree; distinct canonical types have
   representative non-equality checks; no test derives identity from intern
   `Hash`.
3. **HIR bridge:** static and runtime type slots retain order and operand IDs;
   `TypeBinding` and unreflect patterns are traversed; default/forward-ref and
   throws reachability include operands exactly once and exclude nested
   lambdas.
4. **Inference:** operand `<: type`, bound-derived static occurrence type,
   targeted missing-`unreflect` error, only dependent checks deferred, all-
   static conjunctive bounds retained, plan writes preserved through optional
   calls, `Session.eval` exception, LLM helper seed, streaming rejection,
   static-only `from_json`, sealed construction rejection, and no live
   `InferVar` in results.
5. **Wildcards:** port B-230/B-247 cases for `Future<int, _>`, nested
   containers, open throws, and E0147. Review and deliberately unignore each
   old spec case rather than bulk-copying its ignored status.
6. **Mounted packages:** source-less alias/enum/interface lookup, inherited
   associated names/defaults, generic arity/bounds, field substitution, bound
   and UFCS methods, interface-slot dispatch, unsupported builtin, and
   ordinary-name shadowing.
7. **Pattern usefulness:** multiple unreflect patterns remain individually
   possible but do not make a match exhaustive or duplicate each other; no
   bindings/rest misuse; operand throws participate.
8. **Provider/MIR:** table tests assert per-slot runtime/static plans and
   loc-free targets; MIR snapshots assert `BindType`, hidden type operands,
   runtime call flag, and `RuntimeIsType`. Ensure provider conversion cannot
   erase mixed explicit slots.
9. **Diagnostics:** exact shared factory ID/message tests plus checker and LSP
   location tests.
10. **End to end:** stdlib scale gate, mounted package compile/extract/call,
    VM execution, bytecode display, and runtime type identity.

### Existing engine-agnostic BEP suites

Once the full compiler bridge lands, replay the BEP branch's behavior suites
unchanged in intent: `builder_witness_parity`, `compiled_package_identity`,
`constructor_consistency`, `mounted_package_calls`,
`mounted_package_parity`, `runtime_builders_and_pending_types`,
`runtime_classes_and_composites`, `runtime_diagnostic_consistency`,
`runtime_interface_witnesses`, `runtime_package_api_consistency`,
`runtime_package_compile`, `runtime_package_extraction`,
`runtime_package_render_identity`, `runtime_package_session`,
`runtime_type_bindings`, `to_baml_witness_roundtrip`, `type_kinds`, and
`type_value_equality`. Also reconcile the changed `reflect_call_any` and
interface scenarios. Clean canary currently contains only a subset (notably
`reflect_call_any`), so absence is expected before replay.

These suites are runtime/compiler behavior contracts, not TIR implementation
tests. In contrast, relevant tests under the deleted
`baml_tests/src/compiler2_tir/inference.rs` must be rewritten into type-spec/
table/diagnostic tests
(`/media/tony/WesternDigitalNvmeSsd/Code/baml-worktrees/green/PORT_INVENTORY.md:282-305`).

## 7. Risk register

| Rank | Risk | Why it is non-mechanical | Required decision or mitigation |
|---:|---|---|---|
| 1 — Critical | Runtime type values are opaque but `hir_ty` expects all value inference variables solved before results. | Treating `unreflect(e)` as an ordinary `InferVar` either leaks a variable, invents a static type, or raises E0147. TIR's early binding-map seed does not translate to the fixpoint solver. | Represent runtime provenance separately; use a bound/top static occurrence type and an explicit deferred runtime gate in `CallPlan`. Assert no infer-variable leak. |
| 2 — Critical | Current `BodyTypeRefs` and `CallPlan` cannot represent mixed static/runtime slots. | A call-wide `explicit` bit and bare `TypeRefId` list lose operand identity and per-slot policy. Provider conversion currently discards explicit plan args because MIR re-lowers syntax. | Agree the slot enum and make the solved plan authoritative end-to-end before implementing call semantics. |
| 3 — Critical | Mounted call identity is source-location-free, while all current resolutions use HIR locations. | Fake locations break serialization/incrementality and cannot identify dependency-only methods/interfaces. | Design a serializable symbolic target and thread it through package interface, inference, provider, MIR, emit, and project DB. |
| 4 — High | Scoped `type T = unreflect(v)` has no `hir_ty` equivalent. | `LowerCtx`'s frame is fixed; block-local rigid generics must not escape into result/local types and both infer/check exits must unwind them. | Choose a scoped overlay API with RAII/explicit checkpoints; test nested blocks, branches, early divergence, lambdas, and defaults. |
| 5 — High | Package interface lacks interface rows and semantic metadata. | Mounted associated types, defaults, `requires` closure, generic bounds, fields, dispatch slots, and linkability cannot be recovered from current rows. | Version/extend the exported schema first; add round-trip tests before inference consumes it. |
| 6 — High | Runtime-dependent bound deferral can become unsound or over-broad. | Skipping all bounds admits invalid static programs; registering all current obligations rejects valid runtime-dependent calls. Multiple conjunctive bounds may mix both categories. | Compute dependency on runtime slots structurally per expected type/bound and defer only affected checks. VM must enforce the same recorded gate. |
| 7 — High | B-230/B-247 reference tests are partly ignored and BEP TIR still rejects holes. | “Port old behavior” is ambiguous because the branch contains desired tests ahead of its implementation. | Treat `TYPE_SYSTEM.md` and current `hir_ty` as authority; review every ignored test and record intentional expectation decisions. |
| 8 — High | Cutover intentionally changed interface semantics. | Bounds are conjunctive end-to-end rather than TIR's single-bound asymmetry; the one-`Self` existential call gate is stricter; associated defaults and `Self`/`requires` realization fix sound-program failures (`baml_language/crates/baml_compiler2_hir_ty/README.md:46-71`, `:84-131`). | BEP mounted interfaces must match `hir_ty`, not reproduce TIR. Escalate any runtime API depending on old permissiveness. |
| 9 — High | Inference ordering changed. | TIR mutated plain bindings and checked bounds immediately; `hir_ty` accumulates lower/upper constraints and obligations, resolves to fixpoint, defaults only effects, and fails unresolved value vars. LLM seeding and runtime slots can change which evidence wins. | Seed facts before dependent checks, use table snapshots/probes where candidate choice needs them, and add argument-order/permutation tests. |
| 10 — Medium-high | Mixed explicit calls conflict with the provider's present re-lowering convention. | Current conversion emits no plan type args when `explicit` because MIR lowers written types itself (`baml_language/crates/baml_compiler2_mir/src/inference_provider.rs:307-326`). A runtime expression is not a type and cannot follow that path. | Move all type-argument lowering behind the provider plan, or retain the shortcut only behind an asserted all-static branch. |
| 11 — Medium-high | Hidden operands can disappear from throws/default/forward-reference walks. | BEP had to patch multiple old walkers. The new architecture centralizes traversal, but only if AST/HIR child edges are correct. | Add traversal unit tests first and make all consumers use canonical reachability rather than bespoke scans. |
| 12 — Medium | Stable Mint identity can accidentally use interner identity. | Intern `Eq/Hash` is deliberately process-local; a pointer-derived digest may pass one-process tests and fail cache/architecture parity. | Keep canonical plain `NormalTy` + fixed FNV algorithm and cross-construction tests. |
| 13 — Medium | Diagnostics can drift even when semantics work. | New inference delays rendering; copying old builder strings into `hir_ty` can bypass shared code/message parity or attach the wrong node. | Replay the shared factory, emit typed pending variants, and test factory, checker, and LSP layers. No renumbering is needed. |
| 14 — Medium | Special intrinsic rules may leak to ordinary calls. | Extraction contracts, Session result top, LLM schema seeding, streaming rejection, and static-only `from_json` each have deliberately narrow identity conditions. | Centralize intrinsic recognition on resolved callable identity, never spelling alone; add shadowing and near-name negatives. |
| 15 — Medium | Pattern usefulness can claim false exhaustiveness. | A runtime-computed type is not a structural static witness. Reusing a normal type pattern can make two dynamic patterns overlap or cover the universe incorrectly. | Use unique rigid, possible-but-non-covering DPat nodes and explicit exhaustiveness tests; remember provider polarity inversion. |
| 16 — Medium | Obsolete TIR snapshots can be accidentally resurrected. | The green merge contains conflicts for deleted `04_tir` artifacts while surviving MIR/codegen snapshots genuinely need regeneration. | Follow INV classification: delete obsolete TIR output, regenerate only surviving snapshots after semantic gates pass, inspect every delta. |
| 17 — Low/knowledge gap | Referenced design documents are unavailable. | The PR's `doc-inference.md` and brief's reconciliation plan cannot be audited. | Use checked-in README/spec/source as authority and ask the `hir_ty` author the three schema/ownership/scope questions from `/media/tony/WesternDigitalNvmeSsd/Code/baml-worktrees/green/PORT_INVENTORY.md:349-359`. |

The most important intentional-cutover rule is stated by the new engine itself:
`TYPE_SYSTEM.md` wins over TIR, and TIR disagreements were encoded as ignored
spec tests (`baml_language/crates/baml_compiler2_hir_ty/README.md:157-161`).
A migration that makes all old snapshots green by restoring TIR behavior can
therefore be wrong.

## 8. Suggested port order and checkpoints

### Slice 0 — Freeze semantic contracts

- Turn the 35 inventory facts into issue/checklist items.
- Resolve with the `hir_ty` owner: runtime slot representation, loc-free target
  ownership, and scoped generic overlay.
- Pin current `hir_ty` hole/open-throws behavior before other BEP changes.

Checkpoint: type-spec hole/throws tests pass unchanged; schema decisions are
documented; no production behavior changed.

### Slice 1 — Shared algebra and diagnostics foundation

- Replay `type_kind.rs`, `NormalTy` head/subtype arms, `canonical_digest`, shared
  diagnostic IDs, and `runtime_type.rs`.
- Add plain/interned subtype parity and Mint determinism tests.

Checkpoint: shared crate unit tests prove sealed `kind <: type`, user classes
negative, and canonical digest stability; no `hir_ty` fork of these rules.

### Slice 2 — Front-end and canonical HIR edges

- Replay parser/AST/formatter `TypeArg`, `TypeBinding`, and unreflect-pattern
  changes.
- Extend `BodyTypeRefs` with ordered static/runtime slots and operand identity.
- Extend canonical AST/HIR traversal and source maps for hidden operands.

Checkpoint: parser/lowering/traversal tests prove syntax, shadowing/contextual
parsing, source mapping, order, and exactly-once reachability. Throws/default
fact tests see hidden operands before call inference is implemented.

### Slice 3 — Package-interface schema and resolution

- Add exported interfaces, associated metadata, bounds, fields/methods,
  builtin linkability, and loc-free symbolic targets.
- Route type and value resolution through own-or-exported results.
- Add shadow-preserving `baml.reflect/type/json` fallback and foreign type/pin
  validation.

Checkpoint: serialized package-interface round trips; source-less lookup and
negative arity/pin/shadowing tests pass without MIR/VM execution.

### Slice 4 — Interned call inference and durable plans

- Implement per-slot static/runtime instantiation, operand `<: type`,
  bound/top static occurrence types, targeted diagnostics, and precise
  runtime-dependent deferral.
- Make `CallPlan` a single enriched record; include loc-free target, owner split,
  slots, deferred gates, bindings, runtime ID.
- Add extraction contract, Session exception, LLM schema seed, streaming gate,
  static-only `from_json`, optional-call parity, and sealed constructor check.

Checkpoint: type-spec and table tests cover B-01..B-20, every final result is
ground, all-static generic-bound regressions remain green, and no MIR changes
are required to inspect a complete call plan.

### Slice 5 — Scoped bindings, patterns, and effects

- Add the block-scoped rigid generic overlay and escape erasure.
- Add unreflect-pattern inference/usefulness behavior.
- Verify hidden operand effects in callable-throws fixpoints, catch contexts,
  defaults, and nested lambdas.

Checkpoint: nested scope/branch/lambda tests show no synthetic parameter leaks;
unreflect patterns are non-covering; operand throws propagate exactly once.

### Slice 6 — Interface witnesses and mounted calls

- Feed exported interface facts into `Facts`/`impls`/`interfaces`/
  `method_resolution`.
- Implement mounted fields, bound/unbound methods, associated defaults/
  `requires` closure, interface-slot dispatch, and unsupported-call diagnostics.
- Apply `hir_ty`'s intentional conjunctive-bound, one-`Self`, projection, and
  default semantics.

Checkpoint: local and mounted versions of the same witness/member scenarios
produce equivalent inferred types and symbolic targets; deliberate new-engine
semantic differences have explicit tests.

### Slice 7 — Provider, MIR, emit, and project wiring

- Expand provider-side plans/resolutions and convert interned → plain once.
- Replay `BindType`, runtime type operands/call flag, `RuntimeIsType`, loc-free
  targets, and emit/project DB integration.
- Remove the explicit-type re-lowering shortcut for mixed plans.

Checkpoint: MIR snapshots prove the exact runtime operations and target links;
bytecode/link tests pass for local and mounted packages; no AST re-resolution
occurs in MIR.

### Slice 8 — Behavioral replay and wildcard reconciliation

- Replay engine-agnostic BEP suites.
- Port old TIR semantic tests into type-spec/table tests.
- Review/unignore B-230/B-247 cases individually against the spec.
- Run stdlib and whole-corpus sweep.

Checkpoint: runtime type identity, type binding, construction, interface
witness, mounted-call, render/extraction, and diagnostics suites pass; the
`hir_ty` sweep has no new panic/error census regressions.

### Slice 9 — Snapshot reconciliation and final cutover audit

- Delete obsolete `04_tir` expectations.
- Regenerate only surviving MIR/diagnostic/codegen/bytecode snapshots identified
  by INV.
- Audit each delta against the call plan, target, diagnostic, or intentional
  `hir_ty` semantic change which caused it.

Checkpoint: compiler workspace checks pass, all scenario and snapshot suites are
green, the 35-fact checklist is closed, no interned pointer identity crosses a
stable boundary, and no TIR-era resolver/builder abstraction has been
reintroduced.

The safe critical path is therefore:

`shared semantics → HIR identity → package schema → inference plan → scoped/pattern facts → mounted witnesses → MIR/runtime → behavior/snapshots`.

That ordering makes every intermediate failure attributable to one boundary and
prevents MIR or VM code from compensating for missing static facts.
