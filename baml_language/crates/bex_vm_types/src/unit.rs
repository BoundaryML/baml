//! Symbolic, relocatable per-source-file compilation units (B-693 §2).
//!
//! A [`CompilationUnit`] is the relocatable artifact emit produces for one
//! source file. Where today's per-file bytecode reuse *reconstructs* a file's
//! object boundaries by scanning a flat `Program`, a unit *stores* them: the
//! file's own compiled objects carry **unit-local** index operands, an
//! **import table** names every external reference by fully-qualified name, and
//! an **export table** names what the unit provides.
//!
//! The instruction/[`Object`] structs are unchanged — a unit merely
//! reinterprets the existing `ObjectIndex`/`GlobalIndex` *values* through a
//! per-unit convention (§2a): a unit's object address space is
//! `[0 .. n_local_objects)` for its own definitions and `[n_local_objects ..)`
//! for imports, and symmetrically for globals. The [`link`](crate::link) step
//! folds units into a runnable `Program` by resolving imports and rebasing
//! local operands through the existing `relink` operand walkers.
//!
use crate::{RealizedTy, TyTemplate};
use baml_base::Name;
use borsh::{BorshDeserialize, BorshSerialize};

use crate::{
    Object, TestCase,
    types::{InterfaceBound, LocalName},
};

/// The kind of definition a [`Symbol`] refers to. Selects which of the linked
/// `Program`'s name maps resolves the symbol.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum SymbolKind {
    /// A named function (owns a global slot *and* a pool object).
    Function,
    /// A top-level `let` binding (owns a global slot, initialized by `$init`).
    Let,
    /// A class definition.
    Class,
    /// An enum definition.
    Enum,
    /// An interface definition.
    Interface,
    /// A generic-function instantiation (`foo<int>`) referenced as a value. The
    /// instantiation is not itself in any name map; the linker re-interns it
    /// from the [`Symbol::generic`] key (design §9 R1).
    GenericFn,
}

/// The re-intern key for a [`SymbolKind::GenericFn`] symbol.
///
/// A generic-function value (`foo<int>` used as a value, i.e. an
/// `Object::GenericFunction`) is interned across the whole program by
/// `(base function, type arguments)`, so `foo<int>` exists once. Under per-unit
/// emit each unit emits its own local copy; the linker dedups them at link time
/// (design §9 R1) keyed by this pair — hence it must round-trip losslessly.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct GenericFnKey {
    /// Fully-qualified name of the base function being instantiated. Resolves
    /// to a global slot in the linked image (same key space as
    /// `Program::function_global_indices`).
    pub base_fn: String,
    /// Concrete type arguments seeded into `frame.type_args` when the value is
    /// called. Together with `base_fn` this is the whole-program intern key
    /// (mirrors `GenericFunction::type_args`: typevars are invalid in a called
    /// frame's args, so these are `RealizedTy`, not `TyTemplate`).
    pub type_args: Vec<RealizedTy>,
}

/// A cross-unit symbol: what an import references. Exports never use a `Symbol`
/// (they use [`LocalRef`]). `fq_name` is the resolution key in the linked
/// `Program`'s name maps for every kind except [`SymbolKind::GenericFn`], which
/// resolves through [`Self::generic`].
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Symbol {
    /// Which definition kind (and hence which name map) resolves this symbol.
    pub kind: SymbolKind,
    /// Fully-qualified name — the same key space as `Program::function_indices`
    /// / `function_global_indices` / `let_global_indices` / the per-package
    /// class/enum/interface maps. For a [`SymbolKind::GenericFn`] this carries
    /// the base function's name (for determinism/debugging); the true intern
    /// key is [`Self::generic`].
    pub fq_name: String,
    /// Present iff `kind == SymbolKind::GenericFn`: the `(base_fn, type_args)`
    /// re-intern key. `None` for every other kind.
    pub generic: Option<GenericFnKey>,
}

/// Which per-unit bucket + offset an export (or local reference) points at. The
/// linker places each bucket at its own base (classes, then enums, then
/// interfaces, then code — design §3b), so a bare flat offset is not enough to
/// name a definition symbolically.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum LocalRef {
    /// Offset into the unit's `classes` bucket.
    Class(u32),
    /// Offset into the unit's `enums` bucket.
    Enum(u32),
    /// Offset into the unit's `interfaces` bucket.
    Interface(u32),
    /// Offset into the unit's `type_alias_objects` bucket.
    TypeAlias(u32),
    /// Offset into the unit's `code` bucket (functions, lambdas, interned
    /// literals, local generic-fns).
    Code(u32),
}

/// The names a unit provides, mapped to its own local indices. `Vec` (not a
/// `HashMap`) so the on-wire order is the deterministic emit order.
#[derive(Clone, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ExportTable {
    /// Fully-qualified name to the local bucket+offset that defines it. Covers
    /// functions (`LocalRef::Code`) and class/enum/interface definitions.
    pub objects: Vec<(String, LocalRef)>,
    /// Fully-qualified name of each function / `let` that owns a global slot, to
    /// its local global ordinal (dense, `0 .. n_local_globals`). This is the
    /// unit's *complete* local global table, not just an externally-visible
    /// subset — the linker relies on it to size and fill the globals pool.
    pub globals: Vec<(String, u32)>,
}

/// The symbolic (name-referencing, not `ObjectIndex`) twin of a `ProgramPackage`
/// fragment contributed by one unit. The linker merges every unit's fragment
/// into the image's `packages` map, resolving each fully-qualified name to an
/// absolute `ObjectIndex` (design §3b step 5). `Vec` preserves the deterministic
/// emit order the `IndexMap`s in `ProgramPackage` require.
#[derive(Clone, Debug, Default, BorshSerialize, BorshDeserialize)]
pub struct ProgramPackageFrag {
    /// All source-visible declaration names (types, aliases, and values).
    pub exported_names: Vec<LocalName>,
    /// Local class name to fully-qualified class name (resolved to `ObjectIndex`
    /// at link).
    pub classes: Vec<(LocalName, String)>,
    /// Local enum name to fully-qualified enum name.
    pub enums: Vec<(LocalName, String)>,
    /// Local interface name to fully-qualified interface name.
    pub interfaces: Vec<(LocalName, String)>,
    /// Local exported free-function name to its fully-qualified symbol.
    pub functions: Vec<(LocalName, String)>,
    /// Implemented-interface fully-qualified name to the impl rules declared for
    /// it in this unit (the interface may live in a dependency package).
    pub impl_rules: Vec<(String, Vec<ProgramImplRuleFrag>)>,
    /// Recursive type aliases defined in this unit, by fully-qualified name of
    /// the emitted `Object::TypeAlias`. Non-recursive aliases are expanded at
    /// lowering and never appear.
    pub type_aliases: Vec<(LocalName, String)>,
    /// Whole-package enriched interface. Exactly one unit is its carrier.
    pub interface_blob: Vec<u8>,
    /// Fully-qualified synthesized `$init_test` symbol, when present.
    pub test_init: Option<String>,
}

/// Symbolic twin of `ProgramImplRule`: `interface_head` and each method `fqn`
/// are fully-qualified names the linker resolves to `ObjectIndex`es.
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub struct ProgramImplRuleFrag {
    /// Fully-qualified name of the interface this rule heads.
    pub interface_head: String,
    /// Pattern type this rule implements the interface for.
    pub for_ty_pattern: TyTemplate,
    /// Interface bounds on each generic parameter of the rule.
    pub generic_param_bounds: Vec<Vec<InterfaceBound>>,
    /// Interface type arguments.
    pub interface_args: Vec<TyTemplate>,
    /// Associated-type bindings, by associated-type name.
    pub interface_assoc: Vec<(Name, TyTemplate)>,
    /// Method name to its symbolic implementation.
    pub methods: Vec<(Name, ProgramMethodImplFrag)>,
    /// See [`RuntimeImplRule::field_links`](crate::types::RuntimeImplRule::field_links).
    /// Slot indices are layout, not symbols, so they survive relinking unchanged.
    pub field_links: Box<[u32]>,
}

/// Symbolic twin of `ProgramMethodImpl`: `fqn` is the callee function's
/// fully-qualified name (resolved to an `ObjectIndex` at link).
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub struct ProgramMethodImplFrag {
    /// Fully-qualified name of the callee function.
    pub fqn: String,
    /// The callee's type-argument frame at the impl site.
    pub frame: Vec<TyTemplate>,
}

/// One source file's relocatable compiled output — the B-693 unit format
/// (design §2b).
///
/// Definitions are bucketed by emit pass (`classes` / `enums` / `interfaces` /
/// `code`) so the linker can interleave them pass-major across units to
/// reproduce today's flat pool order (design §9 R3 — appending a unit as one
/// block would reorder the pool and break byte-identity). Every index operand
/// inside these objects uses the per-unit convention of §2a, which the linker
/// resolves into program indices.
#[derive(Clone, Debug, Default, BorshSerialize, BorshDeserialize)]
pub struct CompilationUnit {
    /// Project-root-relative source path (equals `Function::source_file`). Never
    /// absolute, so the artifact relocates across checkouts (design §9 R7).
    pub source_file: String,
    /// Package this file contributes to.
    pub package: Name,

    // --- definitions, bucketed by emit pass (design §3b / §9 R3) ---
    /// `Object::Class` definitions, in declaration order.
    pub classes: Vec<Object>,
    /// `Object::Enum` definitions.
    pub enums: Vec<Object>,
    /// `Object::Interface` definitions.
    pub interfaces: Vec<Object>,
    /// `Object::TypeAlias` definitions (recursive aliases only).
    pub type_alias_objects: Vec<Object>,
    /// The pass-4 block: functions, lambdas, interned literals, and local
    /// generic-fn objects, in emit order.
    pub code: Vec<Object>,

    // --- symbolic cross-unit tables ---
    /// Every non-local `ObjectIndex` reference this unit makes, by name.
    pub object_imports: Vec<Symbol>,
    /// Every non-local `GlobalIndex` reference this unit makes, by name.
    pub global_imports: Vec<Symbol>,
    /// The names this unit provides, mapped to local indices.
    pub exports: ExportTable,

    // --- side-table fragments the whole-program passes consume at link ---
    /// This unit's symbolic contribution to its package's structure.
    pub package_fragment: ProgramPackageFrag,
    /// Pass-8 compiled test cases defined in this file.
    pub test_cases: Vec<TestCase>,
    /// `borsh(CallableThrowsFragment)` for this file. Opaque bytes
    /// because `bex_vm_types` sits below `baml_compiler2_hir_ty`, which owns the
    /// typed fragment — the same decoupling as the stdlib-interface blob. Empty
    /// for builtins (their interface rides in the stdlib blob) and for any file
    /// whose fragment failed to serialize. Populated by `decompose_units`;
    /// consumed by the cache layer to project a `callable_throws` seed. Not
    /// folded into `Program`, carries no absolute paths (design §9 R7).
    pub callable_throws_fragment: Vec<u8>,

    /// The whole-*group* `$init` / `$init_test` tail (design §9 R2), carried on
    /// one unit of the group that produces it (empty on every other unit). The
    /// tail cannot be a per-file product — `$init` topo-sorts a package's `let`s
    /// across files — so it is captured pre-synthesized and the linker places it
    /// after the group's regular code (see [`InitTail`]).
    pub init_tail: Option<InitTail>,
}

/// The pre-synthesized `$init` / `$init_test` tail of one emit **group** (design
/// §9 R2). Its objects are the per-package `$init` helper functions (with any
/// lambdas + interned literals), the `$init` functions themselves, and the
/// `$init_test` chainers, in the exact pool order a full compile emits them. The
/// linker appends them after the group's regular code and assigns their global
/// slots after the group's function+`let` slots, reproducing the flat layout.
///
/// # Operand convention
///
/// A tail object's index operands use a per-tail local/import convention
/// mirroring §2a: an `ObjectIndex` `raw < objects.len()` is tail-local (the
/// linker rebases it `tail_object_base + raw`); otherwise it indexes
/// [`Self::object_imports`]. A `GlobalIndex` `raw < slot_objects.len()` is a
/// tail-local slot (a nameless `$init` helper slot, or a named `$init`/
/// `$init_test` slot — rebased `tail_slot_base + raw`); otherwise it indexes
/// [`Self::global_imports`] (a `let`/function slot in the main image).
#[derive(Clone, Debug, Default, BorshSerialize, BorshDeserialize)]
pub struct InitTail {
    /// Tail objects (helpers, their lambdas/literals, `$init`s, `$init_test`s)
    /// in pool order, operands in the tail-local convention above.
    pub objects: Vec<Object>,
    /// Cross-tail object references (main-image classes/enums/interfaces/
    /// functions/generic-fns), by symbol.
    pub object_imports: Vec<Symbol>,
    /// Cross-tail global references (main-image functions/`let`s), by symbol.
    pub global_imports: Vec<Symbol>,
    /// Tail-local object index of each tail object that owns a global slot, in
    /// slot-assignment order. Every such slot holds `Object(that object)`; the
    /// index into this vec is the tail-local slot ordinal.
    pub slot_objects: Vec<u32>,
    /// The named tail functions (`$init` / `$init_test` chainers) to register in
    /// `function_indices` / `function_global_indices`: fq name → tail-local
    /// object index.
    pub named: Vec<(String, u32)>,
    /// This group's contribution to `Program::package_init_order`, in order.
    pub package_init_order: Vec<String>,
}
