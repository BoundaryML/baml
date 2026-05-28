# planv2 — fixing BAML interface & generics defects (BEP-044)

Derived from: a CLI fuzz/stress sweep (`_plan/baml_interface_findings.md`, 45 confirmed bugs),
44 failing regression tests now in `crates/baml_tests/tests/interfaces.rs`
(prefix `fuzz_bug##_…`), and a four-layer code exploration of the compiler.

> Status: all 44 `fuzz_bug*` tests **fail on `canary`** (`0 passed; 44 failed`). They are the
> acceptance criteria — each fix should flip its tests green without regressing the existing
> ~248 interface tests.
>
> Line numbers below are from the exploration snapshot and may drift slightly; treat
> file + function names as authoritative and re-grep before editing.

## How to validate

```bash
# all repro tests (should be 44 failing now; fewer as you fix)
cargo test -p baml_tests --test interfaces fuzz_bug 2>&1 | tail -30
# a single cluster, e.g. generic dispatch
cargo test -p baml_tests --test interfaces fuzz_bug08 fuzz_bug12 fuzz_bug13 fuzz_bug14
# don't regress the rest
cargo test -p baml_tests --test interfaces
# end-to-end with the CLI (manual repro dirs live in _plan/baml_wf/<category>/)
target/debug/baml-cli run --file _plan/baml_wf/generics-basic/<file>.baml main
```

## Pipeline primer

`baml_compiler_parser` → `baml_compiler2_ast` → `baml_compiler2_hir` → `baml_compiler2_tir`
→ `baml_compiler2_mir` → `baml_compiler2_ppir` → `baml_compiler2_emit` (bytecode)
→ `bex_vm` / `bex_engine` (VM).

Interface machinery, by layer:
- **HIR** `baml_compiler2_hir/src/builder.rs` (`lower_class`, ~1102–1117): flattens a class's
  methods from its `implements` blocks. Item ordering comes from `FxHashMap`s in
  `hir/src/package.rs:86` (`namespaces`) and `namespace.rs:102` (`types`) — **unordered**.
- **TIR** `baml_compiler2_tir/src/interfaces.rs` — `package_implements_registry()` (~809–1057)
  builds the `ImplementsRegistry`. Key types: `InterfaceImplRule { interface_ty: Ty::Interface(qtn, type_args, …) }`
  (type args present here), plus side maps `class_implements: Class→{Interface QTN}` and
  `implements_type_args: (Class,Interface)→Vec<Ty>` (**type args split out from identity**).
  `baml_compiler2_tir/src/builder.rs` does member resolution / subtyping / throws checking.
- **MIR** `baml_compiler2_mir/src/lower.rs` — interfaces lower to `Ty::Class` at runtime
  (~281–289); method dispatch resolved in `resolve_implementor_method_candidates()` (~6343–6417)
  and emitted as a guard switch (`emit_interface_dispatch_guard_branch`, ~6091–6270).
- **EMIT** `baml_compiler2_emit/src/lib.rs` (~904–966): builds `program.interface_implementors`.
- **VM** `bex_vm/src/package_baml/type_class.rs`: reflection (`implements`, `implemented_by`,
  `implementors`, `ty_name`). `bex_vm_types/src/types.rs:106`: the runtime
  `interface_implementors: IndexMap<TypeName, Vec<TypeName>>`.

## Three cross-cutting root causes

Almost every bug is one of these three architectural gaps. Fix the foundation and many tests
flip together.

### RC1 — Interface identity is keyed on NAME, dropping type arguments
The type args in `Ty::Interface(qtn, type_args, …)` are carried in TIR but **discarded** at the
boundaries that matter:
- emit drops them: `baml_compiler2_emit/src/lib.rs:924` pattern-matches `Ty::Interface(qtn, _, _)`;
- the runtime registry has no slot for them (`bex_vm_types/src/types.rs:106`);
- MIR dispatch guards check class identity only, not the interface type args being dispatched on
  (`InterfaceClassGuard`, `lower.rs` ~828–830 + `emit_interface_dispatch_guard_branch`).

Consequences: clusters **A** (8,9,12,13,14,27), **E** (5,20,21), **F** (34,35,36).

### RC2 — Nominal interface subtyping isn't applied in "wrapped" positions
`builder.rs::is_subtype()` (~9808–9862) only runs the implements-registry check when the
*source* is a bare `Ty::Class` and the *target* is a bare `Ty::Interface` (guard at ~9818
`&& !matches!(sub, Ty::Interface(..))`). Optional/union targets unwrap and recurse into
`normalize.rs::StructuralTy::is_subtype_of()` (~197–380), which has **no Interface arm**
(catch-all `_ => false`, ~375). The same gap exists in throws/catch matching:
`check_throws_surface()` (~1246–1337) does a raw set-difference (~1260, no subtype check), and
`ty_covers_fact()` / `ty_may_match_fact()` (~5643–5720) have no interface arm.

Consequences: clusters **D** (3,32,33,40,41,45) and **G** (38,39). The registry helper
`registry.type_implements_interface_via_rule(...)` already exists — it's just not called here.

### RC3 — Interface members live on interface-typed views, not on the concrete class; generic scope/runtime repr is incomplete
- A class's flattened method set (`lower_class`, ~1102–1117; `lookup_class_method`, ~8922–9095)
  includes only explicitly-overridden methods — **inherited defaults are absent** from the
  concrete-class namespace (cluster **I**: 17,23,42), and ambiguity detection that scans only
  `class_data.methods` therefore misses default/required clashes (cluster **J**: 25,26).
- Interface generic params are **not bound** when default-method bodies are checked, so `T` is
  "unresolved" (`set_generic_params` never receives interface generics; cluster **B**: 7,16,24,44).
- At runtime an interface value reaching a monomorphized generic-fn param is treated as a map
  but arrives as an instance → `expected map, got instance` crash (cluster **C**: 10,15,43).

## Cluster-by-cluster fix plan

### A 🔴 Generic-interface dispatch ignores type args  → tests 8,9,12,13,14,27
Root: RC1. Two `implements Getter<L>` / `Getter<R>` blocks both register under interface name
`Getter`; the MIR dispatch guard picks the first block regardless of the requested instantiation,
and `.as<Getter<string>>` does too. `#9/#27` are the diagnostic face: unqualified call should be
E0121 instead of silently picking the first.
- **Fix**: thread interface type args into dispatch. Extend `InterfaceClassGuard`
  (`mir/lower.rs` ~828–830) to carry the interface type args alongside the class args; build the
  guard in `interface_class_guard_for_args()` (~212–252); compare them in
  `emit_interface_dispatch_guard_branch()` (~6243). Make
  `implements_target_matches_requested_views()` (~6909–6944) match on `(name, type_args)` not name.
- For 9/27: in `builder.rs` member resolution, when ≥2 implements blocks of the *same* interface
  name with *different* type args satisfy an unqualified call, raise **E0121** (see cluster J).
- **Risk**: medium — touches the dispatch switch; guard against monomorphic (non-generic)
  interfaces still using the cheap `Any` guard.

### B 🔴 Interface type param `T` unscoped in default-method bodies  → tests 7,16,24,44
Root: RC3. Default-method bodies are resolved for signature but never type-checked with the
interface's generics in scope. `resolve_interface_member()` (`tir/builder.rs` ~7893–8050) builds
bindings for interface generics (~7995) but only for signature resolution.
- **Fix**: when lowering/checking an interface default-method body, call `set_generic_params(...)`
  with the interface's generic params merged with the method's own. Likely a dedicated check pass
  invoked after `package_implements_registry()` (in `tir/lib.rs` or `inference.rs`), or pass the
  interface generics through to the body builder at its construction site.
- **Risk**: medium — ensure the method's own generics + `self` receiver compose with interface
  generics without shadowing surprises.

### C 🔴 VM crash `expected map, got instance` for interface value via generic fn  → tests 10,15,43
Root: RC3 runtime side. Interfaces lower to `Ty::Class` (`mir/lower.rs` ~281–289) but a generic
function isn't monomorphized, so a value typed `T = Box<int>` reaches code that unpacks it as a
map. Confirmed crash trace points at `user.main`→`user.read`/`get_value`.
- **Fix**: decide representation. Either (a) ensure interface-typed params are dispatched through
  the same guard-switch as direct interface calls (preferred — reuse cluster A machinery), or
  (b) emit a runtime dispatch table `(interface, type_args, class, method) → fn` consulted by the
  VM. Start by tracing where the Map unpack happens in `bex_vm` for `b.get()` when `b: Box<T>`.
- **Risk**: high — runtime + emit. Pin down the exact opcode that expects Map first.

### D 🟠 Optional/union subtyping + interface? exhaustiveness  → tests 3,32,33,40,41,45
Root: RC2. 
- **Fix (coercion)**: in `builder.rs::is_subtype()`, before delegating to `normalize`, unwrap an
  `Optional`/`Union` *target* and, if the source is a Class (or interface) and an unwrapped target
  member is an Interface, run `type_implements_interface_via_rule`. (Mirror the existing ~9824–9829
  block.) Handle source `Ty::Interface` too (drop the over-tight ~9818 guard).
- **Fix (exhaustiveness/match)**: give `ty_covers_fact()`/`ty_may_match_fact()` (~5643–5720) an
  `Interface` arm doing nominal subtyping, and make the `Interface?` match treat the `null` arm as
  reachable (41) — check the arm-reachability path in `analyze_and_lower_inner` (~11700–11734).
- **Risk**: medium. Decide 41's intended semantics (compile+match-null vs reject-as-non-exhaustive)
  before encoding — see Open questions.

### E 🟠 requires-chain same-name field/method resolves to parent  → tests 5,20,21
Root: RC1 within a requires closure. `.as<B>.label` and a `B`-typed param resolve `label`/`foo`
to the base interface `A`'s slot when `B requires A` and both declare the name.
- **Fix**: when resolving a member through an interface view, prefer the *most-derived* interface's
  own slot in the requires closure before falling back to required parents. Look at
  `interface_closure_*` walks in `tir/builder.rs` (~7910–7964) and the MIR view resolution
  (`interface_closure_type_name_views`, used in `resolve_implementor_method_candidates`).
- **Risk**: medium; overlaps A (same keying/closure code).

### F 🟠 Reflection ignores type args + wrong order  → tests 34,35,36,37
Root: RC1 (34,35,36) + unordered maps (37).
- **Fix (type args)**: stop discarding args at `emit/src/lib.rs:924`; store them in the runtime
  registry (`bex_vm_types/src/types.rs:106` → key on `(TypeName, Vec<Ty>)` or a parallel
  type-args map). Make `ty_name()` (`type_class.rs` ~80–89) surface args and have
  `implements`/`implemented_by`/`implementors` (~31–72) compare/lookup with them.
- **Fix (order)**: build the implementor list in declaration order — switch the `FxHashMap`
  iterations in `tir/interfaces.rs` (~816, 832–833) and `emit/src/lib.rs:945` to ordered
  iteration (IndexMap / sort by source position). May also want `hir` `namespaces`/`types`
  to be `IndexMap` for determinism.
- **Risk**: low–medium; #37 (ordering) is the cheapest standalone win and also fixes latent
  nondeterminism.

### G 🟠 throws interface: false E0096 + catch doesn't match  → tests 38,39
Root: RC2.
- **Fix (38)**: in `check_throws_surface()` (~1260) replace set-difference with subtype-aware
  filtering: an effective throw is OK if it `is_subtype` of some declared throw.
- **Fix (39)**: add the interface arm to `ty_covers_fact`/`ty_may_match_fact` (shared with D) and
  ensure the **runtime** catch type-test in `bex_vm` does nominal interface subtyping (grep the
  catch/throw value-vs-pattern test in `bex_vm`).
- **Risk**: low–medium; compile side is a small change, runtime side needs locating.

### H 🟠 interface method references  → tests 1,2
Root: unbound `Interface.method` doesn't become a polymorphic dispatch closure.
`resolve_interface_member()` returns a raw `Ty::Function` with a receiver generic (required path
~8200–8369; default path ~7978–8196); MIR lowers it to a static `ItemRef::Method`
(`mir/lower.rs` ~657–671) → required-method ref has no body (`expected callable, got any`, #1) and
default-method ref always runs the default (#2).
- **Fix**: lower an unbound interface method reference to a closure that, when applied to a
  receiver, performs the same dispatch as `recv.method()` (reuse cluster A dispatch).
- **Risk**: medium; depends on A's dispatch entry point being callable from a closure.

### I 🟠 inherited default methods absent from concrete class  → tests 17,23,42
Root: RC3. `t.speak()` fails on `class Thing { implements Speaker {} }`.
- **Fix**: include inherited (non-overridden) default methods when flattening class methods
  (`lower_class` ~1102–1117) or when looking up (`lookup_class_method` ~8943 falls through to the
  class's implemented interfaces' defaults). Prefer the lookup-side fix to avoid duplicating bodies.
- **Risk**: low–medium; must preserve override precedence and not reintroduce ambiguity bugs (J).

### J 🟡 ambiguity diagnostics & wording  → tests 25,26,(9,27),11,28,19,22
Root: ambiguity scan only looks at `class_data.methods` (RC3) + diagnostic strings drop type args
(RC1).
- **Fix (25,26)**: `ambiguous_class_method_sources()` (~8866–8909) must consider methods
  contributed by *all* implemented interfaces (incl. empty-block defaults and required methods),
  raising E0121 on clash; called before E0007 is emitted (~6877).
- **Fix (11,28,22)**: carry interface type args into the hint construction in `infer_context.rs`
  (~614–673) so suggestions read `.as<Box<int>>` / `.as<Slot<int>>` / `.as<Container<int>>`.
- **Fix (19)**: E0116 aliased-field mismatch should name the *interface* field (`name`) and note
  the linked class field, not just the class field.
- **Risk**: low; mostly diagnostic plumbing once A/I land.

### K 🟡 parser + union-value dispatch + field/method precedence  → tests 29(/30),31,4,18
- **29/30**: `parse_pattern_atom()` (`parser.rs` ~3960–3963, 4011–4013) only binds with a leading
  `let`; the bare `d: Dog =>` form doesn't parse. **Decide** whether to support it (then add a
  grammar arm) or formally drop it and fix the existing weak test
  `match_narrows_interface_to_concrete_class` (~1133) which uses it but only asserts on the
  E0112–E0132 range. The `fuzz_bug29` test asserts the form runs end-to-end.
- **31 & 4**: calling a method on a `Dog | Cat` union (from match/if arms) crashes. `resolve_member`
  handles `Ty::Union` (~7321–7358) but `try_resolve_member_on_ty()` has a catch-all `_ => None`
  for unions (~7789) so dispatch falls through and the VM crashes. Add union handling there and a
  runtime union method-dispatch path (both arms implement the method).
- **18**: aliased interface field view `name as _name` shadows class method `name()` on a `()`
  call. Fix field-vs-method precedence in `resolve_member` so a parenthesized call prefers the
  class method (or reject the collision with a clear error).
- **Risk**: low–medium each; 31/4 share the union-dispatch root.

## Recommended ordering

1. **RC1 foundation → A, then E, F** (shared keying/closure + reflection). Biggest test payoff,
   unblocks H and J.
2. **RC2 foundation → D + G** (one `is_subtype`/`ty_covers_fact` interface-aware change covers both).
3. **RC3 split**: I (cheap), B (scope), then C (hardest, runtime).
4. **H** (needs A's dispatch entry), **J** (needs A/I), **K** (independent; 29/30 needs a product
   decision; 31/4 share union dispatch).

Suggested first PRs (high value / low blast radius): **F #37 ordering**, **I inherited defaults**,
**G #38 throws subtyping**, **D coercion**. Then the RC1 dispatch refactor (A/E/F-typeargs/C).

## Open questions / decisions needed

- **#41**: should `match` on `Interface?` (a) compile and match `null` via the null arm, or
  (b) be rejected as non-exhaustive without it? `fuzz_bug41` currently expects (a) → `"silent"`.
- **#29/#30**: is `d: Dog =>` (no `let`) supported syntax or not? Pick one and align parser +
  the existing test + `fuzz_bug29`.
- **#31/#4**: should calling a method common to all union members dispatch (preferred), or require
  explicit narrowing (then it's a clean compile error, not a crash)? Either way: **no VM crash**.
- A few "must-reject" tests assert a *needle* the eventual diagnostic must contain
  (`fuzz_bug06` → "Foo", `fuzz_bug11` → "as<Box<int>>", `fuzz_bug19` → "`name`",
  `fuzz_bug22` → "Container<int>", `fuzz_bug28` → "Slot<int>"). Adjust the needle if the chosen
  wording differs — keep the intent (type-args present / interface field named).

---

## Implementation progress (single PR on `aaron/interface-fixes`)

**12 / 44 findings fixed and committed; 0 regressions** (existing 248 interface tests
+ 1576 lib tests still green; one benign TIR snapshot updated).

Fixed:
- **D/G (subtyping):** 3, 33, 38, 40 — nominal interface subtyping now applies through
  optional/union targets and the `throws` surface (`is_subtype` short-circuits +
  subtype-aware throws diff). `crates/baml_compiler2_tir/src/builder.rs`.
- **I (inherited defaults) + J (ambiguity):** 17, 23, 25, 26, 42 — inherited default
  methods are callable unqualified on the concrete class; ambiguity (E0121) now counts
  every contributing interface (override / inherited-default / required). Unified
  `implemented_interface_method_sources`; `class_has_member` recognizes inherited members.
- **B (generic default-method scope):** 7, 16, 44 — interface type params are now bound
  in default-method bodies/signatures (`infer_scope_types` + `enclosing_class_generic_params`
  consult `item_tree.interfaces`).

Still failing (32) — grouped by the work each needs:

### RC1 deep refactor — generic-interface dispatch keyed on type-args (highest value)
Tests 8, 12, 13, 14, 24, 27, 9 (+ reflection 34, 35, 36; requires-chain field views 5, 20, 21).

**Refined root cause (precise):** `interface_class_guard_for_args`
(`mir/src/lower.rs:212`) maps a requested interface instantiation back onto class type
params. For `class Pair<L,R> { implements Getter<L>{} implements Getter<R>{} }` and a
requested `Getter<string>`, the `Getter<L>` block binds only `L→string` and leaves `R`
unbound; the `class_args` collect is `Option<Vec<_>>`, so a single unbound param collapses
the whole guard to `InterfaceClassGuard::Any`. Both blocks therefore guard on `Any` and the
**first** wins. The VM *can* discriminate — `Instance` carries `class_type_args`
(`bex_vm_types/src/types.rs:538`) and `IsType` for `ClassWithTypeArgs` compares them
(`bex_vm/src/vm.rs:4476`) — but only by **full equality** (`expected_args == inst.class_type_args`).

**Fix shape (multi-crate):** make the class guard *partial* —
`InterfaceClassGuard::Exact(Vec<Option<Tir2Ty>>)` (None = wildcard for an unbound class
param). Lower that to a wildcard-aware `IsType`: either a new `ConstValue` variant or a
`TyTemplate` wildcard leaf, plus a VM `IsType` comparison that skips wildcard positions
(position-wise instead of `==`). Reflection 34/35/36 piggyback: stop discarding interface
type args at `emit/src/lib.rs:924`, record them in `program.interface_implementors`
(`bex_vm_types/src/types.rs:106`), and compare in `type_class.rs`. 24 (`expected T, got null`)
is the same substitution gap in default-method dispatch.

### Other remaining
- **C (runtime crash):** 10, 15, 43 — interface value through a generic-fn param →
  `expected map, got instance`. Same RC1 family (monomorphized dispatch repr).
- **H (method refs):** 1, 2 — `Interface.method` must lower to a polymorphic dispatch
  closure (`mir/src/lower.rs` ~657–671), not a static `ItemRef::Method`.
- **K (parser/union):** 4, 18, 29, 31 — `d: Dog =>` no-`let` form (decide + grammar),
  union-of-concrete method dispatch (don't crash), aliased-field-view vs class-method
  precedence on `()`.
- **D/G remainder:** 32 (`let x: I? = i` flagged refutable), 39 (runtime catch by interface
  pattern + `ty_covers_fact` interface arm), 41 / 45 (interface? / union exhaustiveness).
- **J wording:** 11, 19, 22, 28 — diagnostics must carry interface type args
  (`infer_context.rs`) to suggest `.as<Box<int>>` etc.; E0116 should name the interface field.
- **F ordering:** 37 — build the implementor list in declaration order (sort by source
  position; the registry iterates unordered `FxHashMap`s).
- **misc:** 6 — `interface X requires <non-interface>` is silently accepted; needs a new
  interface-validation diagnostic (no per-interface diagnostic pass exists today).






## General problems:
 The optional/union/throws/catch holes existed because is_subtype's interface arm was gated to bare Class <:
    Interface, and normalize, check_throws_surface, and ty_covers_fact each had their own (interface-blind)
    subtype notions. One canonical subtype/assignability function that everything routes through (coercion,
    throws-surface, catch-pattern matching, exhaustiveness) prevents these from drifting apart again. The fix I
    applied only patched two of the call sites; the structural normalizer still has no interface arm.

    5. Determinism: stop iterating FxHashMap where order is observable

    implementors() returning reverse/arbitrary order (and latent nondeterminism elsewhere) comes from iterating
    FxHashMap (namespaces, types, class_implements) in hash order. Use IndexMap or sort by declaration position
    (file + LocalItemId) for anything user-visible or snapshot-tested.

    6. Unify enclosing-generic-scope resolution

    enclosing_class_generic_params and infer_scope_types both special-cased "Class" and silently missed
    interfaces (interfaces push a ScopeKind::Class but live in item_tree.interfaces). A single helper "given a
    function id / scope, return its enclosing type's generic params" that checks classes and interfaces (and
    implements for) removes the duplication and the whole "T unresolved in default body" bug.

    7. Robustness guardrails worth adding

    - Make compile-only tests also exercise runtime where a runtime path exists — bug 1 (method-ref crash) passed
    a #[test] but crashed in the VM because the test never ran it. A lint or convention ("if it compiles a
    callable, call it") would have caught it.
    - Don't let weak assertions mask failures — match_narrows_interface_to_concrete_class "passed" while emitting
    parser errors because it only checked the E0112–E0132 range. Prefer asserting zero diagnostics (or
    end-to-end results) over range-filtered checks.
    - VM "internal error: expected map, got instance" should be impossible by construction — interface values and
    class instances diverging in representation suggests the runtime value model for interface-typed values
    needs a single, enforced shape rather than two implicit conventions.

    8. Smaller polish

    - Several diagnostics (as<Box> hint without type args, E0116 naming the class field instead of the interface
    field) carry only Vec<Name> (simple names). Carrying the resolved interface Ty in the diagnostic payload
    would make messages correct and let multiple call sites format consistently.
    - Interfaces have no structural-item diagnostic pass (only Class/TypeAlias are validated in
    collect_file_diagnostics), which is why requires <non-interface> is silently accepted. Adding interface
    validation there is the natural home.

    The throughline: most of these bugs are duplication bugs — the same concept (member lookup, subtyping,
    interface identity, enclosing generics) implemented N times with N slightly-different behaviors.
    Consolidating each to a single source of truth is what makes the code simultaneously simpler, less buggy, and
    more robust.
---

## Progress update 2 — RC1 core landed (21/44)

Since update 1, the RC1 generic-dispatch foundation is done (+9 findings), zero regressions
(248 existing interface tests + 1576 lib tests green):
- **A (generic dispatch):** 8, 12, 13, 14 — partial class-arg guards. `InterfaceClassGuard::Exact`
  now holds `Vec<Option<Tir2Ty>>`; unbound class params lower to a new `TyTemplate::Wildcard`;
  the VM `IsType` compares `class_type_args` position-wise (wildcard matches any). So
  `Getter<string>` on a `Pair<int,string>` selects the `Getter<R>` block.
- **A/J (ambiguity):** 9, 27 — `implemented_interface_method_sources` dedups by
  (interface, type-args), so `Converter<int>` + `Converter<float>` are distinct → unqualified
  call is E0121.
- **F (reflection):** 34, 35, 36 — `program.interface_implementors` entries carry the interface
  type args each implementor used; `implements`/`implemented_by`/`implementors` compare them.

Remaining (23): C runtime crash (10,15,43), E requires-chain view precedence (5,20,21),
H method refs (1,2), K parser/union (4,18,29,31), D/G remainder (32,39,41,45),
J wording (11,19,22,28), F ordering (37), 24 (generic subst through dispatch), 6.

**Cluster E precise root cause (new):** `resolve_implementor_interface_field_candidates`
(`mir/lower.rs:7032`) matches a requested field against *every* view in the interface's
requires-closure × every impl-block, so for `B requires A` with both declaring `label`,
`.as<B>.label` collects candidates for both the B-view (→`b_label`) and the inherited A-view
(→`a_label`) and impl-block order picks A's. Fix: resolve the field against the *most-derived*
declaring interface in the closure (requested interface first), then only match impl-blocks for
that view. Same shape for methods (finding 5).

---

## Progress update 3 — 26/44 (RC1 + requires-chain + generic-fn dispatch all landed)

Since update 2 (+5), zero regressions (1576 lib + 248 existing interface tests green):
- **E (requires-chain views):** 5, 20, 21 — field/method resolution pins the *most-derived*
  declaring interface in the closure (`interface_declares_field`/`_method` + `method_provider_view`
  in `mir/lower.rs`), so `.as<B>.label` and `A::foo` vs `B::foo` stay distinct.
- **C (generic-fn interface param):** 10, 43 — a requested interface arg that is an enclosing
  function's type-var now matches any implementor (runtime IsType discriminates), fixing the
  `expected Map, got Instance` crash for `fn read<T>(b: Box<T>) { b.get() }`.

Fixed so far (26): 3,5,7,8,9,10,12,13,14,16,17,20,21,23,25,26,27,33,34,35,36,38,40,42,43,44.

### Remaining 18 — each a distinct deeper investigation
- **Exhaustiveness/usefulness matrix vs interface↔optional** (32, 41, 45): `let x: I? = i`
  wrongly E0111-refutable; `match` on `I?` mis-handles the null arm / union arms. Lives in
  `exhaustiveness.rs` + the pattern matrix — needs the matrix to know `Class <: Interface` and
  `T <: T?`.
- **15** (default method's `self.size()` through an interface-typed var → `expected Map, got
  Instance`) and **24** (`b.get()` on `Container<int>` → compile-time `expected T, got null`):
  generic substitution / interface-self runtime repr in default-method dispatch.
- **H method refs** (1, 2): `Interface.method` must lower to a polymorphic dispatch closure
  (`mir/lower.rs` member-ref lowering), not a static `ItemRef::Method`.
- **K parser/union** (4, 18, 29, 31): `d: Dog =>` no-`let` form is a **product decision** (support
  in grammar, or drop it and fix the existing weak test); union-of-concrete method dispatch (4, 31)
  needs a union-receiver dispatch path; 18 is field-view-vs-class-method precedence on `()`.
- **J wording** (11, 19, 22, 28): diagnostics carry only simple `Name`s; suggesting `.as<Box<int>>`
  / naming the interface field needs interface type args plumbed into the diagnostic payloads.
- **F ordering** (37): `implementors()` order — `LocalItemId` is hash-based (position-independent),
  so true declaration order needs sorting by source span.
- **6**: `interface X requires <non-interface>` silently accepted — needs a new per-interface
  validation diagnostic (no such pass exists today).

---

## Progress update 4 — 28/44; entering the high-risk long tail

Since update 3 (+2), zero regressions:
- **18** — class method wins over an aliased interface field view on access.
- **39** — `catch`/`match` against an interface type now tests its implementors
  (`emit_is_type_branch` expands an interface to a disjunction over implementor classes).
- **29/30** — corrected to the canonical `let d: Dog =>` form (per product decision: the
  no-`let` form is not valid syntax); strengthened the pre-existing weak test.

Fixed (28): 3,5,7,8,9,10,12,13,14,16,17,18,20,21,23,25,26,27,29,30,33,34,35,36,38,39,40,42,43,44.

### Remaining 14 — each a distinct, deeper change (deferred for dedicated work)
- **Exhaustiveness/usefulness matrix** (32, 41, 45) — `enumerate_ctors` (builder.rs:11139)
  models a union as `UnionMember`s and an interface as `NonExhaustive`, but a type-ascription
  arm `let a: Animal` against a `Animal | string` scrutinee is lowered as a catch-all, so the
  `string` arm is flagged unreachable; `Interface?`'s null arm and `let x: I? = i` refutability
  hit the same gap. Fix = lower an interface/class type-ascription against a union/optional
  scrutinee to a *member-targeted type test*. HIGH RISK: this code governs all match/catch
  exhaustiveness (1576 lib tests) — needs careful, isolated work.
- **Method-reference dispatch thunks** (1, 2) — `let f = Interface.method; f(recv)` must lower
  the reference to a closure that dispatches on its receiver; today it binds the interface's
  default/`any` statically. Needs a synthesized per-(interface,method) dispatch thunk.
- **Union-of-concrete method dispatch** (4, 31) — calling a method common to all members of a
  `Dog | Cat` value (from `if`/`match` arms) crashes; needs a union-receiver dispatch path.
- **Generic substitution through default dispatch** (15, 24) — interface-self repr in default
  bodies + return-type `T` substitution through `Container<int>`-typed dispatch.
- **Diagnostic wording** (11, 19, 22, 28) — plumb interface type-args / interface-field names
  into diagnostic payloads (`AmbiguousInterfaceField`, projection hints) to suggest
  `.as<Box<int>>` etc.
- **6** — `interface X requires <non-interface>` needs a new per-interface validation pass.
