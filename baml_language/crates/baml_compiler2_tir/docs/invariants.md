> **Provenance:** generated and adversarially verified by the multi-agent semantic-survey workflow at commit `b9c5d7c0e` (the pre-semantic-pass state of the crate), then used as the frozen contract for the 16 semantic refactors in this PR. Representation-level details (S-invariants, struct layouts like `TyAttr`) have evolved as those refactors landed; the behavioral F-invariants still hold and are enforced by the insta snapshot suite.

# TIR refactor invariants (`baml_compiler2_tir`)

The contract the de-slop / rewrite executor is held to. Derived from the model (`model.md`) and the adversarially-verified verdict register (every claim re-checked against the code). The crate is **slow AND too big**; refactors may freely change representation and delete redundancy, but **behavioral equivalence is the hard constraint**: 131 in-crate unit tests + ~1,234 insta snapshots + 4,991 workspace tests must stay green with ZERO snapshot drift.

Three sections:
1. **FROZEN CORE INVARIANTS (F1…)** — verified behavioral semantics NO refactor may change. Typing/subtyping rules, which diagnostics fire and their exact text/ordering, normalization semantics, exhaustiveness semantics, key-string shapes, anything snapshot-visible.
2. **STRUCTURAL INVARIANTS (S1…)** — true today but legitimately changeable by a refactor (internal representations, field duplication, perf indices, code organization). Listed so the executor knows what they MAY rewrite — and what behavior to preserve while doing so.
3. **CORRECTED / FALSE CLAIMS** — register claims the verification corrected, with what is actually true. Bugs to preserve, not fix.

Rule of thumb: if a property is observable in a snapshot or a test assertion, it is **F** and must be preserved bit-for-bit. If it is an implementation detail (which struct holds a map, whether an index exists, how many tuple elements `finish()` returns) it is **S** — change it freely, but keep the F-level behavior it currently produces.

---

## 1. FROZEN CORE INVARIANTS (F)

### Subtyping & normalization semantics (`normalize.rs`, `builder::is_subtype`)

- **F1. Subtyping is purely structural with NO numeric coercion.** Literal subtype arms match only same-representation base (Int-lit <: Int, Float-lit <: Float, Bigint-lit <: Bigint, String-lit <: String, Bool-lit <: Bool). There is NO (Int,Float)/(Int,Bigint)/(Float,Bigint) arm; all such pairs are `false`. Locked by `test_int_not_subtype_of_float`, `test_int_not_subtype_of_bigint`, `test_literal_widens`. (normalize.rs:323-331,361)

- **F2. Sentinel subtyping ordering is semantic and exact.** Early-returns in order: `Never` → true (bottom); `other == BuiltinUnknown` → true (top, at 215); `self == BuiltinUnknown` → false (220, top is NOT a subtype of anything specific); `Void`/`Type` on either side → self-only (226-230); `Unknown`/`Error` on either side → true (bidirectional, 233-237). Reordering changes BuiltinUnknown-vs-Unknown outcomes. (normalize.rs:202-237)

- **F3. Union subtyping is asymmetric and ORDER-DEPENDENT.** `Union(types) <: other` (ALL members) MUST be matched before `inner <: Union(types)` (ANY member). The counterexample `Union<int,string> <: Union<int,string,float>` breaks if reordered. Union membership is checked structurally; subtyping never calls `canonicalize`. (normalize.rs:286-298)

- **F4. Co-inductive (equirecursive) subtyping terminates and is observable-result-stable.** An already-assumed pair returns true; scalar early-returns run before the assumptions check so they can't short-circuit an assumed pair. Mu var names match because BOTH operands are normalized with the SAME `recursive` set from a single `find_recursive_aliases` call. An alias is wrapped in `Mu` IFF in that `recursive` set; a back-edge in the `expanding` set emits `TyVar`; otherwise it inlines. NOTE: the Function arm's `return false` paths bypass `assumptions.remove`, leaving a stale assumed pair — this does NOT change the observable result (those paths already return false). A rewriter MUST preserve the result, not the literal set-state. (normalize.rs:19-22,202-247,364,543-563)

- **F5. Equality (`is_same_normalized_type`) applies `canonicalize()` to both sides; subtyping NEVER does.** Float literals canonicalize (`1.0 == 1.00 == 1e0`). (normalize.rs:31-38,127-185,370-376)

- **F6. EvolvingList/EvolvingMap are subtype-equivalent to List/Map** (normalize collapses both to the same StructuralTy). RustType and Future erase to StructuralTy::Unknown (opaque, compatible with everything); Type maps to Type. Tests `test_evolving_list_subtype_of_list`, `test_evolving_map_subtype_of_map`. (normalize.rs:568-574,602,604,609)

- **F7. Freshness is irrelevant to subtyping/equality.** `Literal(v,Fresh)` and `Literal(v,Regular)` are assignment-equivalent (StructuralTy::Literal drops Freshness). Test asserts `Fresh(1) <: Regular(1)`. (normalize.rs:327-331; ty.rs:380-389)

- **F8. `builder::is_subtype` is the ONLY subtype entry point** and layers BEP-044 nominal rules on top of structural `normalize::is_subtype_of` in this exact order: typevar-bound, union-of-subtypes, class<:interface via registry, interface<:interface, optional/union nominal cases, then falls through to `normalize::is_subtype_of`. 43 call sites. (builder.rs:9742-9828)

- **F9. Class <: Interface is PURELY NOMINAL.** `Class T <: I` iff `I` is reachable through an explicit `implements` rule for T (via `class_implements`/the rule index). No structural/shape-matching escape hatch. (interfaces.rs:148-152,233+; builder.rs:9762-9774)

- **F10. Interface <: Interface** iff A==B (with arg-by-arg equivalence) OR B is in A's transitive `requires` closure (with matching args). Two unrelated interfaces sharing an implementor are NOT subtypes. `interface_requires[I]` contains I itself plus the full transitive set; cycles terminate. (builder.rs:9780-9798; interfaces.rs:1101-1134)

- **F11. Array<T>/Map<K,V> builtin classes bridge to structural List/Map** only when the class is builtin-root `Array` (arity 1) / `Map` (arity 2) AND arity matches, in BOTH the subtype bridge and `infer_bindings_inner`. (generics.rs:457-467; builder.rs:1636-1652)

- **F12. QualifiedTypeName identity (package + namespace + name) is the comparison key everywhere.** Same-simple-name types/interfaces in different namespaces never accidentally match. Test `match_ty_pattern_uses_full_qualified_type_names`. (interfaces.rs:148-152,538-540)

- **F13. `bind_type_var` enforces repeated type-var consistency:** `T` bound twice must bind to the same normalized type, else the match fails (returns None). Test `match_ty_pattern_rejects_repeated_type_var_conflict`. (interfaces.rs:763-779)

### Cycle diagnostics (`normalize.rs`)

- **F14. An alias cycle is INVALID iff its SCC contains NO structural (List/Map) edge.** Optional, Union, and nominal type-args are pass-through and do NOT guard (only List/EvolvingList/Map/EvolvingMap set `in_structural`). (normalize.rs:711-722,768-827)

- **F15. `find_invalid_class_cycles` has NO structural-guard exemption — every SCC is an error.** A field counts as a required edge only if NOT behind Optional/List/Map; a Union edge counts only if ALL variants force the same single dep. This differs from the alias-cycle rule (List/Map suppress the class dep entirely, vs. merely guarding the alias SCC). (normalize.rs:993-1009,1058,1086,1097,1142)

- **F16. Tarjan SCC output ordering is deterministic AND snapshot-observable** (`cycle_path` strings flow into class-cycle diagnostics consumed by baml_tests). normalize.rs Tarjan sorts nodes/successors/components and rotates each cycle to its lexicographically-min element; analysis.rs Tarjan gets determinism from BTreeMap iteration (no rotate). Any rewrite must reproduce the exact emitted ordering. (normalize.rs:887-961; analysis.rs:170-210)

### Type lowering (`lower_type_expr.rs`, `generics.rs`)

- **F17. The package/namespace of a resolved type comes from the DEFINITION's file, never the referencing file** — every resolved-type arm routes through `qualify_def` (`def.file(db)` → `file_package`). (lower_type_expr.rs:601-609)

- **F18. `baml.future.Future` lowers to `Ty::Future` only with EXACTLY 2 generic args; otherwise `Ty::Class`.** `is_builtin_future` (BAML_PACKAGE + FUTURE_NAMESPACE + FUTURE_TYPE) is the single identity source. (lower_type_expr.rs:298-303,311; ty.rs:75-80)

- **F19. A bare single-segment name that fails type resolution but matches an in-scope generic param becomes `Ty::TypeVar`, never UnresolvedType.** `resolve_type` wins first (a generic param shadowing a real type name resolves as the type); the TypeVar interception precedes the enum-variant fallback and the UnresolvedType push. (lower_type_expr.rs:275-277,352-357)

- **F20. Enum-variant paths (e.g. `Status.Active`) produce `Ty::EnumVariant` ONLY if the resolved def is an Enum AND the variant name exists on that enum**; otherwise fall through to UnresolvedType (`Status.Typo` is rejected). (lower_type_expr.rs:362-381)

- **F21. `TypeIsNotGeneric` is pushed for generic args on enum/type-alias, but NOT for arity mismatches on classes/interfaces** (those get a clearer downstream diagnostic / silently lower). Generic args of a non-generic type are STILL lowered first so their own nested diagnostics surface. (lower_type_expr.rs:223-252)

- **F22. `MediaKind::Generic` lowers to `Ty::Unknown`, not a media primitive** (Image/Audio/Video/Pdf map to PrimitiveType media variants). (lower_type_expr.rs:425-434)

- **F23. A `Function` type with no `throws` clause lowers its throws slot to `Ty::Never`**, not Optional/None (the slot is a non-optional `Box<Ty>`). (lower_type_expr.rs:551-567)

- **F24. UnresolvedType "did you mean" suggestions are produced ONLY for single-segment names and are sorted**; multi-segment paths get an empty suggestions vec. (lower_type_expr.rs:388-404)

- **F25. resolve_type/resolve_value precedence (observable):** namespace-qualified → unqualified (only when ns_context empty) → package-prefixed (`root` = own package, dep-name match for deps, path.len() >= 2). First match wins. `root` is the reserved first-segment alias for the own package. (package_interface.rs:505-543,565-605)

- **F26. `resolve_value` is tried before `resolve_type`** (a value and a type with the same bare name resolve to the value); local bindings (`visible_binding_at`) are checked BEFORE package-level resolution (let/param shadows a same-named package item). (resolve.rs:69-110)

- **F27. Local-binding resolution semantics:** position-sensitive (`binding.visible_from <= at_offset`; params have no offset guard); reverse-iteration shadowing (last matching binding wins); bindings precede params in the same scope; the local walk skips intermediate Class scopes (except the starting scope); `scope_at_offset` falls back to ROOT on miss (never panics). (baml_compiler2_hir/src/semantic_index.rs:162-177,302-343,391-396)

- **F28. `substitute_ty` preserves variable scoping:** a Function's own generic params shadow same-named outer bindings (removed from the nested binding map / inserted as self-referential TypeVar), so they are NOT substituted inside that function's body. `bind_type_vars` zip-truncates surplus on either side silently. (generics.rs:37-43,93-102,243-247)

- **F29. `union_ty`/`union_of`/`dedup_and_collapse` canonical forms:** `union_of` preserves duplicates (empty→Never, singleton→bare element, never a 1-element Union); `dedup_and_collapse` flattens exactly ONE level of nested Union and dedups; `union_ty` produces a deduplicated union and collapses single-member to bare. (ty.rs:402-437; generics.rs:484-516)

### Throws / effects semantics (`throw_inference.rs`, `callable.rs`, `throws_analysis.rs`)

- **F30. A declared `throws` clause is a FIREWALL in BOTH engines.** Direct facts = declared facts (body throws ignored); outgoing call edges are NOT propagated. HIR: `has_declared_contract` skips edges; TIR: `callable_throws` returns lowered declared throws before any body walk (also in the cycle seed). (throw_inference.rs:124-126,194-200; callable.rs:349,355-357)

- **F31. Throw facts are FLATTENED leaf types.** Optional decomposes + adds Null; Union decomposes members; Literal widens to its primitive; Never/Void drop. Applied at every fact ingress. Named-callee-summary results and catch residuals are extended WITHOUT re-flattening (assumed already-flat). (throw_inference.rs:469-495; throws_analysis.rs:89,103,105,215,329; callable.rs:96)

- **F32. When a callee's throws cannot be resolved** (neither precise instantiated-function throws nor a named summary), the call conservatively contributes `Ty::Unknown`. (throws_analysis.rs:100-110)

- **F33. `spawn` body throws do NOT escape the spawning function** (only the optional `name` expr is walked). Spawn errors are captured into `Future<T,E>` and re-surfaced at `await` — and an `await` adds the future's E error to the throws set ONLY in catch-base mode (`await_adds_future_error()`, default false, true only for `CatchBaseThrowsAnalysis`). (throws_analysis.rs:311-332; builder.rs:508-510)

- **F34. In catch-base mode a nested catch is OPAQUE:** its residual is never substituted and its clause arms are not walked — only its base is walked. (`CatchBaseThrowsAnalysis` overrides `catch_residual_throws`→None, `walk_catch_clauses`→false.) Otherwise a catch substitutes its precomputed `catch_residual_throws` for the base when available, else walks the base. (builder.rs:482-484,504-506; throws_analysis.rs:213-227)

- **F35. The catch-binding suppression is a body-WIDE name-string heuristic:** a direct throw fact is dropped iff its display name equals a catch-clause binding name ANYWHERE in the body (not scope-scoped — can over-suppress on shadowing). (throw_inference.rs:267-290,447-459)

- **F36. Cross-package callee facts are folded into the caller's DIRECT facts before graph construction; only same-package callees become graph edges.** A callee is cross-package iff some dependency interface publishes that exact key. (throw_inference.rs:127-137,498-505)

- **F37. Only `Expr/Stmt::Throw` contribute direct facts; only `Expr::Call` callees contribute edges; only `FunctionBody::Expr` bodies are analyzed by the HIR pre-pass.** The Expr and Stmt matches in `collect_from_expr`/`collect_from_stmt` are EXHAUSTIVE (no wildcard) — a new variant forces a decision. let-else else-blocks, assignment targets, while/for/return sub-exprs are all walked. (throw_inference.rs:189-265; throws_analysis.rs:119-168,196-339)

- **F38. `join_throw_facts`/`callable_throws` output is canonical:** empty→`Ty::Never`, singleton→bare element, never a 1-element Union. `callable_throws` is meaningfully fact-collected only for `FunctionBody::Expr`; Builtin→Never, Missing→Unknown. (callable.rs:19-30,363-388)

- **F39. Throw-set key shape is uniform and must stay byte-identical across producers/consumers:** `throw_set_key(namespace_path, short_name)`, methods keyed `"ClassName.method"`, `self.X` targets rewritten to `ClassName.X`. Enforced by convention/duplication (the join is re-implemented in `dotted_method_key`, `callable_short_name`, and `throw_set_key`), NOT a shared constructor — drift silently misses named-throw lookups. (throw_inference.rs:101,224-242,351-358; callable.rs:122-138; builder.rs:6087-6134)

- **F40. Throws output ordering is fully deterministic** (BTreeMap/BTreeSet throughout + Tarjan over BTree-ordered edges; `iter_transitive` is Ord-stable). (throw_inference.rs:32,60-63,249-250; analysis.rs:15,63,68-70)

### Exhaustiveness / pattern semantics (`exhaustiveness.rs`, `builder.rs` pattern lowering)

- **F41. `is_inhabited` is consulted at every empty-column and every missing-ctor field; uninhabited ctors/scrutinees are pruned** (uninhabited scrutinee → vacuously exhaustive, all arms unreachable). Uninhabitedness is only ever PROVEN by reaching `Never`; class cycles default to INHABITED (anti-monotone). A witness ctor is skipped when any sub-field type is uninhabited (no `[Never, ..]` impossible witnesses). (exhaustiveness.rs:615-631,867,968-986,1091)

- **F42. `unreachable_arms` is sorted and deduplicated** (or-expansion produces multiple rows per ArmId). (exhaustiveness.rs:888-889)

- **F43. Witness field ordering matches source declaration order** despite the witness stack being built in reverse-unwind order (`apply_ctor` `.rev()`). Pinned ONLY by exact-string snapshot tests (e.g. `user.OptionalPair { true, false }` distinct from `{ false, true }`). (exhaustiveness.rs:916-928,1731-1770)

- **F44. The scrutinee + all column types fed to the matrix are normalized in the CALLER** (`matrix_normalize_scrut`: Optional→Union[T,null], aliases expanded) so UnionMember dispatch covers both branches, while the DISPLAYED diagnostic uses the un-normalized `scrutinee_ty`. `matched_ty` keeps the original (Optional) representation; the `dpat` scrut tags use the normalized form. exhaustiveness.rs never re-normalizes. (builder.rs:1218-1250,5042-5065,11256)

- **F45. List types are never enumerated via `enumerate_ctors`; coverage is via slice splitting only** (`is_list_ty` is checked before `enumerate_ctors` on every path). (exhaustiveness.rs:1056-1058,1099,1108,1130-1134)

- **F46. Exhaustiveness recursion terminates on recursive types without a depth guard** via `split_ctors` short-circuits (empty-matrix path emits synthetic `Missing` without descending; all-wildcard column passes through `NonExhaustive` without enumerating). Confirmed for `class Node { next: Optional<Node> }`. (exhaustiveness.rs:1086-1118,1161-1167)

- **F47. Ctor identity:** Single/Interface/UnionMember by `CtorIdentity` (TyAttr/Freshness-stripped, float-canonicalized String — `1.0 == 1.00 == 1e0`); Class by **qtn only** (stored type-args ignored for identity), with real field types fetched lazily from the COLUMN type's args. `Or` ctors never reach a normal `covers()`/eq check (exploded first by `split_ctors`); `Or` and `Missing` are defensively `false` in `covers()`. (exhaustiveness.rs:96-128,166,210,234-340,1072-1078,1174-1178,1302-1304)

- **F48. `Ctor::NonExhaustive` and `Ctor::Missing` are produced ONLY by the algorithm** (`enumerate_ctors`/`split_ctors`/witness construction), never by source-pattern lowering. (exhaustiveness.rs:83-88; builder.rs dpat lowering)

- **F49. `pattern_types[pat_id]` is written for EVERY pattern PatId at every recursion level** (not just bindings) because MIR's `pat_ty` reads every PatId during structural destructure lowering. `analyze_and_lower` inserts `matched_ty` at one chokepoint; `finalize_pattern_lowering` additionally inserts per-binding types; catch clauses re-insert. (builder.rs:11086-11088,11224,5417-5418,11138-11142)

- **F50. `matched_ty` is always a concrete `Ty` (never Option):** wildcard/bare-bind fall back to `scrut_ty`; unreachable arms use `Never`. `PatternResult.required_ty` is None for wildcards and bare binds (no requirement placed on the scrutinee). (builder.rs:11160,11681-11710; pattern_lowering.rs:45)

- **F51. `PatternBinding.ty` is the binding's FINAL/widened type:** an alias-bind-over-sub-pattern chain types the outer bind at the sub-pattern's narrowed type; union/or branches collapse same-named binds to the JOIN of per-branch types (not last-write-wins). Per-binding SCOPE registration is by-name (duplicate names / or-pattern alternatives collapse to the last declared), while `pattern_types` is keyed by PatId (each alternative position keeps its own type). (builder.rs:11085-11099,11313-11323,11722-11726)

- **F52. Guarded arms are excluded from coverage/exhaustiveness AND from unreachable detection** (only non-guarded arms are pushed into the matrix; report arm ids re-map through `matrix_arm_ids` to source ExprIds). (builder.rs:5036-5079)

### Control-flow narrowing (`narrowing.rs`, `builder.rs`)

- **F53. Narrowing fires only for single-segment paths referring to a known-typed local.** Truthiness narrowing applies ONLY when the type is nullable (a non-nullable `if(x)` produces no narrowing). The condition's type and all path-operand types are inferred into `expr_types` before `extract_narrowings`; a missing entry degrades to no-narrowing, not a panic. (narrowing.rs:157-167,186-197; builder.rs:3045-3048)

- **F54. Narrowing application is balanced and structural:** `apply_then` → branch infer → `restore_and_apply_else` → `restore_narrowings`, in that order. Diverging branches (let-else else) use `discard_scoped_locals` (HARD rollback — discards even outer-binding writes); joining branches (if/if-let/match arms) use `restore_scoped_locals` (merge). `saved.get(name)==None` (absent) → restore removes the local. (narrowing.rs:346-377; builder.rs:844-850,852-920,4541)

- **F55. `subtract_pattern_type` is conservative and never widens beyond the scrutinee:** it keeps a member when a shape can't be decomposed; full subtraction returns `Ty::Never` (via `union_of([])`), NOT the original. Structural shape comparison (`ty_shape_eq`) intentionally ignores TyAttr and literal Freshness (because `Ty`'s derived PartialEq is attr-sensitive). (narrowing.rs:250-313)

- **F56. Lexical scope exit restores shadowed locals to the state immediately before the SHADOWING declaration (not scope entry), propagates outer-binding assignments, and drops inner-shadow assignments** (the "Slack rules", keyed by PatId binding identity). Function/lambda params and captures carry `pattern: None` and ALWAYS propagate. (builder.rs:852-920,938,957)

- **F57. `in_optional_chain > 0` auto-unwraps nullable bases on FieldAccess/Index; at 0 a member/index on a nullable base is a `NullableMemberAccess` error.** The counter is always balanced around the chained region. (builder.rs:3218-3220,3313,3439,4714-4775)

### Diagnostics (text, ordering, suppression — all snapshot-visible)

- **F58. Diagnostics are append-only, frozen after `InferContext::finish()`, NOT deduped or sorted in-crate; ordering is source-walk order and is observable** (pinned by insta snapshots). External dedup in `check.rs` removes only CONSECUTIVE duplicates, relying on emission order placing dups adjacent. (infer_context.rs:968-998; inference.rs:1620-1624; baml_lsp2_actions/src/check.rs:352-354)

- **F59. Warning severity is attached ONLY via `report_warning`; all other `report*` paths produce Error.** `DiagnosticSeverity::Warning` appears exactly once in the crate. (infer_context.rs:997,1057-1059)

- **F60. The synthesized-code suppression flag drops ONLY the 4 kinds `UnresolvedMember`/`UnresolvedType`/`UnresolvedName`/`NotCallable`, ONLY at Error severity, ONLY while `is_auto_derived_body` is set** (for synthesized to_json/from_json bodies). `report_warning` bypasses the guard. (infer_context.rs:932-940,994; builder.rs:1060-1063)

- **F61. Every `TirTypeError` variant has BOTH a `Display` arm AND a `tir_type_error_to_diagnostic_id` arm; both matches are EXHAUSTIVE (no wildcard) across crate boundaries.** Adding a variant fails compilation in both places. (infer_context.rs:297-718; baml_lsp2_actions/src/check.rs:3545-3613)

- **F62. `render()` computes the message eagerly via `Display`, but ~13 type-bearing variants are OVERWRITTEN downstream by the file-aware `source_aware_tir_type_error_message` re-render (LSP path).** `RenderedTirDiagnostic.error` is retained verbatim so downstream can recompute the DiagnosticId and a source-aware message. The verbatim Display message is live for snapshot tests / non-overriding consumers — both message channels are snapshot-observable. (infer_context.rs:827-828; baml_lsp2_actions/src/check.rs:3411,3434-3539)

- **F63. `TirTypeError` is span-free / Salsa-stable** (stores only arena IDs and value payloads; the only `TextRange` lives in `DiagnosticLocation::Span`). `render()` produces a correct TextRange only when given the AstSourceMap for the SAME body that owns the referenced ids; mismatch silently yields empty/wrong spans via `unwrap_or_default()`. (infer_context.rs:23-24,732-768,790-833; inference.rs:1593-1618)

- **F64. Diagnostics generated while building the package interface / implements registry are intentionally DISCARDED** (registry/interface build is diagnostic-free); correctness diagnostics are emitted elsewhere by re-lowering during inference. (package_interface.rs:239,302,331,651; interfaces.rs:929,983,1066)

### Rendering (`ty.rs` — every choice is snapshot-observable across 3 strategy implementors)

- **F65. ALL `Ty`→text rendering funnels through `Ty::render_with` + a `TyRenderStrategy`; `render_with` is total over `Ty` (no wildcard).** No module re-walks `Ty` to render it. The only implementors are `CanonicalTyRender` (Display + render_user_facing) and the two LSP strategies. (ty.rs:643-744,757,778-785; baml_lsp2_actions/src/utils.rs:168,198)

- **F66. `is_local()`/`render_user_facing` elide the implicit `user` package; canonical `Display` keeps it** (`user_facing && self.is_local()`). (ty.rs:88-108,615-616,783)

- **F67. `BuiltinUnknown` and `Unknown` are semantically distinct (top vs error sentinel) but BOTH render the string `"unknown"`.** Any diagnostic needing to tell them apart must not use the rendered string. (ty.rs:736)

- **F68. `needs_postfix_parens` groups Union and Function under postfix `[]`/`?`**; function-return position parenthesizes on Function only (to keep the outer throws clause associated with the outer callable). (ty.rs:621-637,729)

- **F69. `humanize_type_string` (LSP display only) rewrites `SYNTHETIC_EFFECT_PARAM_PREFIX` + a leading digit run to `"callback"`**, requires at least one digit after the prefix (a bare prefix is left as-is), and uses a deliberately LOOSER substring digit-match than `ty::is_synthetic_effect_param` (which requires the entire remainder to be digits). It is a pure total function on the already-rendered string. (user_facing.rs:8-26)

- **F70. `widen_fresh` only removes Fresh-literal specificity (no-op on Regular literals); `make_evolving` only converts `List(Never)`→EvolvingList and `Map(Never,Never)`→EvolvingMap.** EvolvingList/EvolvingMap are created only at unannotated mutable `let` sites from empty containers; reading the variable yields the frozen List/Map (MIR freezes Evolving*→frozen). (ty.rs:508-555; baml_compiler2_mir/src/lower.rs:362-369)

### TIR-only variant erasure at terminal sinks (snapshot-observable downstream output)

- **F71. Freshness never escapes TIR** — MIR and codegen both discard it. (No `Freshness` reference exists outside `baml_compiler2_tir/src`.) (baml_compiler2_mir/src/lower.rs:359; baml_project/src/client_codegen.rs:606)

- **F72. `TypeVar` must be erased before MIR/runtime; any survivor is error-recovery** (general TypeVar→Void; TStream/TFinal→BuiltinUnknown exception). Earlier erasure to Unknown emits a `CannotInferTypeParameter` diagnostic. (baml_compiler2_mir/src/lower.rs:429-434; ty.rs:226-233)

- **F73. `Interface` is a compile-time-only distinction; it collapses to a nominal `Class` at every downstream boundary** (MIR and codegen). Only `StructuralTy` keeps it distinct for nominal subtyping. (baml_compiler2_mir/src/lower.rs:314-320; baml_project/src/client_codegen.rs:560)

### Salsa-correctness / equality (refactors must preserve these or get spurious recompute / drift)

- **F74. `ScopeInference` / `ResolvedClassFields` / `ResolvedTypeAlias` / `FunctionThrowSets` equality (`PartialEq` via `impl_partial_eq_salsa_update!`) is the Salsa early-cutoff signal.** Refactors MUST preserve PartialEq semantics — a recompute that produces an equal value must NOT invalidate downstream. (inference.rs:44-64,300,1461,1471; throw_inference.rs:36)

- **F75. `resolve_class_fields` / `resolve_type_alias` are pure functions of their Loc; `infer_scope_types` is pure per-scope** (per function body / lambda body / top-level let — editing a lambda re-runs only that scope). (inference.rs:683-691,1476,1527)

- **F76. A Function scope is uniquely identified by `(scope.range == func_data.span) AND (scope.name == func_data.name)`** — name alone is insufficient because companion/template functions share a span. Same dual-key at all four sites. `function_scope_id` uniqueness of the triple is assumed; `find()` takes the first match. (inference.rs:719-725,1173-1176,1258-1259,1601; callable.rs:149-154)

- **F77. A Lambda scope's body is reached by walking ancestors to the enclosing Function/Let and matching the lambda by source span** (relies on globally-unique lambda spans within the body). A lambda's contextual parameter types live in the PARENT's `ScopeInference.nested_lambda_types` under the lambda's FileScopeId; absent → defaults to Unknown. (inference.rs:470-497,617-626,1075-1302)

---

## 2. STRUCTURAL INVARIANTS (S)

True today; legitimately changeable by a refactor. The executor MAY rewrite these — but must keep the F-level behavior they currently produce. Each notes the behavior to preserve.

### Representation duplication (prime deletion targets)

- **S1. The ~10–12 per-body inference maps are physically duplicated across FOUR structs** — `TypeInferenceBuilder` live fields, `SavedInferenceState` (12 map fields), `DefaultParameterInference` (10 fields), `ScopeInference` (14 fields) — and enumerated by hand in `take/restore_inference_state` and `check_function_parameter_defaults`. A shared `InferenceMaps` newtype could collapse all of these. PRESERVE: the exact set of maps that end up in `ScopeInference` and the body/default split semantics. (builder.rs:382-395,791-826,1067-1102; inference.rs:147-225)

- **S2. `finish()` returns an unnamed positional 14-element tuple** that the cross-crate LSP consumer (`check.rs`) also destructures, alongside public setters `add_local`/`param_types`/`set_generic_params`/`check_function_parameter_defaults`. Changing the tuple shape breaks the LSP crate — co-update both. (builder.rs:1067-1102; baml_lsp2_actions/src/check.rs:286-321)

- **S3. `DefaultParameterInference` is a near-exact clone of 10 `ScopeInference` fields** kept separate only because parameter-default exprs live in a DIFFERENT AST arena. MIR UNDOES the split, re-merging both halves into one map differing only by `MetadataScope::Body` vs `ParameterDefault`. Replaceable with one arena-tagged key. PRESERVE: default and body ids never collide / never merge into the same map before MIR. (inference.rs:203-225; baml_compiler2_mir/src/lower.rs:1623-1680)

- **S4. The three `path_*` maps (`path_root_types`/`path_segment_types`/`path_member_resolutions`) are partly-redundant views of one multi-segment path.** PRESERVE: the per-segment lookups MIR/LSP perform (including the `path_member_resolutions` shortness caveat — see corrected C7). (inference.rs:147-211)

- **S5. `builder.scope` DUPLICATES `InferContext.scope`; `is_auto_derived_body` DUPLICATES `suppress_member_lookup_errors`.** Nothing ties either pair; both are latent merge candidates. PRESERVE: they currently hold the same value for the whole run. (builder.rs:601,1060-1063,6430,10832)

- **S6. `pattern_types` is an overloaded field with two unrelated meanings keyed by PatId** (Bind-variable type vs Type/Class-pattern runtime-test type); no type-level discriminant. A refactor splitting it must keep both meanings reachable by the existing LSP/MIR consumers. (inference.rs:150-153; builder.rs:604-613)

- **S7. `resolutions` is both a persistent ExprId→resolution table AND a transient scratch slot** (the per-segment loop removes the entry right after reading). The Vec is NOT index-parallel to segments. (builder.rs:6394-6403)

- **S8. `nested_lambda_types` and `param_types` are NOT consumed by MIR.** `nested_lambda_types` has no consumer outside the producing query (intra-query scratch, intentionally NOT saved/restored across `infer_lambda_body` so nested-lambda types bubble to the outermost scope); `param_types` is LSP-only. (builder.rs:715-719; baml_lsp2_actions completions.rs:1303)

### Parallel enums / structural mirrors

- **S9. `StructuralTy` is a private ~80%-copy of `Ty`** (aliases resolved, recursion explicit via Mu/TyVar, attrs/freshness stripped). It never escapes `normalize.rs` (no cross-crate/module reference). Collapsing it into `Ty`-with-resolved-aliases + an external recursion guard is the highest-leverage / highest-risk lever. PRESERVE: F1-F7 subtyping/normalization semantics exactly. (normalize.rs:62,120)

- **S10. `DPat` ≅ `WitnessPat`** (structurally identical `{ctor, fields, ty}`); unify into one `Pat`, keeping the two Display impls as the only fork. `WitnessStack`/`WitnessMatrix` are thin Vec newtypes. `CtorIdentity(String)` exists only because `Ty::PartialEq` is span/freshness-sensitive. PRESERVE: F43/F47 witness ordering + ctor identity. (exhaustiveness.rs)

- **S11. `ClassCycleInfo.cycle_path` is fully redundant with `.members`** (`format_cycle_path(members)`). Derive one from the other; PRESERVE F16 ordering. (normalize.rs:1001-1006,1152-1160)

- **S12. TWO Tarjan + NodeState impls** (`normalize.rs` HashMap+sort+rotate vs `analysis.rs` generic BTreeMap, no rotate). Field-identical `NodeState`. Unifiable if the caller supplies the determinism post-pass. PRESERVE F16/F40 ordering. (normalize.rs:887-961; analysis.rs:170-210)

- **S13. `TyAttr` is inert plumbing inside TIR** — always `TyAttr::default()` (~585 sites), zero non-default assignments, read exactly once at the MIR boundary (`convert_tir2_ty`), `with_attr` has zero in-crate callers. Every `Ty` variant carries exactly one `TyAttr`, with `attr()`/`with_attr()` exhaustive and in sync. (ty.rs:441-498; baml_compiler2_mir/src/lower.rs:283)

### Throws engine duplication

- **S14. TWO (three) parallel throws engines** computing the same per-function escaping-throw notion: HIR-level `function_throw_sets` (Name-keyed, `analysis.rs` graph fixpoint), TIR-level `callable_throws` (FunctionLoc-keyed Salsa query), and a 3rd inline impl in `builder.rs`. Both feed codegen `Raises:`. The `analysis.rs` framework (211 lines, generic, SINGLE instantiation) can be deleted/inlined if the engines merge. CAUTION: they DIVERGE on union-typed callees (see corrected C6) — verify equivalence before merging. (throw_inference.rs; callable.rs; builder.rs)

- **S15. `PackageInterface.throw_sets` is an exact clone of `function_throw_sets(pkg)`** — the same data is reachable two ways. (package_interface.rs:407)

- **S16. The 3 `ThrowsAnalysisContext` trait toggles (`catch_residual_throws`/`walk_catch_clauses`/`await_adds_future_error`) encode essentially one mode bit** (normal vs catch-base). PRESERVE F33/F34. (builder.rs:482-510)

### Interface-impl perf index (deletable — perf is not a constraint)

- **S17. `InterfaceImplRuleIndex` is a PURE perf index, 100% derived from `interface_impl_rules`** (every bucket value is a valid index into the rules; `by_interface[I]` = union of the by_class/by_type/fallback buckets). It can collapse to a linear scan filtered by `interface_qtn`. PRESERVE: behavior-equivalence to scanning all rules (F9) — the non-keyable path already falls back to a full scan. (interfaces.rs:74-116,254-289)

- **S18. `class_implements` is a derived projection of `interface_impl_rules`** (blanket out-of-body generic rules excluded, dispatched only via the index); `all_class_qtns`/`class_implements` keys contain every class even those implementing nothing. (interfaces.rs:409-440,914-917)

### Test-only / dead-ish surface (deletable)

- **S19. `ResolvedMethod` / `PRC::lookup_class_method` / `lookup_own_class_method` are TEST-ONLY** (sole caller is `baml_tests/src/compiler2_tir/inference.rs:570`). Production builder uses its own separate `TypeInferenceBuilder::lookup_class_method` (builder.rs:8916). Deletable (~70 lines + the type). (package_interface.rs:608,636,660)

- **S20. `ResolvedSource` is a thin tag re-encoded immediately into `ResolvedName::{Item,Builtin}`.** `ResolvedName` carries no stored map (re-derived on demand). (package_interface.rs:79-80; resolve.rs:86)

- **S21. `CallContext`/`CallCheckRequest`/`OptionalCallContext` are pure parameter bundles** — no identity, no methods, no impl blocks except derives; constructed inline and immediately destructured. (builder.rs:569,577,589)

### Recursion-walk duplication (audit before unifying — coverage has drifted: see C8)

- **S22. `generics.rs` has 4 divergent Ty walks** (`substitute_ty`/`contains_typevar`/`infer_bindings_inner`/`erase_typevars_matching`) with DRIFTED variant coverage. `normalize.rs` has 4 more (`normalize_impl`/`ty_has_cycle`/`extract_type_alias_deps`/`extract_required_class_deps`). The class-cycle and alias-cycle graph extractors are two independent hand-written walkers with overlapping-but-divergent rules (F14/F15). One generic fold could subsume the structural-descent portions — but the drift (C8) is a latent-bug audit prerequisite. (generics.rs; normalize.rs)

### Wiring / boundary facts (stable but mechanical)

- **S23. The core cluster aggregates (`PackageInterface`, `PackageResolutionContext`, `ImplementsRegistry`) are construct-once / never-mutated**, each produced by exactly one `#[salsa::tracked(returns(ref))]` query; PRC owns (clones) its `own_items` + `dep_interfaces` for incremental soundness. (package_interface.rs:218-220,449-467; interfaces.rs:887-889)

- **S24. `TirTypeError`'s `Display` (~420 LOC) and LSP `source_aware_tir_type_error_message` duplicate the message templates for ~13 type-bearing variants** (and have ALREADY drifted, e.g. MissingReturn / NonExhaustiveMatch wording). Both are snapshot-live (F62) — any unification must reproduce BOTH current outputs verbatim. (infer_context.rs:297-718; baml_lsp2_actions/src/check.rs:3443-3539)

- **S25. Index/co-indexing assumptions are unenforced (no bounds checks):** `index.scopes`/`scope_bindings`/`scope_ids` co-indexed by `FileScopeId::index()`; throws walk indexes body arenas directly; `call_plans` keyed by callee ExprId but throws looks up by a fuzzy reverse arg-set scan. A short vector panics. These hold by construction; a representation change must keep them valid. (inference.rs; throws_analysis.rs; builder.rs:2034)

- **S26. `lib.rs` is a 44-line methodless `#[salsa::db] pub trait Db: ppir::Db {}` marker** + 18 module declarations. Supertrait chain hir←ppir←tir←mir. Every concrete DB explicitly implements `tir::Db` (no blanket impl). Adding/removing the marker cannot change query behavior. (lib.rs:43-44)

---

## 3. CORRECTED / FALSE CLAIMS

Claims the verification corrected. These are facts the executor must NOT "fix" — several are intentional bugs/quirks whose observable result is relied upon.

- **C1. NOT "every sub-body caller saves/restores all six inline-handled fields."** Only the LAMBDA caller (`infer_lambda_body`) saves/restores all six (locals, scoped-local logs, body_source_map, pattern_natural_cache, declared_return_ty, generic_params). `check_function_parameter_defaults` saves/restores ONLY three (locals, scoped-local logs, body_source_map) — sound because the param-default path never WRITES the other three. Actual rule: the 12-map `take/restore_inference_state` is universal; the six inline-handled fields are saved by a caller ONLY if that caller mutates them. (builder.rs:791-826,2044-2142,10764-10897)

- **C2. The co-inductive `assumptions` set is NOT clean stack hygiene.** The pair inserted at normalize.rs:247 is removed at 364, BUT the Function arm's `return false` at 353/356 BYPASSES the remove, leaving a stale assumed pair for the rest of the enclosing traversal. The OBSERVABLE RESULT is unchanged (an assumed pair only yields `true`, and those paths returned `false` which propagates up). A rewriter must preserve the RESULT, not the literal set-state. (See F4.)

- **C3. `Ty` has 23 variants, NOT 26 (and not 24-26 as the model hedges).** The 23: Class, Interface, Enum, EnumVariant, TypeAlias, Primitive, List, Map, Union, Optional, Literal, EvolvingList, EvolvingMap, Function, TypeVar, Never, Void, BuiltinUnknown, RustType, Type, Unknown, Error, Future. `attr()`/`with_attr()` enumerate all 23 with no wildcard (a new variant fails to compile). (ty.rs:170-289,441-498)

- **C4. `is_synthetic_effect_param` is NOT the single source of truth.** The PREFIX CONSTANT (`SYNTHETIC_EFFECT_PARAM_PREFIX`) is shared, but `user_facing.rs:13-15` re-implements the prefix+digit-check LOGIC inline with deliberately LOOSER substring semantics (leading-digit-run then continue scanning, vs. `is_synthetic_effect_param`'s "entire remainder must be digits"). Both must stay in sync by convention; no shared predicate. (See F69.)

- **C5. `ResolvedName::Builtin` / `ResolvedSource::Builtin` means "from a DEPENDENCY package," NOT specifically the `baml` builtin package** — despite the doc comment. The loop iterates ALL dependency interfaces and returns `Builtin(def)` for ANY dependency that resolves the name. Naming is misleading-but-load-bearing (the dependency is usually the baml builtin). (resolve.rs:34-35,102-109; package_interface.rs:79-80)

- **C6. `callable.rs::instantiated_callee_throws` does NOT handle union-typed callees, unlike `builder.rs`.** `callable.rs:257-259` let-else returns None for any non-Function callee; `builder.rs:6048-6061` has a Ty::Union arm that unions throws across function members. A union-typed callee yields None (→ conservative Unknown) in the `callable.rs` path. REAL behavioral divergence — a naive merge of the two throws engines (S14) would change behavior. (See F32/S14.)

- **C7. `path_member_resolutions[expr]` is NOT strictly parallel to `segments[1..]`** — the inference.rs doc claims it is, but the actual builder population states the OPPOSITE: builtin/primitive members (e.g. `String.length`) don't record a `MemberResolution`, so the Vec contains resolutions ONLY for non-builtin members after the root, in order, and may be SHORTER than `segments[1..]`. Positional consumers that `.get()` degrade to None on a short Vec, but a builtin in a MIDDLE segment misaligns later segments. Accurate rule: iterate by value or use `.last()`, not index-correspondence. (inference.rs:177-184; builder.rs:6397-6402; consumers: LSP definition.rs:364, MIR lower.rs)

- **C8. `substitute_ty`/`contains_typevar`/`infer_bindings_inner`/`erase_typevars_matching` do NOT recurse over the same Ty variants — they DIVERGE.** `contains_typevar` has NO Future arm (a typevar nested in a Future is invisible → falls to `false`); `infer_bindings_inner` has no Union/Future/EvolvingList/EvolvingMap arms. `substitute_ty` and `erase_typevars_matching` DO recurse into Future/Union/Evolving*. `erase_typevars_matching` early-returns via `contains_typevar`, so it INHERITS the Future blind spot. These asymmetries are relied upon implicitly; unifying the walks must preserve each function's current (drifted) coverage or risk a behavior change. (generics.rs:60-102,363-389,397-469,524-577)

- **C9. `subtract_pattern_type`'s doc is misleading.** The doc says it "falls back when subtraction would leave an empty union," but on FULL subtraction it returns `Ty::Never` (via `union_of([])`), NOT the original scrutinee. It only returns the original when NOTHING was subtracted. (See F55.)

- **C10. `package_interface` and builder's `lookup_class_method` do NOT share a code path.** `package_interface` + the test-only `lookup_own_class_method` share `lower_class_method_signature`, but `TypeInferenceBuilder::lookup_class_method` (builder.rs:8916) is a SEPARATE richer re-implementation (binds class/method generics, builds a concrete self-type) that does NOT call the helper. Equivalence is not structurally guaranteed. (package_interface.rs:139,270,660; builder.rs:8916-8998)

- **C11. The implicit `user` package constant is BYPASSED in two test sites.** `RESERVED_USER_PACKAGE = "user"` is the intended single source, but `exhaustiveness.rs:1624` and `interfaces.rs:1254` (both inside `#[cfg(test)]`) construct QTNs with raw `Name::new("user")`. (ty.rs:139)
