> **Provenance:** generated and adversarially verified by the multi-agent semantic-survey workflow at commit `b9c5d7c0e` (the pre-semantic-pass state of the crate), then used as the frozen contract for the 16 semantic refactors in this PR. Representation-level details (S-invariants, struct layouts like `TyAttr`) have evolved as those refactors landed; the behavioral F-invariants still hold and are enforced by the insta snapshot suite.

# TIR semantic model (`baml_compiler2_tir`)

Shared brain for the de-slop / rewrite of the TIR phase. Synthesized from per-file, type-cluster, and data-flow sub-models, with contradictions resolved against the code. Crate: `crates/baml_compiler2_tir` (~28,194 lines across 18 source files).

Goal context: TIR is **slow AND too big**. Upcoming refactors must **reduce lines and structural complexity** so the crate is tractable for a future rewrite. Performance preservation is NOT a constraint. **Behavioral equivalence IS the hard constraint**: 131 in-crate unit tests + ~1,234 insta snapshots (compiler2_tir, compiler2_mir, diagnostic_errors, compiles, baml_src) + 4,991 workspace tests must stay green with ZERO snapshot drift. Diagnostics text, ordering, and rendered output are all observable behavior.

---

## 1. Architecture overview

TIR is the **type-inference IR** phase of the compiler2 pipeline. It is the **first phase that produces a resolved `Ty`** — everything upstream (AST → HIR → PPIR) hands TIR un-lowered structures (`TypeExpr`, item trees, semantic indices), so **TIR owns all `TypeExpr → Ty` lowering**. Downstream, MIR (`baml_compiler2_mir`, `Db: tir::Db`) is the sole *structural* consumer; the LSP (`baml_lsp2_actions`) consumes TIR queries directly for editor features; codegen/emit consume the cross-package interface + throw sets.

What it computes, per scope/item:
- **Bidirectional type inference** over an `ExprBody` arena (function body, lambda body, or parameter-default body).
- **Member / path / method resolution** (incl. BEP-044 nominal interfaces).
- **Generic instantiation** (bind/substitute type vars).
- **Pattern lowering + exhaustiveness/refutability** (a self-contained port of rustc's `rustc_pattern_analysis`).
- **Control-flow narrowing** (null / `is` / truthiness, plus early-return narrowing).
- **Throws/effects analysis** (two parallel engines — see §3, §6).
- **Type-level structural normalization + subtyping** (equirecursive, Mu-based).
- **Cycle diagnostics** (invalid alias cycles, unconstructable class cycles via Tarjan SCC).

### Passes / engines (logical, not file-mapped)
1. **Type resolution (Pass 2 substrate):** `lower_type_expr` → `Ty`; `resolve_class_fields` / `resolve_type_alias` (per-item Salsa queries); `package_interface` (cross-package typed summary).
2. **Per-scope inference:** the `infer_scope_types` Salsa query drives a `TypeInferenceBuilder` (`builder.rs`, 12,361 lines / ~252 methods — the structural center of gravity).
3. **Subtyping/normalization:** `normalize.rs` (`StructuralTy`, equirecursive `is_subtype_of`). `builder::is_subtype` layers BEP-044 nominal rules on top.
4. **Exhaustiveness:** `exhaustiveness.rs` matrix algorithm, with `TypeInferenceBuilder` as the `PatCtx` type oracle.
5. **Throws:** `throw_inference.rs` (HIR-level, whole-package call-graph fixpoint via `analysis.rs`) + `callable.rs` (TIR-level per-function salsa query) + `throws_analysis.rs` (the shared structural walk).

### Entry points (Salsa queries)
- `infer_scope_types(db, ScopeId) -> &ScopeInference` — **THE** main query. **Per-scope, not per-function** (each function body, lambda body, top-level let is its own cached scope → editing a lambda re-runs only that scope). `cycle_initial` = empty.
- `resolve_class_fields(db, ClassLoc)`, `resolve_type_alias(db, TypeAliasLoc)` — per-item type resolution.
- `callable_throws(db, FunctionLoc) -> &Ty`; `function_throw_sets(db, PackageId) -> &FunctionThrowSets`.
- `package_interface(db, PackageId)`, `package_resolution_context(db, PackageId)`, `package_implements_registry(db, PackageId)`.
- Non-query helpers: `resolve_name_at` (on-demand LSP name resolution), `render_scope_diagnostics` (joins cached inference with a source map for display).

`lib.rs` is a 44-line wiring file: declares **18** modules (sub-model said 20 — corrected) and an empty `pub trait Db: baml_compiler2_ppir::Db {}` marker. Supertrait chain: `hir::Db ← ppir::Db ← tir::Db ← mir::Db`.

---

## 2. Data-flow map

### Cross-phase inputs (TIR receives NO pre-lowered types)
- **AST (`baml_compiler2_ast`):** `ExprBody` arenas (`exprs`/`stmts`/`patterns` keyed by `ExprId`/`StmtId`/`PatId`, `root_expr`) + `AstSourceMap` (id → `TextRange`). Raw `TypeExpr` is the lowering input. All TIR maps are keyed by arena IDs.
- **HIR (`baml_compiler2_hir`):** loc newtypes (`ClassLoc`/`EnumLoc`/`FunctionLoc`/`TypeAliasLoc`/`LetLoc`/`InterfaceLoc`), `PackageId`/`PackageItems` (`namespaces` → {types, values} → `Definition`), `file_package::PackageInfo {package, namespace_path}`, `ItemTree` (raw `TypeExpr` fields, methods, generic_params, implements, implements_for), `FunctionBody` (Expr/Builtin/Missing), `ElaboratedFunctionSignature` (params/return/throws as `TypeExpr`, user_generic_params, synthetic_effect_params).
- **PPIR (`baml_compiler2_ppir`):** the concrete Salsa accessors — `file_semantic_index` (`scopes[]`/`scope_ids[]`/`scope_bindings[]`, co-indexed by `FileScopeId`), `file_item_tree`, `package_items`, `function_body(+source_map)`, `elaborated_function_signature(+source_map)`, `function_parameter_defaults`.

### Queries: consumes → produces → callers

| Query | Consumes | Produces | Primary callers |
|---|---|---|---|
| `infer_scope_types` | semantic index, item tree, file_package, function/let body(+source map), elaborated sig, PRC, `resolve_type_alias`, recursive self on ancestors, `lower_type_expr_in_ns` | `ScopeInference` (14 fields; see §3) | MIR `lower.rs` (sole structural consumer, `merge_scope`), LSP (hover/tokens/def/usages/completions/annotations), `render_scope_diagnostics`, baml_tests |
| `resolve_class_fields` | item tree class, file_package, package_items, `lower_type_expr_in_ns` per field | `Arc<ResolvedClassFields>{fields:Vec<(Name,Ty,attrs)>, diagnostics}` | `class_actual_fields_ordered`, `collect_class_fields`, LSP |
| `resolve_type_alias` | item tree alias, namespace, package_items, `lower_type_expr_in_ns` | `Arc<ResolvedTypeAlias>{ty, diagnostics}` | `collect_type_aliases`, LSP |
| `callable_throws` | elaborated sig, function_body, file_package, item tree, (for Expr bodies) `infer_scope_types` + `collect_escaping_throws` | `Ty` (Never/single/Union escaping throws); `cycle_initial` recovery | `package_interface` (per fn/method), `builder.rs` (`Ty::Function` construction) |
| `function_throw_sets` | package_items, per-fn sig+body (direct throws + call edges), package_dependencies, dep `package_interface`, `AnalysisGraph` fixpoint | `FunctionThrowSets{transitive: BTreeMap<Name, BTreeSet<Ty>>}` | `package_interface.throw_sets`, `lookup_named_throw_summary`, emit |
| `package_interface` | package_items, item tree, `lower_type_expr_in_ns` (+ Self subst), `callable_throws`, `function_throw_sets` | `PackageInterface{types, functions, throw_sets}` | PRC (clones per dep), throw_inference, LSP |
| `package_resolution_context` | `package_items().clone()`, package_dependencies, `package_interface(dep).clone()` | `PRC{own_items, dep_interfaces, own_package_name}` (owned, not borrowed) | `infer_scope_types`, `resolve_name_at`, callable, builder |
| `package_implements_registry` | package_items, item tree (`implements`), all-files filtered (`implements_for`), `resolve_path_to_interface`, `lower_type_expr_in_ns`, interface `requires` walk | `ImplementsRegistry{interface_impl_rules, …_index, class_implements, interface_requires}` | builder (subtype), MIR (dispatch), emit (reflection), LSP |

### Cross-phase outputs
- **To MIR:** the whole `Ty` vocabulary (imported as `Tir2Ty`); `ScopeInference` flattened per scope into `(MetadataScope, ExprId/PatId)`-keyed maps (drives expr lowering, `Place::Field` chains, method-call self-binding, positional call plans, function-coercion adapters); `ImplementsRegistry` + interface helpers; reuse of `normalize`/`generics`/`lower_type_expr` utilities directly.
- **To LSP:** `infer_scope_types` + `render_scope_diagnostics` + `resolve_*` + `resolve_name_at`. `MemberResolution` → go-to-def/usages; expr/binding/param types → hover/inlay/tokens/completions.
- **Diagnostics:** `TypeCheckDiagnostics` (in `ScopeInference.extra`), `ResolvedClassFields/TypeAlias.diagnostics`, rendered to `RenderedTirDiagnostic`. Text/ordering/spans are snapshot-observable.
- **Cross-package:** `PackageInterface` (`ExportedType`/`ExportedFunction` with baked `callable_throws`) + `FunctionThrowSets`.

---

## 3. Type clusters

### Cluster A — `Ty` model (the resolved-type currency) — `ty.rs`
- **`Ty`** (~24–26 variants, every variant carries a `TyAttr`): nominal (Class/Interface/Enum/EnumVariant/TypeAlias, keyed by `QualifiedTypeName`), structural (Primitive/List/Map/Union/Optional/Function/Future), TIR-only inference states (`Literal`+`Freshness`, `EvolvingList`, `EvolvingMap`, `TypeVar`), sentinels (Never=bottom, Void=unit, BuiltinUnknown=top, Unknown=error-recovery, Error, RustType, Type).
- **`QualifiedTypeName`** = (private pkg/namespace/name) + **public `generic_params: Vec<Name>`** (declared, unsubstituted). **`FunctionParamTy`** {name?, ty, mode}, **`FunctionParamMode`** Required|Optional, **`PrimitiveType`** (11 scalars/media), **`Freshness`** Fresh|Regular (TS-style; subtyping-irrelevant), **`TyAttr`** (re-export of `baml_base::attr::TyAttr` — @sap.* streaming flags, **always `default()` inside TIR**).
- **`TyRenderStrategy`** trait + `CanonicalTyRender` — the single renderer (`render_with`); two more implementors in the LSP. ALL diagnostic/hover text funnels through it (snapshot-load-bearing).
- **Lifecycle:** born in `infer_scope_types`; stored immutably in `ScopeInference` maps; never mutated in place (transforms `widen_fresh`/`make_evolving`/`substitute_ty`/`with_attr` return fresh values). Not Salsa-interned → `ScopeInference` is recomputed (not patched) on input change. Terminal sinks erase TIR-only variants: Interface→Class, Evolving*→frozen container, Literal drops Freshness, TypeVar→Void/Unknown, Never→Void.
- **Redundancy:** a **4-enum chain** carries essentially the same information — `Ty` ↔ `StructuralTy` (normalize.rs) ↔ `baml_type::Ty` (MIR, via ~250-line `convert_tir2_ty`) ↔ `cg::Ty` (codegen, via parallel ~120-line `convert_tir_to_codegen_ty`). `TyAttr` on every TIR variant is inert plumbing (read once, at the MIR boundary). `EvolvingList/Map` could be a flag on `List/Map` (mirroring `Freshness` on `Literal`). `generic_params` (Vec<Name>) vs Class/Interface `Vec<Ty>` args = two generic-state carriers on the same nominal type.

### Cluster B — Per-scope builder state — `builder.rs`
- **`TypeInferenceBuilder`** (~32 fields, ~252 methods): owns `InferContext` (diag sink), the resolution context (`res_ctx`/`package_items`/`scope`/`ns_context`/`aliases`), configured scope facts (`declared_return_ty`/`generic_params`/`generic_param_bounds`/`implements_block_interface`/`is_auto_derived_body`/`body_source_map`), and ~18 accumulator maps drained by `finish()` (a **14-tuple** contract with `inference.rs`).
- **`InferContext`** = {db, scope, `RefCell<TypeCheckDiagnostics>`, `Cell<bool>` suppress flag}.
- **Save/restore machinery:** `SavedInferenceState` (12 maps `mem::take`'d for sub-body recursion) + `ScopedLocalsSnapshot` (locals clone + 2 log watermarks) + append-only logs `ScopedLocalDeclaration` / `ScopedAssignment` (the "Slack rules": propagate outer-binding writes, drop inner-shadow writes, keyed by `PatId` identity).
- **Transient bundles:** `CallContext`/`CallCheckRequest`/`OptionalCallContext`/`CheckedCallInner`/`OptionalBaseInfo` (call pipeline), `BuiltinResolution`, `PatternExpectedTy`/`NaturalKind` (pattern typing), `IrrefutablePatternContext`/`IrrefutableContextKind`, `InterfaceMethodLowerCtx`/`InterfaceFieldMatch`, plus throws helpers `ThrowPatternMatches`/`PatternMatchStrength`/`CallbackThrowProvenance`.
- **Redundancy:** `builder.scope` DUPLICATES `InferContext.scope`; `is_auto_derived_body` DUPLICATES `suppress_member_lookup_errors`. The per-body map set is hand-written in 4 places (builder fields + `SavedInferenceState` + `DefaultParameterInference` + `ScopeInference`). The scoped-locals snapshot stores a full locals clone AND delta logs (overlapping rollback info).

### Cluster C — Inference entry/result types — `inference.rs` + `resolve.rs`
- **`ScopeInference`** (14 fields): `expressions` (ExprId→Ty), `pattern_types` (PatId→Ty, **dual role**: Bind-variable type AND Type/Class pattern runtime-test type), `resolutions`, `path_root_types`/`path_segment_types`/`path_member_resolutions` (3 parallel views of one multi-segment path), `catch_residual_throws`, `exhaustive_matches`, `call_plans`, `function_coercions`, `nested_lambda_types` (intra-query scratch, FileScopeId-keyed), `param_types` (Vec, LSP-only), `parameter_defaults` (`DefaultParameterInference`), `extra` (boxed diagnostics, only when non-empty).
- **`DefaultParameterInference`** = a near-exact clone of 10 ScopeInference fields (separate AST arena for default-value exprs). MIR re-merges it under `MetadataScope::ParameterDefault` vs `Body`.
- **`CallPlan`/`ParamBinding`** (Provided/OmittedDefault), **`FunctionCoercion`**, **`MemberResolution`** (Field/Variant/Free/BoundMethod/UnboundMethod/InterfaceDefaultMethod), **`ResolvedClassFields`**/**`ResolvedTypeAlias`** (Arc, shared diagnostics shape), **`ResolvedName`** (Local/Item/Builtin/Unknown — re-derived on demand, no stored map).
- **Redundancy:** `DefaultParameterInference` ⇄ ScopeInference (the largest structural dup; the split is undone by MIR). The 3 `path_*` maps overlap (`path_segment_types[0]` ≡ `path_root_types`). `call_plans` keyed by callee ExprId but throws looks up by arg set → fuzzy linear reverse scan.

### Cluster D — Diagnostics / errors — `infer_context.rs`
- **`TirTypeError`** (66 variants, span-free for Salsa cacheability) + giant `Display` (~420 LOC). **`TirDiagnostic`** = error + `DiagnosticSeverity` + `DiagnosticLocation` (arena IDs) + `Vec<RelatedNote>`. **`TypeCheckDiagnostics`** = newtype `Vec<TirDiagnostic>` (append-only, no dedup/sort). **`RenderedTirDiagnostic`**/**`RenderedRelatedInformation`** = display-ready (arena IDs → TextRange).
- **Two diagnostic channels:** rich `TirDiagnostic` (body inference, via `InferContext`) AND a bare `Vec<TirTypeError>` (type-annotation lowering in `lower_type_expr`/`generics`/`interfaces`/`package_interface`/MIR — 49 throwaway `let mut diags = Vec::new()` sites, many discarded). The two carry the same `TirTypeError` values inconsistently.
- **Redundancy:** `Display` ⇄ LSP `source_aware_tir_type_error_message` duplicate the message templates for ~15 type-bearing variants (and have already drifted, e.g. MissingReturn / NonExhaustiveMatch wording). Two exhaustive cross-crate matches over all 66 variants (Display + `tir_type_error_to_diagnostic_id`). `RenderedTirDiagnostic` keeps `error` AND `message` (re-render). Related-note machinery exercised by only 3 sites.

### Cluster E — Normalization / subtyping — `normalize.rs`
- **`StructuralTy`** (private, ~3rd parallel Ty enum): aliases resolved, recursion explicit via `Mu`/`TyVar`, attrs/freshness stripped, primitives flattened, EvolvingList/Map→List/Map, RustType/Future→Unknown. **`StructuralFunctionParam`** (twin of `FunctionParamTy`). **`ClassCycleInfo`** (public; `cycle_path` is redundant with `members`). **`GraphResult`**, **`NodeState`**, **`Tarjan`** (deterministic SCC over `QualifiedTypeName`).
- **Lifecycle:** built fresh per `is_subtype_of`/`is_same_normalized_type` call; `find_recursive_aliases` re-run inside every comparison; **never memoized** (primary slowness cause). All public except `ClassCycleInfo` are private — internal IR rewrite has zero cross-crate blast radius.
- **Redundancy:** TWO `Tarjan`+`NodeState` impls (here vs `analysis.rs`). `StructuralTy` is ~80% a copy of `Ty`. THREE structural-subtyping impls cross-crate (`StructuralTy`, `baml_type::Ty`, `sys_jinja_types::Type`). `has_cycle`/`ty_has_cycle` DFS + Tarjan = two cycle detectors over the same alias graph; 4 separate recursive Ty walks (`normalize_impl`, `ty_has_cycle`, `extract_type_alias_deps`, `extract_required_class_deps`).

### Cluster F — Exhaustiveness / pattern analysis — `exhaustiveness.rs` + `pattern_lowering.rs`
- **`Ctor`** (9 variants: Single/Slice/Class/Interface/UnionMember/Or/Wildcard/NonExhaustive/Missing — identity via `CtorIdentity` string, NOT structural Ty eq; Class compares qtn only). **`SliceShape`** (Fixed/Variable). **`DPat`** {ctor, fields, ty} (algorithm input). **`WitnessPat`** (algorithm output, **structurally identical to DPat**). **`Row`/`Matrix`/`ArmId`** (private grid). **`UsefulnessReport`** {missing, unreachable_arms}. **`WitnessStack`/`WitnessMatrix`** (witness accumulators). **`PatternResult`** {dpat, required_ty (up-flow), matched_ty (down-flow), bindings} + **`PatternBinding`** {name, pat_id, ty}.
- **Lifecycle:** fully internal to the crate. Only export crossing the crate boundary is `pattern_types: FxHashMap<PatId, Ty>` (read by MIR/LSP) + `UsefulnessReport` for diagnostics. Two-phase per match/let/for: lowering (builder walk → `PatternResult`) then usefulness (matrix, all stack-local, dropped on return). No caching.
- **Redundancy:** `DPat` ≅ `WitnessPat` (unify into one `Pat`; only Display forks). `WitnessStack`/`WitnessMatrix` are thin Vec newtypes. `Ctor::Single/Interface/UnionMember` share copy-paste identity/Hash. `Ctor::Class` stores a Vec<Ty> ignored by identity. `CtorIdentity(String)` exists only because `Ty`'s PartialEq is span/freshness-sensitive. `TestingCtx` (in tests) is a second `PatCtx` impl with subtly different normalization.

### Cluster G — Throws / effects — `throw_inference.rs` + `callable.rs` + `throws_analysis.rs` + `analysis.rs`
- **`FunctionThrowSets`** {transitive: BTreeMap<Name(dotted), BTreeSet<Ty>>} (HIR-level output). **`ThrowsAnalysisContext`** trait (the shared walk seam; 3 impls). **`CallableThrowsAnalysis`** (ScopeInference-backed), **`BuilderThrowsAnalysis`** (builder-backed), **`CatchBaseThrowsAnalysis`** (3-flag decorator). **`AnalysisGraph`/`AnalysisResult`/`Tarjan`/`NodeState`** (generic 2-pass framework, single instantiation).
- **Lifecycle:** `FunctionThrowSets` and `callable_throws` are the two durable (Salsa-cached) outputs; everything else is ephemeral scaffolding. `catch_residual_throws` is the one cross-phase state (computed via `CatchBaseThrowsAnalysis`, stored on builder, persisted into `ScopeInference`, reused by the `callable_throws` path).
- **Redundancy:** **TWO PARALLEL THROW ENGINES** computing the same per-function escaping-throw notion (HIR-level `function_throw_sets` vs TIR-level `callable_throws`), plus a 3rd inline impl in builder. Key-string construction duplicated 3× (`throw_set_key`/`callable_key`/`dotted_method_key`). `analysis.rs` framework = 211 lines for one caller. The 3 trait toggles encode one mode bit. `join_throw_facts`/`facts_to_ty`/`ty_from_concrete_facts` = 3 near-identical "BTreeSet→Ty" joiners.

### Cluster H — Package interface & interface-impl — `package_interface.rs` + `interfaces.rs` + `narrowing.rs`
- **`PackageInterface`** {types, functions, throw_sets}, **`ExportedType`** (Class/Enum/TypeAlias), **`ExportedFunction`**, **`ResolvedSource`** (Item/Builtin), **`ResolvedMethod`** (test-only wrapper), **`PackageResolutionContext`** (owned own_items + dep_interfaces).
- **`ImplementsRegistry`** {interface_impl_rules (canonical), `…_index` (derived accel), class_implements (derived view), interface_requires (closure)}, **`InterfaceImplRule`** (unified impl shape), **`InterfaceImplRuleIndex`** (100% derived from rules), **`InterfaceImplInstantiation`**, **`InterfaceImplOrigin`**, **`ResolvedInterface`**.
- **`Narrowing`** {name, then_type, else_type} (loosely part of the cluster; self-contained, feeds `builder.locals`).
- **Redundancy:** `InterfaceImplRuleIndex` is a pure perf index → **deletable** in favor of a linear scan (perf is not a constraint). `class_implements` is a derived projection of `interface_impl_rules`. `PackageInterface.throw_sets` clones `function_throw_sets` verbatim. `ResolvedMethod`/`PRC::lookup_class_method`/`lookup_own_class_method` are **test-only** (verified: only caller is `baml_tests/src/compiler2_tir/inference.rs:570`; production builder uses its own `TypeInferenceBuilder::lookup_class_method` at builder.rs:8916). `ResolvedSource` is a thin tag re-encoded into `ResolvedName`.

---

## 4. Per-file roles

| File | Lines | Role |
|---|---:|---|
| `lib.rs` | 44 | Crate root: declares **18** modules, empty `Db` marker trait. Table of contents only. |
| `user_facing.rs` | 49 | LSP-only cosmetic helper `humanize_type_string` (`__effect_param_N` → `callback` in rendered strings). Looser digit-match than `ty::is_synthetic_effect_param` (deliberate, for substring use). |
| `pattern_lowering.rs` | 58 | Pure data: `PatternResult`/`PatternBinding` structs + re-exports `check_irrefutable`/`compute_match_usefulness`. No logic. |
| `resolve.rs` | 113 | On-demand single-name resolution (`resolve_name_at`) → `ResolvedName`. No stored map; re-walks scope chain + PRC each call. LSP-facing only. |
| `analysis.rs` | 211 | Generic 2-pass direct+transitive dataflow framework (`AnalysisGraph`/`Tarjan`). Single instantiation (`function_throw_sets`). |
| `throws_analysis.rs` | 340 | The shared throws structural walk (`collect_escaping_throws`/`collect_from_expr`) over `ThrowsAnalysisContext`. Exhaustive Expr/Stmt matches. |
| `callable.rs` | 389 | TIR-level `callable_throws` query + shared throws helpers (`lookup_named_throw_summary`, `substitute_throws_with_inferred_bindings`). |
| `narrowing.rs` | 430 | Control-flow narrowing (`extract_narrowings` + apply/restore over `builder.locals`); `remove_null`/`subtract_pattern_type` algebra (reused widely). |
| `throw_inference.rs` | 521 | HIR-level `function_throw_sets` (whole-package call-graph fixpoint). `flatten_ty_to_facts`, `is_banned_catch_binding_type`. |
| `generics.rs` | 609 | Type-var bind/substitute machinery (`substitute_ty`/`infer_bindings`/`bind_type_vars`/`erase_typevars_matching`/`contains_typevar`/`skip_self_param`). 4 divergent Ty walks. |
| `package_interface.rs` | 703 | `PackageInterface` + `PackageResolutionContext` queries; cross-package boundary. |
| `lower_type_expr.rs` | 776 | `TypeExpr → Ty` (name resolution, `qualify_def`, `substitute_self_in`). The bridge from syntax to semantic types. |
| `ty.rs` | 804 | The `Ty` data model + `QualifiedTypeName` + renderer. |
| `infer_context.rs` | 1065 | Diagnostic vocabulary (`TirTypeError`, 66 variants) + diagnostic sink + render pipeline. |
| `interfaces.rs` | 1487 | BEP-044 nominal interface subtyping: `ImplementsRegistry`, `match_ty_pattern*`, rule instantiation. |
| `inference.rs` | 1625 | Salsa query layer: `infer_scope_types`, `ScopeInference`, `resolve_class_fields`/`resolve_type_alias`, lambda plumbing. |
| `normalize.rs` | 2475 | Normalization + equirecursive subtyping + cycle diagnostics. `StructuralTy`, Tarjan. (~1300 lines are tests.) |
| `exhaustiveness.rs` | 4134 | rustc-port usefulness algorithm. (~2800 lines are tests.) |
| `builder.rs` | 12361 | The per-scope inference engine. ~252 methods. The single biggest mass; member/path/interface resolution (~3500 lines) is where most cruft lives. |

---

## 5. Invariant register

Every invariant claim from the sub-models, numbered, with claimed status and evidence. **This is the input to adversarial verification.** Status = sub-model's claim (enforced/assumed/unclear); corrections noted in *(verified: …)* where I checked the code.

### builder.rs
- **I1** *(enforced)* Every `infer_expr` records the expr's type into `self.expressions` before returning. Evidence: tail at builder.rs:3231-3232 `record_expr_type`; some `check_*` helpers record early and return — totality is broad but not a single chokepoint.
- **I2** *(enforced)* `pattern_types[pat_id]` is written for EVERY pattern PatId at every recursion level (MIR's `pat_ty` reads it). Evidence: builder.rs:11224 single point of truth; finalize 11086-11088; catch 5417-5418.
- **I3** *(enforced)* `aliases` always contains the full alias map; `expand_alias_chains` terminates on cyclic aliases. Evidence: 64-iteration cap builder.rs:1194; provided once at construction 979-1000, never mutated.
- **I4** *(enforced)* `is_subtype` is the only subtype entry point, layering BEP-044 on `normalize::is_subtype_of`. Evidence: builder.rs:9742-9828; 43 call sites.
- **I5** *(enforced)* Lexical scope exit restores shadowed locals to the pre-shadowing state (not scope entry) and propagates outer-binding assignments while dropping inner-shadow ones. Evidence: `restore_scoped_locals` 852-920 filters by inner_pat_ids.
- **I6** *(enforced)* Diverging branches (let-else else) use `discard_scoped_locals` (hard rollback); joining branches use `restore_scoped_locals` (merge). Evidence: discard 844-850 @4541; restore @5031/5168/5183.
- **I7** *(enforced)* Sub-body entry takes/restores the 12 shared maps; locals/logs/source-map/cache/return-ty/generics are saved inline by each caller. Evidence: 791-826; infer_lambda_body 10784-10889; check_function_parameter_defaults 2051-2141.
- **I8** *(enforced)* `nested_lambda_types` is NOT saved/restored across `infer_lambda_body`, so nested lambda types bubble to the outermost scope. Evidence: 10784-10897; field comment 715-719.
- **I9** *(enforced)* `in_optional_chain > 0` enables auto-unwrap of nullable bases; counter is balanced. Evidence: OptionalChain arm 3218-3220; Assign/AssignOp 4714/4749/4755/4775.
- **I10** *(enforced)* `resolutions` is both a persistent table AND a transient scratch slot; per-segment loop removes the entry immediately. Evidence: infer_local_rooted_path 6401-6403.
- **I11** *(assumed)* `matrix_normalize_scrut` (Optional→Union[T,null]) applied consistently to all matrix/dpat scrut tags + witness inputs; `Ty::Optional` canonical elsewhere. Evidence: PatCtx 11005/11015/11056, lower_pat_dispatch 11256, call sites 5047/5192/4574/11111. A missed site silently desyncs column/row tags.
- **I12** *(enforced)* `is_auto_derived_body` suppresses type-arg/member-lookup diagnostics for synthesized to_json/from_json bodies. Evidence: set_auto_derived 1060-1063; resolve_explicit_type_args gates 1833/1860.
- **I13** *(enforced)* Function/lambda params carry no AST PatId (`pattern: None`), so their assignments always propagate as outer-scope. Evidence: add_local/narrow_local/seed_capture_unknown 1132-1178; restore keeps `None` unconditionally 876-879.

### exhaustiveness.rs
- **I14** *(assumed)* Scrutinee + all column types fed to the matrix are normalized (Optional→Union, aliases expanded) BEFORE the algorithm sees them. Evidence: enforced in CALLER (builder `matrix_normalize_scrut`); the file never re-normalizes. **TestingCtx normalizes Optional differently** → unit tests exercise a different path than production.
- **I15** *(enforced, slice debug-only)* Ctor arity == `DPat.fields.len()`/`WitnessPat.fields.len()` at every position. Evidence: `debug_assert_eq` DPat::slice 389; algorithm always builds `arity` slots. Only debug-asserted for slice.
- **I16** *(enforced)* `Or` ctors never reach a normal `covers()`/eq check; any Or head is exploded first by `split_ctors`. Evidence: 1072-1078; covers() 166 defensively false; specialize 715-745.
- **I17** *(enforced)* Recursion terminates on recursive types without an explicit depth guard. Evidence: empty-matrix path 1086-1118 returns Missing; all-wildcard path 1161-1167 returns NonExhaustive. Comment 1001-1002.
- **I18** *(enforced)* Ctor identity: Single/Interface/UnionMember by `ty_ctor_identity`, Class by qtn only (args ignored). Evidence: PartialEq 96-108 / Hash 118-127. `Class(Foo,[int]) == Class(Foo,[string])` as ctors; real field types fetched via column type.
- **I19** *(enforced)* `is_inhabited` consulted at every empty-column + missing-ctor field; uninhabited ctors/scrutinees pruned. Evidence: entry 867, split 1091, apply_missing 975; default assumes inhabited on cycles 620. Uncached O(type-size) walk per column — perf hotspot.
- **I20** *(enforced)* `unreachable_arms` sorted + deduped. Evidence: 888-889.
- **I21** *(assumed)* Witness field ordering matches source declaration order despite reverse-unwind build. Evidence: `apply_ctor` 916-928 `.rev()`; pinned only by exact-string snapshot tests.
- **I22** *(enforced, debug-only)* `into_single_column` receives length-1 stacks. Evidence: `debug_assert_eq!(s.0.len(),1)` 991 then `pop().unwrap()`.
- **I23** *(enforced)* List types never enumerated via `enumerate_ctors`; coverage via slice splitting only. Evidence: `is_list_ty` checks at 1099/1130 run before enumerate; both PatCtx impls return empty for List.
- **I24** *(assumed)* `NonExhaustive`/`Missing` never appear in lowered source patterns (algorithm-internal). Evidence: doc 84-88; holds by construction of the caller's dpat lowering (only Single/Class/Or/Wildcard/Slice/UnionMember).

### normalize.rs
- **I25** *(enforced)* An alias is wrapped in Mu IFF in the `recursive` set from `find_recursive_aliases`. Evidence: normalize_impl 543-563; `expanding` set 544-545 emits TyVar back-edge.
- **I26** *(enforced)* `is_subtype_of` normalizes BOTH operands with the same `recursive` set so Mu var names match. Evidence: 19-22, 36-38.
- **I27** *(unclear)* The co-inductive `assumptions` set is a path-scoped stack (insert@247, remove@364). Evidence: **the Function arm has `return false` early-exits that bypass `assumptions.remove(&pair)`** *(verified in code: the ret/throws checks `return false` before reaching line 364)*. Almost certainly behavior-preserving (an assumed pair only yields `true`, and the same outer call already returned false), but NOT clean stack hygiene. **Preserve observable result, not literal set state.**
- **I28** *(enforced)* `is_subtype_of` is asymmetric; `Union <: other` (all members) MUST match before `inner <: Union` (any). Evidence: arm order 291-298 + comment 286-290.
- **I29** *(enforced)* Purely structural, NO numeric coercion: int ⊀ bigint/float; only same-representation literal→base widening. Evidence: literal arms 327-331; tests 1436-1514.
- **I30** *(enforced)* BuiltinUnknown=top (not subtype of anything specific); Unknown/Error bidirectional; Void/Type self-only. Ordering of early-returns is semantic. Evidence: 208-237.
- **I31** *(enforced)* `canonicalize()` used for equality (`is_same_normalized_type`) but NEVER inside `is_subtype_of`. Evidence: 197-367 never calls it; only 37-38 does.
- **I32** *(enforced)* `find_invalid_alias_cycles` flags an SCC IFF no structural (List/Map) edge; Optional/Union/nominal-args are pass-through. Evidence: extract_type_alias_deps 768-827; 711-722.
- **I33** *(enforced)* `find_invalid_class_cycles` has NO structural-guard exemption: every SCC is an error; field edge counts only if not behind Optional/List/Map; Union edge only if ALL variants force the same single dep. Evidence: build_class_graph 1019-1037; 1058/1140-1145; 998-1008.
- **I34** *(enforced)* Tarjan output deterministic (sorted nodes/successors/components, rotate-to-min). Evidence: 887-961. (analysis.rs Tarjan achieves determinism differently — BTreeMap, no rotate.)
- **I35** *(assumed)* `normalize_impl` maps RustType/Future→Unknown, Type→Type (opaque to pattern-matching). Evidence: 598-609; downstream reliance asserted by comments only.
- **I36** *(enforced)* EvolvingList/EvolvingMap are subtype-equivalent to List/Map (collapse in normalize_impl). Evidence: 568-574; tests 1934-2047.

### inference.rs
- **I37** *(assumed)* `index.scopes`/`scope_bindings`/`scope_ids` co-indexed by `FileScopeId::index()`; every reachable id has an entry in all three. Evidence: indexed at 691/1086/1108-1113/1206; no bounds check.
- **I38** *(assumed)* `infer_scope_types` only called with a ScopeId valid for the file's semantic index. Evidence: 689-691 no validation.
- **I39** *(enforced)* A Function scope is uniquely identified by `(range==span) AND (name==name)` — name alone insufficient (companion functions share span). Evidence: 725; repeated 1173-1176/1258-1259/1601.
- **I40** *(enforced, debug-only)* If no item_tree function matches a Function scope, it's a template string (no expr body). Evidence: `debug_assert!` 1068-1072.
- **I41** *(enforced)* A Lambda scope's body is reached by walking ancestors to the enclosing Function/Let and matching by source span. Evidence: 1075-1302 + find_lambda_by_span 470-497. Relies on globally-unique lambda spans within the body.
- **I42** *(enforced)* A lambda's contextual param types live in the PARENT's `ScopeInference.nested_lambda_types` under the lambda's FileScopeId. Evidence: seed_lambda_and_infer 617-626; builder 3655/4181/10830-10836. Cross-query consistency assumed; fallback Unknown.
- **I43** *(assumed)* Looking up contextual lambda param types via `nested_lambda_types` avoids a Salsa cycle (through package_interface). Evidence: comments 613-617/186-189; design intent.
- **I44** *(enforced)* `inference_owner_scope` returns a Function/Let/Lambda scope or the root. Evidence: 66-83.
- **I45** *(enforced)* Diagnostics stored boxed only when non-empty; empty shares a 'static EMPTY const. Evidence: 1351-1355; diagnostics() 453-460. No dedup-by-span here.
- **I46** *(enforced)* `parameter_defaults` maps use a DIFFERENT AST arena and must never merge into body maps. Evidence: doc 203-208; mem::take + restore builder 2122-2135; MIR loops separately keyed.
- **I47** *(assumed)* `path_member_resolutions[expr]` is parallel to `segments[1..]`. Evidence: doc 177-184; population in builder, not verifiable here.
- **I48** *(enforced)* `resolve_class_fields`/`resolve_type_alias` are pure functions of the Loc. Evidence: `#[salsa::tracked]` 1476/1527.
- **I49** *(enforced)* Alias chain expansion / cycle detection delegated to `normalize::find_invalid_*_cycles`. Evidence: 1401-1422.

### interfaces.rs
- **I50** *(enforced, fail-safe)* Every index bucket value is a valid index into `interface_impl_rules`. Evidence: `from_rules` 73-117 only constructor; `.get(*idx)` 337 fails safe.
- **I51** *(enforced)* `by_interface[I]` = union of `by_class[I][*]` + `by_type[I][*]` + `fallback_by_interface[I]`. Evidence: from_rules 80-112.
- **I52** *(enforced)* Class-shaped actual matches via by_class/fallback (never by_type); non-class via by_type/fallback/by_interface. Evidence: 254-289 + 86-113.
- **I53** *(assumed)* Index acceleration is behavior-equivalent to scanning all rules — an omitted rule could never have matched. Evidence: distributed across from_rules + `implementation_key_for_ty` + `match_ty_pattern_into`; **NOT a single guard**; prime place for a silent regression.
- **I54** *(assumed)* `requested_iface_ty` passed to `type_implements_interface_via_rule` is a `Ty::Interface`. Evidence: 494-499; non-interface falls back to full scan 240-251.
- **I55** *(assumed)* `match_ty_pattern_into`'s normalized fast-path and structural arms agree (when eligible & fast-path fails, structural cannot succeed). Evidence: comments 525-535/624-630; not independently checked.
- **I56** *(enforced)* `interface_requires[I]` contains I + full transitive set; cycles terminate. Evidence: interface_closure 1101-1134.
- **I57** *(enforced)* Blanket out-of-body generic rules excluded from `class_implements`; dispatched via the index only. Evidence: derive_compatibility_views 428-432.
- **I58** *(enforced)* `bind_type_var` enforces repeated type-var consistency (same normalized type). Evidence: 763-779; test 1334.
- **I59** *(enforced)* `all_class_qtns` contains every class (even those implementing nothing). Evidence: 914 + 415-417.
- **I60** *(enforced)* Diagnostics during registry build are discarded (build is diagnostic-free). Evidence: local `diags` 929/983/1066 never returned.

### infer_context.rs
- **I61** *(enforced)* `TirTypeError` carries no TextRange (Salsa-stable); locations via `DiagnosticLocation`. Evidence: 25-284; only TextRange is `DiagnosticLocation::Span` 768.
- **I62** *(enforced)* Every variant has both a Display arm and a downstream `DiagnosticId` mapping (both exhaustive, no `_`). Evidence: Display 299-717; `tir_type_error_to_diagnostic_id` check.rs:3549-3612.
- **I63** *(enforced)* Diagnostics NOT deduped/sorted in-crate; raw Vec in walk order. Evidence: push 975-984; dedup happens externally in check.rs:352 (relies on walk order placing dups adjacent).
- **I64** *(assumed)* `render()` produces a correct TextRange only with the SAME body's AstSourceMap. Evidence: 790-833 `unwrap_or_default()` on miss; correctness depends on render_scope_diagnostics matching by scope range.
- **I65** *(assumed)* `ExprMember` only attached to MemberAccess; `ExprSegment` only to multi-segment Path. Evidence: 800-805 dispatch; nothing enforces caller passes the right ExprId kind.
- **I66** *(enforced)* Suppression flag drops only the 4 synthesized-code kinds, only at Error severity. Evidence: push_error 994; report_warning 1057 bypasses; is_synthesized_code_diag 932-940.
- **I67** *(assumed)* For `RelatedLocation::Item`, exactly one contribution matches the Definition. Evidence: 846-857 find_map; zero-match silently dropped.
- **I68** *(enforced)* `RenderedTirDiagnostic.error` retained verbatim for downstream DiagnosticId/source-aware message. Evidence: 827; check.rs:3411/3439.
- **I69** *(enforced)* `render` computes message eagerly via Display, but ~12 type-bearing variants are overwritten by a file-aware re-render downstream. Evidence: 828; check.rs:3439.

### ty.rs
- **I70** *(enforced)* Every Ty variant carries exactly one TyAttr; `attr()`/`with_attr()` exhaustive + in sync. Evidence: 441-498 no wildcard.
- **I71** *(enforced)* `render_with` is total over Ty. Evidence: 643-744 no `_`.
- **I72** *(assumed)* `is_builtin_future` identifies exactly `baml.future.Future`; lower_type_expr keys off it. Evidence: 75-80; caller mapping in lower_type_expr.rs.
- **I73** *(enforced)* `is_local()`/`render_user_facing` elide the implicit `user` package; canonical Display keeps it. Evidence: render_dotted 93-108. "Single source" only if no caller post-processes (see I84).
- **I74** *(assumed)* Freshness ignored by subtype checker (Fresh(1) ≡ Regular(1)). Evidence: doc 380-389; enforced in normalize.rs (StructuralTy drops Freshness).
- **I75** *(enforced)* `widen_fresh` only removes Fresh-literal specificity; `make_evolving` only touches List(Never)/Map(Never,Never). Evidence: 508-530 / 543-555.
- **I76** *(enforced)* `union_of` preserves duplicates; callers needing dedup use `dedup_and_collapse` first. Evidence: 402-408 / 418-437.
- **I77** *(enforced)* `dedup_and_collapse` flattens only ONE level of nested Union. Evidence: 421-435.
- **I78** *(unclear)* `is_synthetic_effect_param` is the sole prefix-check authority across TIR/MIR/LSP. Evidence: 156-160 canonical, but **user_facing.rs:13-16 re-implements the prefix+digit logic inline** *(verified)* with deliberately looser semantics → "single source" is violated.
- **I79** *(enforced)* BuiltinUnknown and Unknown are semantically distinct but BOTH render "unknown". Evidence: render_with 736.
- **I80** *(enforced)* `needs_postfix_parens` groups Union and Function under postfix `[]`/`?` and return position. Evidence: 621-623.
- **I81** *(enforced)* TyAttr always `default()` in TIR; never set to a meaningful sap value; read once at MIR boundary. Evidence: 585 `default()` sites, 0 non-default, attr() read once in convert_tir2_ty, with_attr 0 in-crate callers.
- **I82** *(enforced)* Freshness never escapes TIR (MIR + codegen discard it). Evidence: lower.rs:367 / client_codegen.rs:609 bind `_freshness`.
- **I83** *(enforced)* TypeVar must be erased before MIR; survivors → error recovery (Void; TStream/TFinal→BuiltinUnknown exception). Evidence: convert_tir2_ty ~466.
- **I84** *(enforced)* Interface is compile-time-only; collapses to Class at every downstream boundary. Evidence: lower.rs:283-296; client_codegen.rs:560.
- **I85** *(assumed)* `QualifiedTypeName.generic_params` holds declared (unsubstituted) names, empty once args substituted; independent of Class/Interface Vec<Ty> args. Evidence: doc 22-24; render_with 652 keys on `type_args.is_empty() && !generic_params.is_empty()`.
- **I86** *(assumed)* Implicit user package is the single `RESERVED_USER_PACKAGE='user'`; only the user-facing path elides it. Evidence: 139; **BUT exhaustiveness.rs:1624 and interfaces.rs:1254 construct QTNs with raw `Name::new("user")` bypassing the constant.**

### lower_type_expr.rs
- **I87** *(assumed)* `TypeExpr::Path` segments always non-empty; `.last()`/`.unwrap()` never panic. Evidence: 274 `.expect`; 167 indexes `path[..len-1]`; 363-364 bare `.unwrap()`. AST invariant, not enforced here.
- **I88** *(enforced)* The package/namespace of a resolved type comes from the DEFINITION's file, never the referencing file. Evidence: qualify_def 601-609 (`def.file(db)` → `file_package`). Central correctness claim.
- **I89** *(enforced)* `baml.future.Future` → `Ty::Future` only with exactly 2 generic args; else `Ty::Class`. Evidence: 298.
- **I90** *(enforced)* A bare single-segment name that fails type resolution but matches an in-scope generic param becomes `Ty::TypeVar`, never UnresolvedType; resolve_type wins first (shadowing). Evidence: 353-357.
- **I91** *(enforced)* Enum-variant paths only produce `Ty::EnumVariant` if def is Enum AND variant exists. Evidence: 367-380.
- **I92** *(enforced)* `TypeIsNotGeneric` pushed for enum/alias generic args, but NOT for class/interface arity mismatches. Evidence: lower_non_generic 245-250; Class arm 309-311 no check.
- **I93** *(enforced)* Generic args of a non-generic type are still lowered so nested diagnostics surface. Evidence: 235-242 (return unused).
- **I94** *(enforced)* UnresolvedType "did you mean" suggestions only for single-segment names, sorted. Evidence: 388-400.
- **I95** *(enforced)* Cross-package fallback fires only for `path.len() >= 2`; first segment = `root` or interned package name. Evidence: 180-192.
- **I96** *(enforced)* `substitute_in` is pure structural clone-walk, never consults DB/diagnostics. Evidence: 80-144; tests 663-729.
- **I97** *(assumed)* Definition variants other than Class/Interface/Enum/TypeAlias in the TYPE namespace map to `Ty::Unknown` (no diagnostic) — defensive dead-ish code. Evidence: 348-350. Value def leaking into `types` would be a silent failure.
- **I98** *(enforced)* `MediaKind::Generic` → `Ty::Unknown`. Evidence: 430-434.
- **I99** *(enforced)* A `Function` type with no `throws` lowers throws to `Ty::Never`. Evidence: 551-567 `.unwrap_or(Never)`.

### package_interface.rs
- **I100** *(enforced)* PRC stores OWNED PackageInterface clones (not borrowed refs) for revision soundness. Evidence: 455-469 `.clone()`; doc 91-94.
- **I101** *(assumed)* A type's ns key (from `pkg_items.namespaces`) coincides with its qtn ns (from `file_package`). Evidence: 227/234-235/281 vs class_ns; no assertion.
- **I102** *(enforced)* resolve_type/resolve_value precedence: namespace-qualified → unqualified (only when ns empty) → package-prefixed (`root`/dep-name). First match wins, observable. Evidence: 506-543 / 571-605.
- **I103** *(enforced)* `root` is the reserved first-segment alias for the own package. Evidence: 528 / 589.
- **I104** *(enforced)* `items_for_package` returns Some only for own package or a DECLARED dependency. Evidence: 484-495.
- **I105** *(assumed)* For a dependency, re-derived `package_items` is consistent with the cloned dep PackageInterface. Evidence: 491-492/596-598 re-query; never checked.
- **I106** *(unclear)* `package_interface` and builder's `lookup_class_method` produce identical method lowering (shared helper). Evidence: package_interface 270 + lookup_own_class_method 660 share helper, BUT **builder.rs:8916 is a SEPARATE richer impl that does NOT call the helper** → equivalence not guaranteed.
- **I107** *(enforced)* The `self` param is detected by `name=="self" && ty==Unknown`; only then `build_self_type_for_class`. Evidence: 172-178.
- **I108** *(enforced)* `build_self_type_for_class` treats exactly Array(arity 1)/Map(arity 2) as builtin containers; else Class under BAML_PACKAGE. Evidence: 417-444.
- **I109** *(enforced)* `to_ty`/`def_to_ty` always yield EMPTY type-args + default TyAttr (generics dropped at boundary). Evidence: 118-124 / 677-702.
- **I110** *(assumed)* Diagnostics during interface build are intentionally discarded; correctness diagnostics emitted elsewhere by re-lowering. Evidence: every `diags` Vec unread.

### generics.rs
- **I111** *(assumed)* Callers pass matching-length `generic_params`/`concrete_args` to `bind_type_vars`; surplus silently dropped (zip). Evidence: 39 `.zip()`; doc 35-36.
- **I112** *(enforced)* `substitute_ty` preserves variable scoping: a Function's own generic params shadow outer bindings (removed from nested map). Evidence: 93-96.
- **I113** *(enforced)* `lower_type_expr_with_generics` enforces the same shadowing by INSERTING `param→TypeVar(param)`. Evidence: 243-247.
- **I114** *(unclear)* `substitute_ty`/`contains_typevar`/`infer_bindings_inner`/`erase_typevars_matching` recurse over the SAME Ty variants. Evidence: **They DIVERGE — `contains_typevar` does NOT handle Future (363); `infer_bindings_inner` does NOT recurse into Union/Future/Evolving (397).** Asymmetries relied upon implicitly; fragile.
- **I115** *(enforced)* Single-segment type-var paths intercepted before lowering (no 'unresolved type'). Evidence: 164-170.
- **I116** *(enforced)* Empty bindings → pure pass-through fast path. Evidence: 54-56 / 159-161.
- **I117** *(assumed)* In the `other =>` arm, passing `bindings.keys()` as `generic_params` keeps nested typevars as TypeVar; subsequent `substitute_ty` replaces them. Evidence: 320-331; contract lives in lower_type_expr.rs.
- **I118** *(enforced)* `skip_self_param` only strips a leading param named `self`. Evidence: 347-357.
- **I119** *(enforced)* Array<T>/Map<K,V> bridged to List/Map only when builtin-root Array/Map AND arity matches. Evidence: 457-467 (mirrors builder 1637-1649).
- **I120** *(enforced, but inherits I114 blind spot)* `erase_typevars_matching` only walks types containing a typevar. Evidence: 524 early-return — inherits `contains_typevar`'s Future blind spot.
- **I121** *(enforced)* `union_ty` produces a deduplicated union, collapses single-member to bare type. Evidence: 485-515.

### throw_inference.rs
- **I122** *(enforced)* Key shape uniform: `throw_set_key(namespace_path, short_name)` with methods `"ClassName.method"`. Consumers MUST reconstruct identically. Evidence: 224-242/101-102; callable.rs 122-138; builder.rs 6087-6134. **Enforced by convention/duplication, NOT a shared constructor for the method case.**
- **I123** *(enforced)* A declared `throws` clause is a firewall: direct facts = declared facts (body ignored), outgoing edges not propagated. Evidence: 194-200; 124-126.
- **I124** *(enforced)* Cross-package callee facts folded into caller's DIRECT facts before graph construction; only same-package callees become edges. Evidence: 127-137.
- **I125** *(assumed)* A callee is cross-package IFF some dep interface publishes that exact key. Same-package collision would be misclassified. Evidence: 128 / 498-505; no package-of-origin check.
- **I126** *(assumed)* dep_interfaces' throw_sets are already fully transitively closed when read. Evidence: package_interface.rs:407; salsa ordering.
- **I127** *(assumed)* `pkg_items`/`resolve_path_to_ty` behave like `lower_type_expr_in_ns` so recovered throw types match inference. Evidence: 411-414 "Mirrors"; hand-maintained duplication, unchecked drift.
- **I128** *(enforced)* Only Expr/Stmt::Throw contribute direct facts; only Expr::Call contributes edges; only FunctionBody::Expr analyzed. Evidence: 252-265/294-300/189-217.
- **I129** *(enforced)* Catch-binding suppression is a name-string heuristic (drop fact iff display name == a catch binding name anywhere). Evidence: 270-276/282-290/447-459. Can over-suppress on shadowing.
- **I130** *(enforced)* `resolve_path_to_ty` treats segments[..n-1] as enum path + last as variant before whole-path lookup; requires non-empty (`.expect`). Evidence: 390-403/407.
- **I131** *(enforced)* Output ordering fully deterministic (BTreeMap/BTreeSet + Tarjan). Evidence: all accumulators BTree*.

### narrowing.rs
- **I132** *(enforced)* The condition's type and all path-operand types are already in `expr_types` before `extract_narrowings`. Evidence: builder 3045/4270/4901 infer first; `local_name_and_ty` bails via `?`.
- **I133** *(assumed)* `pattern_types` has an entry for a pattern's PatId before Expr::Is narrowing reads it. Evidence: 140 graceful else-return.
- **I134** *(assumed)* apply_then → branch infer → restore_and_apply_else → restore_narrowings called in exactly that order on the SAME saved/narrowings. Evidence: builder 3056-3071/4281-4300; hand-maintained, not enforced; post-diverge path deliberately omits save/restore.
- **I135** *(enforced)* `saved.get(name)==None` means absent → restore removes it. Evidence: 355-358 / 373-374.
- **I136** *(enforced)* Narrowing only fires for single-segment paths referring to a known-typed local. Evidence: 194-197.
- **I137** *(enforced)* Truthiness narrowing only applies when the type is nullable. Evidence: 160.
- **I138** *(enforced; doc slightly misleading)* `subtract_pattern_type` never produces an empty union / never widens beyond scrutinee. Evidence: 267-271 returns original if nothing subtracted; **full subtraction returns Never via `union_of([])`, NOT the original (doc at 244 is misleading).**
- **I139** *(enforced)* Structural shape comparison ignores TyAttr + literal Freshness (because derived `Ty::PartialEq` is attr-sensitive). Evidence: ty_shape_eq 296-313.
- **I140** *(enforced)* set_current_type/apply helpers create bindings with `declared_ty=None`, `pattern=None` when absent. Evidence: 421-428.

### callable.rs
- **I141** *(enforced)* `callable_throws` is meaningfully fact-collected only for FunctionBody::Expr; Builtin→Never, Missing→Unknown. Evidence: 363-388.
- **I142** *(enforced)* A declared `throws` clause short-circuits all body analysis (firewall), in both steady-state and cycle seed. Evidence: 355-357 / 349.
- **I143** *(enforced)* `generic_params` = enclosing-class generics ++ user generics ++ synthetic effect params, in order. Evidence: 50-52.
- **I144** *(enforced; uniqueness assumed)* `function_scope_id` returns the unique Function scope matching (kind, range==span, name); missing → Unknown. Evidence: 149-155/365-369; uniqueness of the triple is assumed, `find()` takes first.
- **I145** *(assumed)* `infer_scope_types` is populated for the function scope before its throws are read. Evidence: 370; missing entries tolerated via Option fallbacks.
- **I146** *(assumed)* `callable_key`/`named_callee_key` produce the SAME key string as `function_throw_sets`/`lookup_named_throw_summary`. Evidence: 134-138 vs throw_inference 234-242; agreement by convention, no shared constructor.
- **I147** *(enforced)* `named_callee_key` prefers direct/path MemberResolution func_loc, falls back to package value resolution; only Definition::Function yields a key. Evidence: 165-195.
- **I148** *(enforced)* Method-call-convention callees skip self before binding inference. Evidence: 261-271 (same as builder 6067).
- **I149** *(enforced)* `join_throw_facts`: empty→Never, singleton→bare (never 1-element Union). Evidence: 19-30.
- **I150** *(enforced)* `callable.rs::instantiated_callee_throws` does NOT handle union-typed callees, unlike builder. Evidence: 257 returns None; builder 6048-6061 has a Union arm. **Real behavioral divergence.**

### throws_analysis.rs
- **I151** *(assumed)* Every id reached indexes validly into THIS body's arenas. Evidence: 55/119/196/222/256 no bounds handling.
- **I152** *(enforced)* The Expr match in `collect_from_expr` is exhaustive (no wildcard). Evidence: 196-339 explicit no-op arms.
- **I153** *(enforced)* The Stmt match in `collect_from_stmt` is exhaustive. Evidence: 119-168.
- **I154** *(enforced)* For a Call with OptionalMemberAccess callee, optional wrapper stripped before reading callee throws; OptionalCall unwrap always true. Evidence: 206-208 / 210-212.
- **I155** *(enforced)* A catch substitutes precomputed `catch_residual_throws` for its base when available; else walks base. Requires the map populated BEFORE this walk. Evidence: 213-218.
- **I156** *(enforced)* In catch-base mode, a nested catch is opaque (residual None, clause arms not walked). Evidence: CatchBaseThrowsAnalysis builder 482/504; falls to walk-base 214/skip-clauses 219.
- **I157** *(enforced)* An `await` adds the future's E to throws ONLY in catch-base mode. Evidence: 321-332 gated on `await_adds_future_error()`; default false, override true builder 508.
- **I158** *(enforced)* `spawn` body throws do NOT escape (only `name` expr walked). Evidence: 311-320.
- **I159** *(enforced)* Unresolved callee throws → `Ty::Unknown` (conservative). Evidence: 106-110.
- **I160** *(enforced; partial)* Throw facts are FLATTENED leaves. Evidence: flatten at 89/103/329. **Named-callee summary results (105) and catch residuals (215) extended WITHOUT re-flattening** (assumed already-flattened).
- **I161** *(enforced)* Result set deduped + deterministically ordered (BTreeSet). Evidence: 70 and all signatures.
- **I162** *(enforced)* let-else else-blocks/assignment targets can throw and escape; while/for/return sub-exprs walked. Evidence: 124-165.

### analysis.rs
- **I163** *(enforced, structural)* Tarjan emits SCCs in reverse topological order (`propagate` depends on it). Evidence: components pushed only when subtree complete 208; 87/101 read scc_facts at indices < current.
- **I164** *(enforced)* Every node (node/edge source/edge target) is a key in `edges`. Evidence: add_node 40, add_edge 46-47.
- **I165** *(enforced)* Every node in `edges` has a (possibly empty) entry in `direct`. Evidence: add_node 39, add_edge or_default 45; missing `from` default is benign.
- **I166** *(enforced)* `node_to_scc` maps every node to exactly one SCC index. Evidence: Tarjan partitions; on_stack prevents re-entry.
- **I167** *(enforced)* `add_node` OVERWRITES direct facts; `add_edge` never clobbers (or_default). Throw_inference relies on this (nodes-then-edges ordering). Evidence: 39 insert vs 45 or_default; throw_inference 140-148.
- **I168** *(enforced)* Output ordering (`iter_transitive`) deterministic regardless of insertion order. Evidence: BTreeMap/BTreeSet 63/15.
- **I169** *(assumed)* `strong_connect` recursion depth bounded by longest acyclic chain; deep graphs could overflow. Evidence: 170 genuinely recursive, no guard.
- **I170** *(enforced)* The local `node: NodeState` copy stays source of truth for low_link, written back after the successor loop. Evidence: 186-193; successors read only index/on_stack mid-recursion.

### resolve.rs
- **I171** *(enforced)* The local-binding check (`visible_binding_at`) runs BEFORE package resolution (let/param shadows package item). Evidence: 69-88 early return.
- **I172** *(enforced)* Local visibility is position-sensitive: visible iff `binding.visible_from <= at_offset`. Evidence: semantic_index.rs:169; params have no such guard.
- **I173** *(enforced)* The local walk skips intermediate Class scopes (except the starting scope). Evidence: semantic_index.rs:162-165.
- **I174** *(enforced)* Local bindings shadow by reverse-iteration (last wins); bindings precede params in the same scope. Evidence: semantic_index.rs:168/173.
- **I175** *(enforced)* `scope_at_offset` never panics on miss; falls back to ROOT. Evidence: semantic_index.rs:336-342.
- **I176** *(enforced)* Package-level resolution depends only on the file's namespace_path, not cursor scope. Evidence: resolve.rs 77-83/92-93.
- **I177** *(enforced)* `resolve_value` tried before `resolve_type` (value wins on name collision). Evidence: 83-110.
- **I178** *(assumed)* For type resolution, `resolve_type`'s success is used ONLY as a guard; the Definition is re-derived by a second lookup (can in principle disagree). Evidence: 89-110; for the single-segment name_path used here, the divergent branches are unreachable so they agree in practice.
- **I179** *(enforced)* `name_path` is always single-segment (len 1) here, making multi-segment branches in resolve_value/resolve_type dead for these callers. Evidence: 82 `slice::from_ref`.
- **I180** *(assumed)* `items_for_package(dep_name)` always resolves via the dependency branch (never own). Evidence: 102-103; holds unless a package lists itself as its own dependency.
- **I181** *(unclear)* `ResolvedName::Builtin` denotes a dependency-package result, NOT specifically the `baml` builtin package, despite the doc. Evidence: doc 35 vs ResolvedSource::Builtin 79-80; misleading naming.
- **I182** *(enforced)* `binding_site` is total over BindingIds from `visible_binding_at`. Evidence: semantic_index.rs:391-396.

### Ty/StructuralTy/normalize cross-cluster (type-cluster sub-models)
- **I183** *(enforced)* `class_implements` + `interface_impl_rule_index` are pure derived projections of `interface_impl_rules`. Evidence: from_rules 1046 + derive_compatibility_views 1047; field doc 122-124 steers new code to the rules.
- **I184** *(enforced)* `PackageInterface.throw_sets` is an exact clone of `function_throw_sets(pkg)` — same data reachable two ways. Evidence: 407.
- **I185** *(enforced; verified)* `ResolvedMethod`/`PRC::lookup_class_method`/`lookup_own_class_method` are never exercised by production — only by one regression test. Evidence: *verified* — only caller is baml_tests/src/compiler2_tir/inference.rs:570; all builder `self.lookup_class_method` calls bind to `TypeInferenceBuilder::lookup_class_method` (builder.rs:8916), a separate method.
- **I186** *(assumed)* The HIR throw engine (`function_throw_sets`) and the TIR engine (`callable_throws`) agree wherever both are observed. Evidence: no cross-check; two independent impls feeding codegen 'Raises:'; relied upon by snapshot stability.

### lib.rs
- **I187** *(enforced)* All public symbols advertised in the rustdoc exist (`Ty`, `ScopeInference`, `infer_scope_types`, `TypeInferenceBuilder`, `resolve_name_at`, `resolve_class_fields`, `resolve_type_alias`). Evidence: ty.rs:170, inference.rs:147/684/1477/1528, builder.rs:599, resolve.rs:45.
- **I188** *(enforced)* `Db` is a methodless marker; supertrait chain hir←ppir←tir←mir holds. Evidence: lib.rs:44; ppir lib:35; mir lib:15.
- **I189** *(enforced)* Every concrete DB running TIR queries explicitly implements `tir::Db` (no blanket impl). Evidence: baml_project db.rs:119, lsp testing.rs:106, emit lib.rs:2091, per-module TestDbs.

---

## 6. Redundancy & complexity observations (merged, deduplicated)

Ranked by line/structural-reduction leverage. All are **behavior-preserving** opportunities (outputs unchanged by deduplicating producers).

### Top structural levers

**R1 — The 4-enum `Ty` chain.** `Ty` ↔ `StructuralTy` (normalize.rs, ~80% a copy + Mu/TyVar) ↔ `baml_type::Ty` (MIR, ~250-line `convert_tir2_ty`) ↔ `cg::Ty` (codegen, ~120-line `convert_tir_to_codegen_ty`). The only real TIR-specific additions (Freshness, Evolving*, Interface, TypeVar) are all **erased before MIR anyway**. Collapsing `StructuralTy` into `Ty`-with-resolved-aliases + an external recursion guard would delete the entire `StructuralTy` mirror (~120 lines of enum + `canonicalize` + `substitute` + `is_subtype_of` variant duplication). Highest leverage AND highest risk (subtype semantics + ~1,234 snapshots ride on it).

**R2 — Two (three) parallel throws engines.** `function_throw_sets` (HIR-level, Name-keyed, `analysis.rs` graph fixpoint) vs `callable_throws` (TIR-level, FunctionLoc-keyed salsa query) vs a 3rd inline impl in `builder.rs::instantiated_callee_throws`. Both feed codegen 'Raises:' output. ~520 + ~390 lines plus duplicated key derivation and path/type resolution (`resolve_path_to_ty` mirrors `lower_type_expr_in_ns`). The `analysis.rs` framework (211 lines, generic, **single instantiation**) can be deleted/inlined if the engines merge. **Note divergence (I150):** the two differ on union-typed callees, so naive merge is behavior-changing — verify equivalence first.

**R3 — `DefaultParameterInference` ⇄ `ScopeInference`.** A near-exact clone of 10 fields, with a fully parallel `iter_default_*` accessor surface (~12 methods) and doubled MIR merge loops, driven only by the separate-AST-arena constraint. The split is **undone by MIR** (re-merged under `MetadataScope::Body` vs `ParameterDefault`). Replace with a single arena-tagged key or a generic newtype → deletes the second struct, doubled accessors, the parallel `mem::take` block, and the doubled MIR merge.

**R4 — The per-body map set written 4× + the 14-tuple `finish()`.** The same ~10–12 maps appear as builder live fields + `SavedInferenceState` + `DefaultParameterInference` + `ScopeInference`, and `finish()` returns them as an **unnamed positional 14-tuple** that the LSP crate (`check.rs:285`) also destructures. A shared `InferenceMaps` newtype embedded in all four would collapse `take/restore_inference_state`, the 14-tuple, and the `ScopeInference{...}` construction into one move/swap each.

**R5 — `InterfaceImplRuleIndex` is a pure perf index.** 100% derived from `interface_impl_rules`; carries no information not recomputable by scanning. Since **perf is not a constraint**, the four maps + `from_rules` + `primary_or_fallback` + `any_indexed_rule_matches` + the actual_ty dispatch can collapse to one linear scan filtered by `interface_qtn` — deletes a large chunk with zero behavior change. (`class_implements` is likewise a derived view — R8.)

### Member/path resolution (the single biggest mass)

**R6 — Triplicated member resolution.** `resolve_member` / `resolve_member_for_path_segment` / `try_resolve_member_on_ty` are three near-parallel matches over the same Ty variants (~600 lines combined) differing only in diagnostic span + side-effect-freeness. Plus ~12 interface-source helpers (`for_each_implemented_interface_field`, `class_interface_field_sources`, etc.) each re-resolve class_loc + item_tree + ns + walk `implements` + `interface_closure_locs`. This ~3500-line cluster (mostly accumulated BEP-044 machinery) is where a rewrite gains the most lines.

**R7 — Class-method lowering exists 3×.** `lower_class_method_signature` (package_interface, used for dep interface + the test-only `lookup_own_class_method`) vs `TypeInferenceBuilder::lookup_class_method` (builder.rs:8916, separate richer impl with concrete self-type). The generic-param merge is duplicated verbatim at 4+ sites. Related dead weight: **`ResolvedMethod` + `PRC::lookup_class_method` + `lookup_own_class_method` are test-only (I185) → deletable** (~70 lines + the type).

### Diagnostics

**R8 — Two diagnostic channels + duplicated message templates.** Rich `TirDiagnostic` (via `InferContext`) vs bare `Vec<TirTypeError>` (lowering: lower_type_expr/generics/interfaces/package_interface/MIR — 49 throwaway sites, many discarded). Separately, `Display` ⇄ LSP `source_aware_tir_type_error_message` duplicate message templates for ~15 type-bearing variants and **have already drifted**. Unify the channel (pass the `InferContext` sink instead of `&mut Vec<TirTypeError>`) and parameterize message-formatting by a Ty-renderer fn.

### Recursion-walk duplication

**R9 — N divergent Ty/StructuralTy/TypeExpr walks.** `generics.rs` alone has 4 (`substitute_ty`/`contains_typevar`/`infer_bindings_inner`/`erase_typevars_matching`) **with drifted variant coverage** (I114: Future/Union/Evolving missing from some) — a latent-bug audit prerequisite to unifying. `normalize.rs` has 4 more (`normalize_impl`/`ty_has_cycle`/`extract_type_alias_deps`/`extract_required_class_deps`). `interfaces.rs` has `contains_bound_typevar`/`contains_generic_function_binders`. One generic fold/visitor could subsume the structural-descent portions.

**R10 — Two Tarjan + NodeState impls.** `normalize.rs::Tarjan` (HashMap, deterministic sort+reverse+rotate, cycle-filter) vs `analysis.rs::Tarjan` (generic `<N:Ord>`, BTreeMap, no rotate). `NodeState` is field-identical in both. The generic one can subsume the other if the caller supplies the determinism post-pass (which the diagnostics ordering depends on). Plus `find_recursive_aliases` (DFS) + `find_invalid_alias_cycles` (Tarjan) walk the same alias graph twice.

### Smaller / localized

**R11 — `DPat` ≅ `WitnessPat`** (structurally identical {ctor, fields, ty}); unify into one `Pat`, keep the two Display impls as the only fork. `WitnessStack`/`WitnessMatrix` are thin Vec newtypes. `CtorIdentity(String)` exists only because `Ty::PartialEq` is span/freshness-sensitive.

**R12 — `union_ty` duplicated** in builder.rs (~9700) with the identical TODO comment + `TyAttr::default()` dedup-collapse. The `Ty::Function` rebuild block recurs ≥4× (builder, `substitute_ty`, `erase_typevars_matching`).

**R13 — Key-string construction 3×** (`throw_set_key`/`callable_short_name`/`dotted_method_key` — `"Class.method"` join). `segments_to_dotted_name` also reimplemented inline. Must stay byte-identical or named-throw lookups silently miss.

**R14 — `is_synthetic_effect_param` (I78)** + `callback` literal both duplicated in `ty.rs` and `user_facing.rs` (deliberately looser digit-match in the latter); `user_facing.rs` could be deleted by pushing substitution into the LSP's `display_type_expr`.

**R15 — Optional-member/index nullable idioms** (`infer_optional_member_access_expr`/`infer_optional_index_expr`/path-segment branch) re-implement the same `remove_null + is_pure_null + gate-on-in_optional_chain + report + re-wrap` flow. `try_container_method_call`/`try_index_assign_mutation` duplicate the evolving-container flow for List vs Map.

**R16 — `ResolvedSource` thin tag** re-encoded immediately into `ResolvedName::{Item,Builtin}`; `ResolvedInterface` (loc+qtn, qtn usually discarded) overlaps LSP's `ResolvedInterfaceData`. The dep-type-alias harvesting loop is duplicated in `inference.rs` and LSP `check.rs`.

### Essential complexity (DO NOT casually simplify — high snapshot risk)
- `check_call_inner` (call pipeline: phase0 reverse + 2-pass forward inference, union-of-functions fold, lambda-args-last ordering, `bindings_from_inference` diagnostic suppression).
- The scoped-locals "Slack rules" (propagate-outer / drop-inner / hard-rollback-on-divergence) + the split save/restore (SavedInferenceState + inline saves).
- `split_ctors` (7 regimes, each a real termination/duplicate-witness edge case) + the witness stack-reversal.
- Constant folding (`try_fold_*`, overflow/cap/div-by-zero/shift guards must match the VM exactly).
- The equirecursive Mu machinery + co-inductive `assumptions` (I27 stack-discipline leak is observable-result-preserving but not literal-clean).
- `match_ty_pattern_into` fast-path/fallback duality (I55) + generic-function alpha-equivalence matching.
- `infer_catch_expr` residual-throw tracking across clauses/arms.
- `render_with` presentation rules (postfix parens, `<_,_>` placeholders, `(evolving)`, `user.` elision, `callback`) — exact choices are snapshot-observable across 3 trait implementors.
