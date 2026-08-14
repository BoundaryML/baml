# BEP-066 `hir_ty` port contracts

This document freezes the compiler-core contracts for the BEP-066 port after
the TIR-to-`hir_ty` cutover. `TYPE_SYSTEM.md` and the current `hir_ty` behavior
remain authoritative. The names below may be shortened during implementation,
but ownership, identity, ordering, and escape rules are binding.

## Semantic port checklist

Every semantic fact in `PORT_INVENTORY.md` has one checklist item. A checked
item means its implementation and slice checkpoint are complete, not merely
that code exists.

| Done | Fact | Slice | Acceptance condition |
| --- | --- | --- | --- |
| [ ] | I-01 | 3, 4, 7 | Loc-free external calls preserve a symbolic free/method/interface target, receiver mode, user generic frames and bounds, and linkability through inference and MIR. |
| [x] | I-02 | 3, 6 | Mounted aliases and enum variants are supplied by `PackageInterface` to the same fact consumers as source-backed definitions. |
| [x] | I-03 | 3, 6 | Mounted interface associated-type lookup closes transitively over `requires` with cycle termination. |
| [x] | L-01 | 3 | Type lookup distinguishes source-backed definitions from source-less exported types without reaching into absent dependency source. |
| [x] | L-02 | 3 | Foreign classes, interfaces, enums, aliases, and variants receive the same kind and arity validation as local definitions. |
| [x] | L-03 | 3, 6 | Foreign interface pins validate names and duplicates, realize defaults with symbolic `Self`, and diagnose missing required bindings. |
| [x] | L-04 | 3 | `reflect.X` type shorthand is tried only after ordinary local, user, and package resolution fails. |
| [x] | L-05 | 4 | Only the exact extraction contract permits an outer function type without `throws`; ordinary function types retain current diagnostics and recovery. |
| [x] | R-01 | 3 | Value roots `reflect`, `type`, and `json` fall back to package `baml` only after ordinary resolution fails, preserving the full path. |
| [x] | T-01 | 2, 5 | Runtime type operands and type-binding values are ordinary hidden expression edges for throw-fact traversal. |
| [ ] | B-01 | 2, 4 | Runtime slots and loc-free callables are isolated per body/default run and survive only in durable result tables. |
| [x] | B-02 | 5 | Inference-time type lowering sees declared generics plus the active scoped overlay. |
| [x] | B-03 | 4 | Only the special `Session.eval` result slot defaults an uninferable generic to `unknown`; ordinary slots keep current errors. |
| [x] | B-04 | 3, 4 | Mounted owner/function generics and bounds instantiate normally, synthetic effect parameters do not affect user arity, and bound receivers seed owner substitutions. |
| [x] | B-05 | 2, 4 | Written type arguments retain ordered static/runtime provenance; runtime operands use a bound-or-`unknown` occurrence type and never become solver variables. |
| [x] | B-06 | 4 | A bare value in a generic slot reports the targeted “requires `unreflect`” diagnostic. |
| [x] | B-07 | 4 | Every runtime operand is inferred and checked below primitive `type`, with normal error/pending cascade suppression. |
| [x] | B-08 | 4 | Only checks that depend on a runtime slot are deferred; operands are still inferred and unrelated static bounds remain enforced. |
| [ ] | B-09 | 4, 7 | Exact `baml.reflect.Package.get_function<F>` extraction uses its special type position and MIR consumes the solved plan without re-lowering syntax. |
| [x] | B-10 | 4 | Argument binding enriches one call plan without erasing type slots; optional and ordinary calls use the same write path. |
| [x] | B-11 | 4 | Uncontracted render/build helpers seed schema `T` from a named, non-generic LLM function return type. |
| [x] | B-12 | 2, 5 | Default and forward-reference traversal visits hidden operands exactly once and default inference cannot leak per-body external-call state. |
| [x] | B-13 | 5 | A runtime type binding installs a statement-identity rigid parameter, validates its value, and cannot escape its lexical block. |
| [x] | B-14 | 4 | Sealed reflection-kind classes cannot be object-constructed in inferred or expected-type paths. |
| [x] | B-15 | 3, 6 | Source-less mounted free functions, types, variants, and UFCS methods resolve exclusively from `PackageInterface`, after real-name resolution. |
| [x] | B-16 | 3, 6 | Mounted fields and bound/unbound methods specialize receiver generics and preserve direct-method or interface-slot dispatch identity. |
| [x] | B-17 | 3, 6 | Mounted builtins without a loc-free link contract are reserved and fail with the targeted unsupported-call diagnostic, including optional calls. |
| [x] | B-18 | 4 | Streaming calls reject runtime type-argument slots. |
| [x] | B-19 | 4 | `from_json` reconstruction runs only for an all-static type-argument plan. |
| [x] | B-20 | 3 | Expression inference uses the same shadow-preserving `baml.reflect/type/json` fallback as type lowering. |
| [x] | B-21 | 5 | An `unreflect` pattern checks its operand as `type`, preserves the scrutinee type, has unique possible-but-non-covering usefulness identity, and binds nothing. |
| [x] | B-22 | 2, 5 | Inference and throw analysis share canonical hidden-child traversal for runtime operands and type-binding values. |
| [x] | N-01 | 1 | Static Mint identity hashes canonical plain `NormalTy` with fixed FNV-1a-64 and architecture-stable numeric encoding, never an intern handle. |
| [x] | N-02 | 1 | The sealed builtin reflection-kind classes have outer category `type`. |
| [x] | N-03 | 1 | Exactly the sealed builtin reflection-kind classes subtype primitive `type`; user classes cannot opt in. |

## Runtime type-slot contract

The canonical syntax bridge is an ordered slot enum in
`baml_compiler2_hir::body_type_refs`:

```rust,ignore
enum BodyTypeArgRef {
    Static(TypeRefId),
    Runtime { operand: ExprId },
}
```

`BodyTypeRefs::expr_type_args` stores every written slot in source order. A
runtime operand is also a canonical child expression for reachability,
defaults, forward references, and throw facts. Its `ExprId` is its identity;
source spans remain in the existing source maps and are not copied into typed
data.

Inference lowers that syntax once into the authoritative per-call plan:

```rust,ignore
enum CallTypeArgPlan {
    Static { ty: Ty },
    Runtime {
        operand: ExprId,
        occurrence_ty: Ty,
        parameter: ParamTy,
    },
}

struct CallPlan {
    bindings: Vec<ParamBinding>,
    type_args: Vec<Ty>,
    own_offset: usize,
    explicit: bool,
    slots: Vec<CallTypeArgPlan>,
    deferred_checks: Vec<RuntimeCheck>,
    runtime_id: Option<ExprId>,
    target: Option<SymbolicCallableTarget>,
}
```

The full solved `type_args` remains in declared De Bruijn order and is ground
at finalization. `slots` preserves written provenance and order. For a runtime
slot, `occurrence_ty` is the first usable static bound or `unknown`; it is not
an `InferVar`, cannot be unified with the runtime value, and is never minted
from intern identity. `deferred_checks` names only argument/bound checks whose
expected type actually depends on runtime slots. An all-static plan may use an
asserted compatibility shortcut, but mixed plans are never re-lowered from AST
syntax by MIR.

The MIR provider converts interned types to plain types once and mirrors this
plan losslessly. The plan, rather than syntax inspection, controls type
operands, runtime gates, call flags, extraction contracts, and target links.

## Loc-free callable-target contract

The canonical serializable descriptor lives beside callable metadata in
`baml_compiler2_hir_ty::callable`; `PackageInterface` exports it and
`MemberResolution`/`CallPlan` carry it. This avoids making package-interface
storage the semantic owner while still giving source-less consumers a durable
value.

```rust,ignore
enum SymbolicCallableTarget {
    Free { package: Name, namespace: Vec<Name>, function: Name },
    Method { package: Name, owner: QualifiedTypeName, method: Name },
    Interface { package: Name, interface: QualifiedTypeName, slot: Name },
}

struct ExternalCallable {
    target: SymbolicCallableTarget,
    receiver: ReceiverMode,
    owner_generic_params: Vec<BoundedParam>,
    function_generic_params: Vec<BoundedParam>,
    linkability: Linkability,
}
```

Targets are namespaced structural identities, Borsh-serializable and
location-free. No mounted definition receives a forged `SourceFile`, item id,
or source location. Source-backed calls retain their existing location-based
resolution variants. A mounted call receives an external resolution containing
the descriptor, and its call plan clones the symbolic target before inference
finishes. `Linkability::ReservedBuiltin` is explicit data and produces the
unsupported-mounted-call diagnostic instead of failing in MIR or the linker.

Owner parameters precede function parameters. Exported user arity excludes
synthetic effect parameters; bounds remain attached to the corresponding rigid
parameter. Bound methods consume `self` and seed the owner prefix, unbound
methods retain it, and interface targets identify a virtual slot rather than a
concrete source body.

Changing this Borsh schema intentionally invalidates compiler-built cached
package-interface bytes. Compatibility is per compiler build; it is not an
external persistence or wire-format promise.

## Scoped generic-overlay contract

`LowerCtx` continues to own the immutable declaration frame. Dynamic runtime
type bindings are owned by the body-local `InferenceContext` as an ordered
overlay:

```rust,ignore
struct ScopedTypeBinding {
    name: Name,
    parameter: ParamTy,
    operand: ExprId,
    occurrence_ty: Ty,
}
```

The synthetic `ParamTy` is rigid and keyed deterministically by the body owner
and `StmtId`; textual names alone are never identity. Type-name lookup searches
the innermost active overlay first, then the fixed declared generic frame, then
ordinary type/package names. This gives ordinary lexical shadowing without
mutating reusable declaration-lowering contexts.

Block entry records an integer overlay checkpoint. Both inference-driven and
expectation-driven block exits call the same finalizer, which resolves types,
erases parameters introduced after the checkpoint from the block result and
all locals that can leave the block, then truncates the overlay. The erasure
uses the binding's static occurrence type (bound or `unknown`), never the
runtime operand as a solver variable. Nested blocks, branches, defaults, and
lambdas receive independent checkpoints; a default/lambda body cannot observe
or leak an outer transient overlay unless lexical ownership explicitly places
the binding in that body.

The implementation uses explicit checkpoint/finalize methods rather than a
borrow-holding RAII guard, so recursive mutable inference remains possible and
both block paths are mechanically routed through identical cleanup.

## Pinned new-engine hole and open-throws behavior

These are existing `hir_ty` semantics and must not change as a consequence of
the port:

- An allowed `_` creates a fresh inference hole. `let x: _ = 1` resolves the
  hole exactly to `int`.
- Expression-position holes participate in contextual inference, including
  `Box<_> { v: 5 }`, which resolves to `Box<int>`.
- A hole that remains unsolved reports the current E0147 diagnostic. Positions
  where holes are forbidden keep their current E0147 behavior.
- A top-level partial clause such as `throws AppError | _` is open: the named
  part remains declared and the inferred body residue is unioned into the
  callable effect. The pinned fixture resolves to `7 | user.AppError`.
- A missing `throws` on an ordinary written function type keeps the current
  diagnostic and `never` recovery. Only the exact extraction-contract position
  introduced in Slice 4 may interpret that omission as a runtime wildcard.

The existing type-spec fixtures
`wildcard_hole_in_let_annotation.baml`,
`wildcard_hole_in_constructor_generic_arg.baml`, and
`partial_throws_clause.baml` are the Slice 0 executable pins. Former TIR-era
B-230/B-247 integration cases are reviewed individually in Slice 8; an old
ignored expectation never overrides these fixtures.
