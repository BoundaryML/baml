# Interface / Generic-Bounds / Associated-Type Logic — TIR Map

> **Purpose.** A map of *every* place interface, generic-bound, and associated-type
> reasoning lives in the compiler2 TIR, how each piece relates to the others, and how it
> connects to the adjacent phases (AST/HIR upstream, MIR/Emit/VM downstream). Written as
> a demolition reference: it identifies the load-bearing walls, the redundant scaffolding,
> and the Salsa wiring you must preserve when you nuke the rule logic.
>
> **Aspiration vs. reality.** [`TYPE_SYSTEM.md`](./TYPE_SYSTEM.md) is *prescriptive* (how
> it should work); this doc is *descriptive* (how the code works today, 2026-06). Where
> they diverge, that divergence is itself a demolition target and is called out.

---

## 0. TL;DR — the demolition surface

There are **three overlapping interface-reasoning systems** in the TIR plus **two
normalization layers**. The redundancy is the reason "nuking" is attractive:

| # | System | Where | Status |
|---|---|---|---|
| 1 | `InterfaceImplRule` registry + rule matcher + coherence | [`interfaces.rs`](./crates/baml_compiler2_tir/src/interfaces.rs) | **Canonical** (newest, keep the shape) |
| 2 | "Compatibility views" (`class_implements`, `type_implements`, `blanket_class_implements`, `implements_type_args`) | [`interfaces.rs:200-671`](./crates/baml_compiler2_tir/src/interfaces.rs:200) | **Derived legacy** — built *from* system 1 for old callers; deletable once callers migrate |
| 3 | Hand-rolled `TypeInferenceBuilder::is_subtype` with ~10 interface special-cases | [`builder.rs:14690`](./crates/baml_compiler2_tir/src/builder.rs:14690) | **Duplicates** `baml_type::normalize::is_subtype`; the biggest cleanup |
| A | `baml_type::normalize` — canonical set-theoretic algebra w/ `TypeContext` trait | [`baml_type/src/normalize.rs`](./crates/baml_type/src/normalize.rs) | **Aspirational target** — only ~4 call sites adopted in TIR |
| B | `baml_compiler2_tir::normalize` — TIR-local subtype/normalize | [`normalize.rs`](./crates/baml_compiler2_tir/src/normalize.rs) | **Legacy** — still 25 call sites; to be subsumed by (A) |

**The clean end-state** most of this code is groping toward: one impl-rule registry
(system 1) feeding nominal facts through the `TypeContext` trait into one canonical
algebra (system A), with systems 2, 3, B deleted. Keep this in mind as the north star.

---

## 1. Pipeline placement

```
 AST            HIR                       TIR (this doc)                        MIR / Emit / VM
 ───            ───                       ──────────────                        ───────────────
 InterfaceDef   item_tree::Interface      lower_type_expr  ─┐                   VirtualCall vs
 Implements*Def ImplementsBlock/For       generics          ├─ ImplementsRegistry  static switch
 (unresolved    (still unresolved          interfaces ──────┤   (Salsa-tracked)  resolve_implements_rule
  TypeExprs)     TypeExprs + method IDs)   associated_proj  ─┤                   (runtime, open-world)
                                          normalize (×2)    ─┘
                                          builder (driver: is_subtype, NormalizeCtx)
                                          package_interface (export)
                                          ── diagnostics live in baml_lsp2_actions::check ──
```

- **Upstream (AST/HIR)** keep interface syntax as **unresolved `TypeExpr` trees** plus
  method-ID lists. They do *no* type resolution — name/scope only. So all
  interface *semantics* begin in the TIR.
- **The TIR** resolves those `TypeExpr`s to `baml_type::Ty`, builds the impl registry,
  answers subtype/membership/coherence, and resolves associated-type projections.
- **Downstream (MIR/VM)** re-derive dispatch from the *same* registry query
  (`package_implements_registry`) — the TIR does **not** hand MIR a resolved impl id; MIR
  re-asks. Final impl selection is deferred to a separate **runtime** resolver (open world).

---

## 2. The type representation (`ty.rs` → `baml_type`)

TIR no longer owns its `Ty`. [`ty.rs`](./crates/baml_compiler2_tir/src/ty.rs) is a
re-export shim; the canonical enum is [`baml_type::Ty`](./crates/baml_type/src/lib.rs:134).
The variants that carry interface/generic/assoc information:

| Variant | Shape | Carries |
|---|---|---|
| `Ty::Interface(TypeName, Vec<Ty>, Vec<(Name,Ty)>, TyAttr)` | existential `dyn I<args; Assoc=…>` | generic args **and** associated-type bindings inline |
| `Ty::TypeVar(Name, TyAttr)` | a generic param / `Self` / assoc-projection root | **just a `Name`** — no id, no de Bruijn, **no bound stored on it** |
| `Ty::AssociatedTypeProjection { base, interface: Option<Box<Ty>>, member, attr }` | `T.Item` or `(T as I).Item` | the projection, possibly unresolved |
| `Ty::Class(TypeName, Vec<Ty>, TyAttr)` | `Box<int>` | generic type args |
| `Ty::Function { generic_params, generic_param_bounds: Vec<Option<Ty>>, … }` | generic fn type | **bounds live here**, parallel to params |

**Two critical representation facts that shape everything downstream:**

1. **A type variable's bound is *not* on the `TypeVar`.** It lives either in
   `Function::generic_param_bounds` (for function generics) or, during inference, in a
   **side table** `TypeInferenceBuilder::generic_param_bounds: FxHashMap<Name, Ty>`
   ([`builder.rs:674`](./crates/baml_compiler2_tir/src/builder.rs:674)). Every bound lookup
   is name-keyed against that map. This is why the same `Name` can mean different things in
   different scopes and why substitution is pure name-replacement.

2. **`Ty::Interface` bakes associated-type bindings inline**, but a *bound* uses the
   separate [`baml_type::interface::Interface<T>`](./crates/baml_type/src/interface.rs:13)
   struct (`{ name, generics, associated_types: HashMap }`) where assoc bindings are
   *optional*. Existential-position interfaces must be fully specified; bound-position
   interfaces need not be. Two representations of "an interface", chosen by position.

**Subenum partition** ([`baml_type/src/runtime_ty.rs`](./crates/baml_type/src/runtime_ty.rs)):
`ConcreteTy ⊆ RealizedTy ⊆ RuntimeTy ⊆ Ty`. `TypeVar`, `AssociatedTypeProjection`, and the
error/evolving sentinels are *not* in `ConcreteTy`/`RealizedTy`. The
`is_valid_impl_subject` / `validate_runtime` helpers gate what may implement an interface and
what may cross to runtime. This partition is the spec's "concrete types are atomic" rule made
structural — preserve it.

---

## 3. The TIR modules, in dependency order

### 3.1 `lower_type_expr.rs` — syntax → `Ty`

[`lower_type_expr.rs`](./crates/baml_compiler2_tir/src/lower_type_expr.rs) turns an
unresolved `ast::TypeExpr` into a `Ty`, resolving paths against package items.

- `lower_type_expr_in_ns` ([:468](./crates/baml_compiler2_tir/src/lower_type_expr.rs:468)) —
  the workhorse; resolves a path to a `Class`/`Interface`/`Enum`/alias, lowers generic args
  and associated-type bindings into the `Ty::Interface`/`Ty::Class` payloads.
- **`Self` handling is textual substitution at this layer**, not a `Ty` variant:
  `substitute_self_in` / `substitute_self_in_preserving_associated_projections`
  ([:47](./crates/baml_compiler2_tir/src/lower_type_expr.rs:47),
  [:55](./crates/baml_compiler2_tir/src/lower_type_expr.rs:55)) rewrite `Self` (and `Self`-
  rooted member paths into associated projections) *before* lowering. There is no
  `Ty::SelfType`; `Self` becomes either a concrete type (in an `implement … for C`) or a
  `TypeVar("Self")` (in an interface body), bounded by the interface via the side table.
- `can_be_associated_type_projection_base` ([:457](./crates/baml_compiler2_tir/src/lower_type_expr.rs:457))
  decides whether `Foo.Bar` lowers to a projection vs. a namespaced path.

This file is mostly **mechanical and reusable** — it would survive a nuke largely intact.
The `Self` substitution is the subtle part; keep its associated-projection-preserving variant.

### 3.2 `generics.rs` — substitution & inference primitives

[`generics.rs`](./crates/baml_compiler2_tir/src/generics.rs) is the pure type-variable
toolkit. **No Salsa, no registry** — just `Ty → Ty` functions:

- `bind_type_vars(params, args) → FxHashMap<Name,Ty>` ([:37](./crates/baml_compiler2_tir/src/generics.rs:37)) — zip params to args.
- `substitute_ty(ty, bindings)` ([:53](./crates/baml_compiler2_tir/src/generics.rs:53)) — recursive name substitution; scopes nested fn binders to avoid capture.
- `infer_bindings(formal, actual, &mut bindings)` ([:778](./crates/baml_compiler2_tir/src/generics.rs:778)) and the **`_rigid_self`** variant ([:789](./crates/baml_compiler2_tir/src/generics.rs:789)) — unify a formal (with type vars) against an actual to infer generic args at a call site. `rigid` pins `Self` so argument inference never instantiates it (mirrors rustc's `ty::Param`).
- Type-var detectors: `contains_typevar`, `contains_non_rigid_typevar(ty, rigid)` ([:506](./crates/baml_compiler2_tir/src/generics.rs:506)), `is_value_call_inferable` — the readers my memory note ["unspecialized generic representation"] depends on.
- `erase_unresolved_typevars` / `erase_typevars_where` ([:850](./crates/baml_compiler2_tir/src/generics.rs:850), [:961](./crates/baml_compiler2_tir/src/generics.rs:961)) — turn unresolved callee type vars into `BuiltinUnknown` after inference.

This module is **foundational and largely orthogonal** to the impl-rule logic. A nuke of
the *rule* system should leave it standing.

### 3.3 `interfaces.rs` — THE registry, matcher, and coherence engine ★

[`interfaces.rs`](./crates/baml_compiler2_tir/src/interfaces.rs) (4260 lines) is the heart
of what you're removing. Three sub-systems:

**(a) The canonical rule and its index.**
- `InterfaceImplRule` ([:71](./crates/baml_compiler2_tir/src/interfaces.rs:71)) — the unified shape every impl lowers to: `{ generic_params, generic_param_bounds, for_ty_pattern, interface_ty, origin, source_span }`. In-body `implements I {}`, out-of-body `implement<T> I for C<T>`, concrete `implement I for int`, and bounded blanket impls **all** become this one struct. *This is the abstraction worth keeping.*
- `InterfaceImplOrigin` ([:57](./crates/baml_compiler2_tir/src/interfaces.rs:57)) — `InBodyClass{class_qtn}` vs `OutOfBody`; lets MIR recover methods from HIR without the rule carrying them.
- `InterfaceImplRuleIndex` ([:102](./crates/baml_compiler2_tir/src/interfaces.rs:102)) — lookup acceleration: `by_interface` / `by_class` / `by_type` / `fallback_by_interface`. Pure performance; rebuildable from the rule vec via `from_rules`.

**(b) The matcher** (the part the runtime resolver mirrors):
- `match_ty_pattern` / `match_ty_pattern_into` ([:951](./crates/baml_compiler2_tir/src/interfaces.rs:951)) — unify a rule's `for_ty_pattern` against an actual type, binding the rule's generic params. Handles nested interface args, union order-insensitivity, repeated-var conflicts, function generic-bound matching.
- `rule_matches_actual` ([:317](./crates/baml_compiler2_tir/src/interfaces.rs:317)) and `type_implements_interface_via_rule` ([:435](./crates/baml_compiler2_tir/src/interfaces.rs:435)) — the public "does `T` implement `I`?" entry, routed through the index. Takes an `is_subtype` closure as an **oracle boundary** (so bound obligations defer to the builder's subtype checker rather than re-deriving).
- `validate_rule_bounds` ([:814](./crates/baml_compiler2_tir/src/interfaces.rs:814)) — after a structural match, prove each `generic_param_bound` holds at the bound instance.
- `first_failing_bound` ([:371](./crates/baml_compiler2_tir/src/interfaces.rs:371)) — diagnostic helper to name the unsatisfied bound instead of a bare mismatch.

**(c) The coherence (overlap) engine** — the genuinely hard, genuinely valuable part:
- `package_coherence_diagnostics` ([:1721](./crates/baml_compiler2_tir/src/interfaces.rs:1721), Salsa-tracked) — per-package overlap check over the package **and its dependency closure**, relying on the orphan rule to make the per-package pass complete (the [interface-coherence-plan] memo).
- `impls_conflict` / `impls_overlap` ([:1828](./crates/baml_compiler2_tir/src/interfaces.rs:1828), [:1888](./crates/baml_compiler2_tir/src/interfaces.rs:1888)) — does an overlapping common instance exist? `Overlap{Yes,No,Unknown}` is a **three-valued** result: ACI-unification is NP-hard, so the search is bounded (`MAX_OVERLAP_SEARCH_STEPS = 4096`, `MAX_UNIFY_DEPTH = 256`) and **fails closed** to `Unknown` → conservative rejection. This implements TYPE_SYSTEM.md §"Interface Coherence" (the 3-SAT reduction) — the unit tests at the bottom of the file literally encode 3-SAT and pigeonhole instances.
- The unification core: `unify_into` / `unify_into_at` / `unify_all` ([:2303](./crates/baml_compiler2_tir/src/interfaces.rs:2303)), union ACI handling (`unify_union_members`, `try_union_set_equality`, `cover_search` [:2773](./crates/baml_compiler2_tir/src/interfaces.rs:2773)), `bounds_hold_at_common_instance` ([:1946](./crates/baml_compiler2_tir/src/interfaces.rs:1946)), and the local normalizer `nf` ([:2069](./crates/baml_compiler2_tir/src/interfaces.rs:2069)) that folds complete enums/bools and subsumes literals.

  ⚠️ **Demolition caution:** the overlap engine is the one piece here that is *not* redundant
  with anything else and encodes hard-won soundness (fail-closed budget). If you nuke
  `interfaces.rs`, this engine must be **relocated, not deleted**. It depends only on the rule
  vec + an `enum_variants` lookup + aliases, so it is movable.

**(d) `requires`-closure walkers** (interface-to-interface subtyping & assoc propagation):
- `interface_closure` ([:3021](./crates/baml_compiler2_tir/src/interfaces.rs:3021)) — transitive `requires` set per interface, cached; feeds `interface_requires` in the registry.
- `interface_closure_locs` ([:3060](./crates/baml_compiler2_tir/src/interfaces.rs:3060)) and `interface_closure_locs_with_args_and_assoc` ([:3104](./crates/baml_compiler2_tir/src/interfaces.rs:3104)) — BFS the `requires` chain carrying generic args **and** associated-type bindings down to parents (e.g. `Child requires Parent<Item=int>`). **Heavily used by `builder.rs`** (member resolution) and `associated_projection.rs` — 15+ call sites. Any nuke must preserve these or their equivalent.

**(e) The legacy compatibility views** — `ImplementsRegistry` fields `class_implements`,
`type_implements`, `blanket_class_implements`, `implements_type_args`,
`type_implements_type_args`, built by `derive_compatibility_views`
([:609](./crates/baml_compiler2_tir/src/interfaces.rs:609)). These are **pure projections of
the rule vec** for callers that predate rules. `ImplementsRegistry::implements` /
`type_implements` / `blanket_class_implements_interface` read them. **These are the first
thing to delete** once callers route through `type_implements_interface_via_rule`.

### 3.4 `associated_projection.rs` — resolving `T.Item`

[`associated_projection.rs`](./crates/baml_compiler2_tir/src/associated_projection.rs) owns
`AssociatedTypeProjection` resolution. `AssociatedProjectionResolver` ([:47](./crates/baml_compiler2_tir/src/associated_projection.rs:47))
is constructed with the db, resolution context, aliases, and the type-var bound table, then:
- `resolve_deep` ([:90](./crates/baml_compiler2_tir/src/associated_projection.rs:90)) — recursively concretize every projection in a `Ty`, with a `resolving` set to stop cycles.
- `resolve_projection` dispatches to `resolve_primitive_projection` / `resolve_interface_projection` / `resolve_class_projection` ([:362](./crates/baml_compiler2_tir/src/associated_projection.rs:362), [:425](./crates/baml_compiler2_tir/src/associated_projection.rs:425), [:582](./crates/baml_compiler2_tir/src/associated_projection.rs:582)) — each consults `package_implements_registry` + `interface_closure_locs_with_args_and_assoc` to find the impl's `Assoc=…` binding.
- `resolve_projection_bound` ([:219](./crates/baml_compiler2_tir/src/associated_projection.rs:219)) — when the base is still generic, surface the *bound* (`type Item extends Summarizable`) so member lookup can proceed on an abstract projection.
- `projection_views_equivalent` ([:259](./crates/baml_compiler2_tir/src/associated_projection.rs:259)) — equate two projections that name the same `(I,T,A)` triple even when unresolved; called from `builder::is_subtype`.

This module is a **consumer** of the registry, not part of the rule system itself, but it's
tightly coupled to `interface_closure_locs_with_args_and_assoc`. It must move with whatever
replaces the closure walkers.

### 3.5 The two normalize layers

- **TIR-local** [`normalize.rs`](./crates/baml_compiler2_tir/src/normalize.rs):
  `is_subtype_of(sub, sup, aliases)` ([:18](./crates/baml_compiler2_tir/src/normalize.rs:18)),
  `is_same_normalized_type`, plus a full structural normalizer. Takes **only aliases** — it
  has no notion of the impl registry, so it cannot answer `C <: I`. ~25 call sites remain.
- **Canonical** [`baml_type::normalize`](./crates/baml_type/src/normalize.rs): the
  set-theoretic algebra parameterized by the `TypeContext` trait
  ([:50](./crates/baml_type/src/normalize.rs:50)) with `alias_def`, `implements_interface`,
  `type_var_bound`, `interface_requires`, `enum_variants` — i.e. exactly the nominal facts
  the registry provides. Every lookup **fails safe**. Public API: `normalize`, `equivalent`,
  `is_subtype`, `definitely_disjoint`, `definitely_equal`, `constant_equality`.

The canonical layer is the **intended single home** for all structural reasoning; the TIR
local copy is legacy. The migration is *in progress* — only `normalize`, `equivalent`,
`constant_equality`, and the `TypeContext` impl are wired up so far.

### 3.6 `builder.rs` — the inference driver & oracle

[`builder.rs`](./crates/baml_compiler2_tir/src/builder.rs) (18k lines) drives per-scope
inference and is where the interface logic is *actually invoked*. Key interface-facing pieces:

- **`NormalizeCtx`** ([:531](./crates/baml_compiler2_tir/src/builder.rs:531)) — the
  `TypeContext` adapter that bridges the canonical algebra to the registry. `implements_interface`
  → `type_implements_interface_via_rule`; `type_var_bound` → the side table;
  `interface_requires` → `registry.interface_requires` + `interface_requires_instantiation`;
  `enum_variants` → `lookup_enum_variants`. **This adapter is the seam** along which the clean
  end-state is built: more reasoning should move *behind* it into `baml_type::normalize`.
- **`is_subtype`** ([:14690](./crates/baml_compiler2_tir/src/builder.rs:14690)) — the hand-rolled
  oracle, ~250 lines of interface special-cases: type-var reflexivity before bound expansion,
  associated-projection equivalence, `TypeVar` bound → `Interface` matching with assoc bindings,
  alias/projection resolution, function generic binders, union-as-sub, invariant class-arg
  compatibility, `C <: I` via registry, `I <: J` via `requires`. **This is system #3 — the
  prime duplication target.** Much of it should be `baml_type::normalize::is_subtype` once the
  `TypeContext` is complete.
- `generic_param_bounds: FxHashMap<Name, Ty>` ([:674](./crates/baml_compiler2_tir/src/builder.rs:674)) — the per-scope **bound side table** (§2 fact 1).
- `rigid_self_var` ([:512](./crates/baml_compiler2_tir/src/builder.rs:512)) and `infer_call_bindings_rigid_self` ([:10545](./crates/baml_compiler2_tir/src/builder.rs:10545)) — the Self-pinning machinery for the "exactly one `Self` param ⇒ dynamic dispatch allowed" rule (TYPE_SYSTEM.md §`Self`).
- `registry_package_for_interface_check` ([:14646](./crates/baml_compiler2_tir/src/builder.rs:14646)) — picks *which package's* registry to query (the orphan rule means the answer lives in either the type's or the interface's package).
- `lower_generic_param_bounds` ([:14992](./crates/baml_compiler2_tir/src/builder.rs:14992)) — lowers `<T extends I>` bound `TypeExpr`s to `Ty`, used both during inference and when building rules.
- The 15+ `interface_closure_locs*` call sites (member resolution: a method/field on an interface-typed or bounded-type-var receiver is found by walking the `requires` closure).

### 3.7 `package_interface.rs` — the export boundary

[`package_interface.rs`](./crates/baml_compiler2_tir/src/package_interface.rs) builds the
`PackageInterface` (exported types/functions, fully resolved) and the
`PackageResolutionContext` — the single entry point for cross-package name resolution
(ARCHITECTURE.md §"Package Resolution Context"). Two Salsa queries: `package_interface`
([:304](./crates/baml_compiler2_tir/src/package_interface.rs:304)) and
`package_resolution_context` ([:545](./crates/baml_compiler2_tir/src/package_interface.rs:545)).
Interfaces are exported here so dependents can resolve `dep.SomeInterface` and check impls
against dependency interfaces (needed for the cross-package coherence pass).

---

## 4. Salsa wiring

The TIR layer defines **exactly these tracked queries** (`grep salsa::tracked`):

| Query | Key | Returns | Role |
|---|---|---|---|
| `package_implements_registry` ([interfaces.rs:1394](./crates/baml_compiler2_tir/src/interfaces.rs:1394)) | `PackageId` | `&ImplementsRegistry` | **The** impl registry. Built once per package; read by builder, associated_projection, MIR, LSP check. |
| `package_coherence_diagnostics` ([interfaces.rs:1721](./crates/baml_compiler2_tir/src/interfaces.rs:1721)) | `PackageId` | `&Vec<CoherenceViolation>` | Per-package overlap diagnostics; consumed only by `check.rs`. |
| `infer_scope_types` ([inference.rs:1152](./crates/baml_compiler2_tir/src/inference.rs:1152)) | `ScopeId` | `&ScopeInference` | Main per-scope inference (cycle-aware). Drives `builder.rs`; reads the registry transitively. |
| `package_interface` ([package_interface.rs:304](./crates/baml_compiler2_tir/src/package_interface.rs:304)) | `PackageId` | `&PackageInterface` | Exported, fully-resolved interface. |
| `package_resolution_context` ([package_interface.rs:545](./crates/baml_compiler2_tir/src/package_interface.rs:545)) | `PackageId` | `&PackageResolutionContext` | Single name-resolution entry point. |
| `resolve_class_fields` / `resolve_type_alias` ([inference.rs:2410](./crates/baml_compiler2_tir/src/inference.rs:2410), [:2461](./crates/baml_compiler2_tir/src/inference.rs:2461)) | item loc | structural data | Per-item field/alias resolution. |
| `callable_throws` ([callable.rs:358](./crates/baml_compiler2_tir/src/callable.rs:358)) | — | throws set | Effect inference (cycle-aware). |

**Salsa observations relevant to the nuke:**

1. **The registry is keyed by `PackageId` only** — coarse-grained. Any edit to *any* file in a
   package invalidates the whole `package_implements_registry`, which re-walks every class and
   every `implements_for` block in the package. This is the [decoupled-for-runtime-eval] tension:
   coarse but correct. If you redesign, decide whether to keep per-package or go finer.
2. **`type_implements_with_deps`** ([interfaces.rs:1374](./crates/baml_compiler2_tir/src/interfaces.rs:1374))
   is a *plain function*, not tracked — it fans out across `package_dependency_closure` (an HIR
   query) and calls `package_implements_registry` per dep. Cheap because the per-package results
   are memoized.
3. **The matcher, coherence engine, closure walkers, substitution, and `is_subtype` are all plain
   functions** invoked *inside* the tracked queries. They are not independently memoized — they
   re-run whenever their enclosing query does. So Salsa granularity for interface reasoning is
   entirely determined by the five queries above; the rule logic itself is not Salsa-aware.
4. **No interned interface-impl structs.** `InterfaceImplRule` is a plain owned struct stored in
   the registry value. Identity comparisons use `QualifiedTypeName` + structural `Ty` equality,
   not Salsa ids. (HIR provides interned `InterfaceLoc`/`ClassLoc`/`PackageId`; the TIR consumes
   them but interns nothing new.)
5. **`db` trait chain:** `tir::Db : ppir::Db : hir::Db : parser::Db : workspace::Db`
   ([lib.rs:47](./crates/baml_compiler2_tir/src/lib.rs:47)). The registry query depends on
   `ppir::package_items` (PPIR-expanded symbols), `hir::file_item_tree`, `hir::file_package`,
   and `hir::package_dependency_closure`.

---

## 5. Upstream inputs (AST / HIR) — what the rule logic consumes

Interface syntax stays **unresolved** until the TIR. The relevant carriers:

**AST** ([`baml_compiler2_ast/src/ast.rs`](./crates/baml_compiler2_ast/src/ast.rs)):
`InterfaceDef` (generic_params + `generic_param_bounds: Vec<Option<TypeExpr>>` + `requires:
Vec<SpannedTypeExpr>` + `associated_types: Vec<AssociatedTypeDef>` + required/default methods),
`ImplementsBlockDef` (in-body), `ImplementsForDef` (out-of-body, with its own generics+bounds),
`AssociatedTypeDef {bound, default}`, `AssociatedTypeBindingDef {name, type_expr}`, and
`TypeExpr::Path { generic_args, associated_type_bindings }`.

**HIR** ([`baml_compiler2_hir/src/item_tree.rs`](./crates/baml_compiler2_hir/src/item_tree.rs)):
mirrors the AST but stores method **IDs**, not bodies — `Interface` (
[:220](./crates/baml_compiler2_hir/src/item_tree.rs:220)), `ImplementsBlock`, `ImplementsFor`,
and the two `method_to_iface_*` maps ([:400](./crates/baml_compiler2_hir/src/item_tree.rs:400))
that link an impl method back to its interface + assoc bindings (so the TIR can resolve
`default.foo()` and assoc refs inside method bodies). `Self` is **not** an HIR node — it's
resolved by the enclosing `ScopeKind::Class` scope name. The registry query
(§4) reads `file_item_tree(...).classes/interfaces/implements_for` directly.

Everything the rule logic needs is therefore *already* present as unresolved `TypeExpr`s the
moment you enter the TIR — the nuke does not require touching HIR, only re-deciding how those
`TypeExpr`s become rules and get matched.

---

## 6. Downstream consumers (MIR / Emit / VM) — the boundary contract

**Critical:** the TIR does **not** annotate calls with a resolved impl. MIR **re-queries**
`package_implements_registry` and `type_implements_interface_via_rule` itself
([`mir/lower.rs:832`](./crates/baml_compiler2_mir/src/lower.rs:832),
[`:3697`](./crates/baml_compiler2_mir/src/lower.rs:3697)) to decide dispatch shape:

- **Static `Call`** — receiver's concrete type & impl known at lower time.
- **Closed-world type-tag `Switch`** — finite known implementor set.
- **Open-world `VirtualCall`** ([`mir/ir.rs`](./crates/baml_compiler2_mir/src/ir.rs)) — receiver
  is interface-typed / a bounded type var / `Self` in a default body. Carries `{ iface, method,
  ntypeargs, … }`; the receiver is the first value arg.

Generic **type arguments are passed as runtime values**: MIR emits `Rvalue::LoadType(template)`
where templates contain `TypeArgRef(n)` into the enclosing frame's `type_args`; Emit pushes them
as `Object::Type`; the VM's `LoadType` substitutes against `frame.type_args`.

**Final impl selection happens at runtime**, by a *separate* resolver that mirrors the TIR
matcher: `resolve_implements_rule` ([`bex_vm/src/package_baml/resolve.rs`](./crates/bex_vm/src/package_baml/resolve.rs))
over `RuntimeImplRule` ([`bex_vm_types/src/types.rs`](./crates/bex_vm_types/src/types.rs)). It
does the same `rule_applies` unification + bound discharge + arg selection, bounded by
`MAX_OBLIGATION_DEPTH`. This is the [canonical-impl-resolver] memo's "runtime vtable" target.

**Implication for the nuke:** there are **two matchers** (TIR `match_ty_pattern` and runtime
`rule_applies`) that must agree. Whatever you build to replace the TIR matcher should share a
representation with — ideally generate — the `RuntimeImplRule` the VM consumes, or the two will
drift. The [baml_type-subenum-rework] / [canonical-impl-resolver] memos already plan to converge
these on `baml_type`.

---

## 7. The diagnostics layer (`baml_lsp2_actions::check`)

All user-facing interface validation lives **outside** the TIR crate, in
[`baml_lsp2_actions/src/check.rs`](./crates/baml_lsp2_actions/src/check.rs). It *calls* the TIR
registry/coherence queries but owns the error wording. Major entry points:

- `check_interfaces` ([:435](./crates/baml_lsp2_actions/src/check.rs:435)) — top-level interface validation.
- `orphan_check` ([:4212](./crates/baml_lsp2_actions/src/check.rs:4212)) — RFC-2451-style orphan rule (E0139).
- `validate_implements_for` / `validate_class_implements` ([:4235](./crates/baml_lsp2_actions/src/check.rs:4235), [:4648](./crates/baml_lsp2_actions/src/check.rs:4648)) — well-formedness of impl blocks.
- The coherence consumer ([:610](./crates/baml_lsp2_actions/src/check.rs:610)) iterates `package_coherence_diagnostics` (E0132).
- A large family of `validate_associated_type_*` functions ([:1268](./crates/baml_lsp2_actions/src/check.rs:1268)–[:3233](./crates/baml_lsp2_actions/src/check.rs:3233)) for assoc-type binding/default/projection checks.
- `interface_has_cycle` ([:726](./crates/baml_lsp2_actions/src/check.rs:726)) — `requires` cycles (E0118).

Diagnostic codes in play: **E0112, E0116, E0118 (requires cycle), E0119, E0120, E0121, E0125
(missing required parent impl), E0131, E0132 (coherence overlap), E0133 (requires non-interface),
E0136, E0138 (impl subject not concrete), E0139 (orphan rule).**

**Implication for the nuke:** the *checks* you most want to preserve are specified here, not in
`interfaces.rs`. If you replace the rule engine, you must keep feeding these validators the same
facts (overlap verdicts, orphan judgments, bound-satisfaction, valid-impl-subject). This is the
de-facto behavioral contract / regression surface — drive it from the snapshot tests in
`baml_tests` (phase `05` diagnostics).

---

## 8. Relationship summary (one diagram)

```
        AST TypeExprs (unresolved)
                │  lower_type_expr.rs  (+ Self substitution)
                ▼
        baml_type::Ty  ◄──────── generics.rs (substitute / infer_bindings / erase)
                │
   ┌────────────┴───────────────────────────────────────────────┐
   │  package_implements_registry  (Salsa, per-package)          │
   │     • InterfaceImplRule[]  + index                          │
   │     • requires-closure (interface_requires)                 │
   │     • [legacy compat views]  ← derive_compatibility_views   │
   └────────────┬───────────────────────────────────────────────┘
                │ matcher: match_ty_pattern / type_implements_interface_via_rule
                │ closure: interface_closure_locs_with_args_and_assoc
                ▼
   builder.rs  ──is_subtype (system 3) ──┐        associated_projection.rs
      │  NormalizeCtx (TypeContext) ──────┼──────► resolve_deep / resolve_projection
      ▼                                   ▼
   baml_type::normalize (system A)   tir::normalize (system B, legacy)
      │
      ├─► package_interface / package_resolution_context  (export)
      ├─► package_coherence_diagnostics ──► check.rs (E0132 …)  [overlap engine]
      ▼
   infer_scope_types  ──► MIR re-queries registry ──► VirtualCall ──► runtime resolve_implements_rule
```

---

## 9. Demolition checklist — what to keep, move, delete

**Keep / treat as the spine:**
- `InterfaceImplRule` + `InterfaceImplOrigin` as the single impl representation.
- The **overlap/coherence engine** (`impls_overlap`, unification, `cover_search`, `nf`, the
  fail-closed budget) — relocate, never delete; it is the only copy of NP-hard-aware ACI logic.
- The `TypeContext` trait + `baml_type::normalize` as the canonical algebra; grow it, don't fork it.
- The `requires`-closure walkers (or a replacement that still carries args + assoc bindings).
- The runtime `RuntimeImplRule` / `resolve_implements_rule` *contract* (MIR/VM depend on it).

**Move (coupled to the rule system, must travel with it):**
- `associated_projection.rs` (consumes the registry + closure walkers).
- The `NormalizeCtx` adapter (the seam to the canonical algebra).

**Delete / collapse (the redundancy that motivates the nuke):**
- The **legacy compatibility views** (`class_implements`, `type_implements`,
  `blanket_class_implements`, `implements_type_args`, `derive_compatibility_views`) once callers
  use `type_implements_interface_via_rule`.
- **`builder.rs::is_subtype`'s interface special-cases** — fold into `baml_type::normalize::is_subtype`
  behind a complete `TypeContext`.
- **`tir::normalize` (system B)** — subsume into `baml_type::normalize` (system A).

**Watch out for:**
- **Two matchers must agree** (TIR `match_ty_pattern` ↔ runtime `rule_applies`). Share or
  co-generate.
- **The bound side table** (`generic_param_bounds`) is the only place a type var's bound lives;
  any new representation must thread it (or move bounds onto the rule/signature) or `T extends I`
  reasoning silently breaks.
- **`Self` is textual, not a `Ty` variant.** The associated-projection-preserving substitution is
  load-bearing.
- **Coarse Salsa keying** (`PackageId`) — a redesign is the moment to reconsider granularity, but
  finer keys interact with the runtime-eval decoupling constraint.
- **Diagnostics (E0112–E0139) are the behavioral contract**, defined in `check.rs`, validated by
  `baml_tests` phase-05 snapshots. Use them as the regression net.
