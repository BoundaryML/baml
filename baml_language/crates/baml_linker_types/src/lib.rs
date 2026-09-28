//! Relocatable compilation units — one package's compiled output for one
//! link — and the package-level products a link needs beside them.
//!
//! A [`CompilationUnit`] is what the emitter produces for ONE package. It is
//! a pure function of the package's sources and of the interfaces of the
//! packages it reaches: the package's own definitions in per-kind buckets, a
//! [dependency table](DependencyEntry), an [import table](ImportEntry) whose
//! entries name a declaration *inside* a dependency by that package's own
//! item coordinates, an [export table](ExportTable) in the same coordinates,
//! and code whose index operands are local-or-import ordinals. The linker
//! (`baml_linker`) binds each dependency slot to a package, matches every
//! import against the bound package's own exports, and lays the units out
//! into one runnable [`Program`](bex_vm_types::Program).
//!
//! The grain is the package because the package is the unit of naming and
//! dependency in the language — a file is not a semantic entity, and cutting
//! a package at file boundaries would turn every intra-package reference
//! into an import of the package itself. The one place a unit is smaller
//! than a package is a session: each submission is a unit of the growing
//! session package, and reaches earlier submissions' declarations as
//! imports at [`DepSlot::SELF`].
//!
//! Nothing in a unit is a rendered spelling of a foreign declaration: a
//! declaration is addressed by *which dependency* and *which item path in
//! that package* — the way ELF, WebAssembly, the JVM constant pool, and
//! rustc's `CrateNum` + `DefPath` all address a foreign symbol. A package has
//! no intrinsic name and a unit never states one; the link set says which
//! package a unit is.
//!
//! # Operand convention
//!
//! The instruction and [`Object`] structs are unchanged; a unit reinterprets
//! their `ObjectIndex` / `GlobalIndex` VALUES — and every type head's, since a
//! head in a unit is the declaration's object operand
//! ([`TypeHead::unresolved_operand`](bex_vm_types::TypeHead::unresolved_operand)),
//! as is every class key of a type switch
//! ([`SwitchKey::Declaration`](bex_vm_types::bytecode::SwitchKey)):
//!
//! - an object operand `raw < n_local_objects` is unit-local, laid out
//!   bucket by bucket — classes, enums, interfaces, recursive aliases, then
//!   code — so class `k` is `k`, enum `k` is `C + k`, interface `k` is
//!   `C + E + k`, alias `k` is `C + E + I + k`, and code `k` is
//!   `C + E + I + A + k`;
//! - a global operand `raw < n_local_globals` is unit-local: functions and
//!   interface bodies in `[0, F)` in emit order, then `let`s in `[F, F + L)`
//!   (the partition [`ExportTable::globals`] records);
//! - an operand `raw >= IMPORT_BASE` is import ordinal `raw - IMPORT_BASE`
//!   into [`CompilationUnit::object_imports`] or
//!   [`CompilationUnit::global_imports`].
//!
//! Imports live in a fixed range rather than "past the locals" because the
//! import count is unknown until a unit closes, and rebasing operands at close
//! would collide with parallel codegen's fragment rebase. An [`InitTail`]
//! uses the same convention over its own tables.

use baml_base::Name;
use bex_vm_types::{DeclPath, InterfaceBound, Object, ObjectIndex, TyTemplate, types::LocalName};
use borsh::{BorshDeserialize, BorshSerialize};

/// The first import ordinal in every per-unit index space (see the module
/// doc). Kept below `2^32` so it is a valid `usize` on every target this crate
/// builds for — wasm32 has a 32-bit `usize` — while leaving `2^31` local
/// objects or slots, far beyond any real unit.
pub const IMPORT_BASE: usize = 1 << 31;

/// The import ordinal an operand encodes, or `None` for a local operand.
#[must_use]
pub fn import_ordinal(raw: usize) -> Option<usize> {
    raw.checked_sub(IMPORT_BASE)
}

/// The operand value encoding import ordinal `ordinal`.
#[must_use]
pub fn import_operand(ordinal: usize) -> usize {
    IMPORT_BASE + ordinal
}

/// A package-relative item coordinate: namespace path plus item name. The
/// owning package's own coordinates — never a consumer's spelling of them.
pub type ItemPath = LocalName;

/// SHA-256 over the raw Borsh payload of a package interface: the surface a
/// compile assumed of a dependency, checked at link against the package that
/// binds the slot.
pub type Digest = [u8; 32];

/// A slot in a unit's (or tail's) dependency table. Slot `0` is the unit's own
/// package and is never stored; slot `k >= 1` is `dependencies[k - 1]`.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, BorshSerialize, BorshDeserialize,
)]
pub struct DepSlot(pub u32);

impl DepSlot {
    /// The unit's own package. As an IMPORT's slot it is meaningful only for
    /// a session submission, whose package already holds the declarations of
    /// earlier submissions; any other unit's own declarations are locals.
    pub const SELF: Self = Self(0);

    #[must_use]
    pub fn is_self(self) -> bool {
        self == Self::SELF
    }

    /// The slot for entry `index` of a dependency table.
    #[must_use]
    pub fn of_dependency_index(index: usize) -> Self {
        Self(u32::try_from(index + 1).expect("dependency tables fit u32"))
    }

    /// The dependency-table entry this slot names, or `None` for
    /// [`Self::SELF`].
    #[must_use]
    pub fn dependency_index(self) -> Option<usize> {
        self.0.checked_sub(1).map(|k| k as usize)
    }
}

/// One dependency of a unit: which package binds slot `k + 1`, and the
/// interface the compile assumed of it.
///
/// One entry per package ROOT: a package mounted under two aliases is one
/// root, located by its first alias. The table is topologically ordered —
/// `via` always names an earlier slot — so a transitive root (one the
/// consumer cannot spell from source but can reach through a dependency's
/// API) is located through its parent's edge table.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct DependencyEntry {
    /// The edge name that locates the package in `via`'s edge table.
    pub edge: Name,
    /// Whose edge table `edge` is read from: [`DepSlot::SELF`] for a direct
    /// dependency, an earlier slot for a transitive one.
    pub via: DepSlot,
    /// Digest of the interface payload this compile read; `None` for a
    /// transitive root, whose interface the consumer never read.
    pub fingerprint: Option<Digest>,
}

/// An import: the declaration a unit references, addressed inside the package
/// bound to `dep`.
#[derive(Clone, Debug, PartialEq, Eq, Hash, BorshSerialize, BorshDeserialize)]
pub struct DeclKey {
    pub dep: DepSlot,
    pub path: DeclPath,
}

/// One entry of a unit's import table. The table is TOTAL over the foreign
/// declarations the unit references in any way: a type head is an object
/// operand like any other, so a declaration reached only through a type is
/// imported exactly as one reached by an instruction.
#[derive(Clone, Debug, PartialEq, Eq, Hash, BorshSerialize, BorshDeserialize)]
pub struct ImportEntry {
    pub key: DeclKey,
}

/// Which per-unit bucket + offset an export points at. The linker places each
/// bucket at its own base, so a bare flat offset cannot name a definition.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, BorshSerialize, BorshDeserialize)]
pub enum LocalRef {
    /// Offset into the unit's `classes` bucket.
    Class(u32),
    /// Offset into the unit's `enums` bucket.
    Enum(u32),
    /// Offset into the unit's `interfaces` bucket.
    Interface(u32),
    /// Offset into the unit's `type_alias_objects` bucket.
    TypeAlias(u32),
    /// Offset into the unit's `code` bucket.
    Code(u32),
}

impl LocalRef {
    /// Is this the bucket a declaration at `path` is pooled in? Never true
    /// for a `let`, which owns no pool object.
    #[must_use]
    pub fn holds(self, path: &DeclPath) -> bool {
        matches!(
            (path, self),
            (DeclPath::Class(_), Self::Class(_))
                | (DeclPath::Enum(_), Self::Enum(_))
                | (DeclPath::Interface(_), Self::Interface(_))
                | (DeclPath::TypeAlias(_), Self::TypeAlias(_))
                | (
                    DeclPath::Function(_) | DeclPath::InterfaceBody(_),
                    Self::Code(_)
                )
        )
    }
}

/// What a unit provides, in its own package's coordinates. `Vec`s, not maps,
/// so the on-wire order is the deterministic emit order.
#[derive(Clone, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ExportTable {
    /// Every pooled declaration the unit defines — types, functions, and
    /// interface bodies — to its bucket + offset.
    pub objects: Vec<(DeclPath, LocalRef)>,
    /// The unit's COMPLETE local global-slot table: every
    /// [`DeclPath::Function`] / [`DeclPath::InterfaceBody`] at slots `[0, F)`
    /// in emit order, then every [`DeclPath::Let`] at `[F, F + L)`. The linker
    /// sizes and fills the slot pool from it; a slot holds `Object(the
    /// exported object)` for a function or body and `Null` (filled by `$init`)
    /// for a `let`.
    pub globals: Vec<(DeclPath, u32)>,
}

/// One `implements` rule a unit declares, in unit convention. Rules ride the
/// unit that declares them, so each rule's provided-method bodies are objects
/// in that unit's own `code` bucket.
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub struct ProgramImplRuleFrag {
    /// The implemented interface, in the unit's object convention (a local
    /// interface or an import).
    pub interface_head: ObjectIndex,
    /// Pattern type this rule implements the interface for.
    pub for_ty_pattern: TyTemplate,
    /// Interface bounds on each generic parameter of the rule.
    pub generic_param_bounds: Vec<Vec<InterfaceBound>>,
    /// Interface type arguments.
    pub interface_args: Vec<TyTemplate>,
    /// Associated-type bindings, by associated-type name.
    pub interface_assoc: Vec<(Name, TyTemplate)>,
    /// Method name to its provided implementation. Provided-only: an adopted
    /// interface default is not in this table (the resolver adopts it at
    /// dispatch through the interface's `default_fn`).
    pub methods: Vec<(Name, ProgramMethodImplFrag)>,
    /// See [`RuntimeImplRule::field_links`](bex_vm_types::types::RuntimeImplRule::field_links).
    /// Slot indices are layout, not symbols, so they survive linking unchanged.
    pub field_links: Box<[u32]>,
}

/// A provided method's body: an offset into the declaring unit's `code`
/// bucket (a body has no name on any wire) plus the callee's type-argument
/// frame at the impl site.
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub struct ProgramMethodImplFrag {
    pub code_offset: u32,
    pub frame: Vec<TyTemplate>,
}

/// One package's relocatable compiled output for one link.
///
/// Definitions are bucketed by kind so the linker can interleave them
/// pass-major across the units of a package group, reproducing the flat pool
/// order of a whole-program emit. Every index operand inside these objects
/// uses the module-level convention.
#[derive(Clone, Debug, Default, BorshSerialize, BorshDeserialize)]
pub struct CompilationUnit {
    /// The packages this unit reaches, by slot (see [`DepSlot`]).
    pub dependencies: Vec<DependencyEntry>,

    // --- definitions, bucketed by kind ---
    /// `Object::Class` definitions, in declaration order.
    pub classes: Vec<Object>,
    /// `Object::Enum` definitions.
    pub enums: Vec<Object>,
    /// `Object::Interface` definitions.
    pub interfaces: Vec<Object>,
    /// `Object::TypeAlias` definitions (recursive aliases only).
    pub type_alias_objects: Vec<Object>,
    /// Functions, lambdas, interned literals, and this unit's own copies of
    /// the generic-function values it uses, in emit order.
    pub code: Vec<Object>,

    // --- cross-unit tables ---
    /// Every foreign object reference, by ordinal.
    pub object_imports: Vec<ImportEntry>,
    /// Every foreign global-slot reference, by ordinal.
    pub global_imports: Vec<ImportEntry>,
    /// What this unit provides.
    pub exports: ExportTable,
    /// The `implements` rules this unit declares. Their templates carry heads
    /// in the unit convention like every other object of the unit.
    pub impl_rules: Vec<ProgramImplRuleFrag>,
}

/// The whole-package product of a compile, produced beside the unit
/// (rustc's crate metadata beside its codegen units): the interface a
/// dependent compiles against.
#[derive(Clone, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PackageRecord {
    /// Versioned artifact containing the whole-package interface.
    pub interface_blob: Vec<u8>,
}

/// One package's `$init` / `$init_test` tail: the package's `$init_let_{i}`
/// helpers and `$init` (its init part), then its `$init_test` chainer (its
/// test part), with tables of its own in the unit convention.
///
/// A tail is kept apart from the unit because it is placed apart: the linker
/// places every package's init part after its group's regular code in
/// package initialization order, then every package's test part in
/// package-name order — a group-wide `$init` cannot reach another package's
/// `let`s through a slot its own package has no edge for.
///
/// # Operand convention
///
/// Object operand `raw < objects.len()` is tail-local; global operand
/// `raw < slot_objects.len()` is a tail-local slot (the slot's ordinal). An
/// operand `>= IMPORT_BASE` indexes [`Self::object_imports`] /
/// [`Self::global_imports`], resolved through [`Self::dependencies`].
#[derive(Clone, Debug, Default, BorshSerialize, BorshDeserialize)]
pub struct InitTail {
    /// The packages this tail reaches, by slot (see [`DepSlot`]).
    pub dependencies: Vec<DependencyEntry>,
    /// Tail objects in pool order: the init part `[0, test_objects_start)`
    /// then the test part.
    pub objects: Vec<Object>,
    /// Every foreign object reference, by ordinal.
    pub object_imports: Vec<ImportEntry>,
    /// Every foreign global-slot reference, by ordinal.
    pub global_imports: Vec<ImportEntry>,
    /// Tail-local object index of each tail object that owns a global slot,
    /// in slot order: init-part slots `[0, test_slots_start)` then test-part
    /// slots. Every such slot holds `Object(that object)`.
    pub slot_objects: Vec<u32>,
    /// Tail-local index of the package's `$init`, when it has `let`s.
    pub init: Option<u32>,
    /// Tail-local index of the package's `$init_test` chainer, when it has
    /// tests.
    pub init_test: Option<u32>,
    /// Where the test part of [`Self::objects`] begins.
    pub test_objects_start: u32,
    /// Where the test part of [`Self::slot_objects`] begins.
    pub test_slots_start: u32,
}

/// One package's compiled output for one link: what the emitter produces for
/// a package and the linker consumes for it. Carries no package identity —
/// the driver that emits or loads it knows which package it is for — and
/// every reference into another package is by dependency slot and
/// declaration path, so it is a pure function of the package's sources and
/// its dependencies' interfaces.
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub struct EmittedPackage {
    pub unit: CompilationUnit,
    pub record: PackageRecord,
    pub tail: Option<InitTail>,
}

/// One `let` of a session submission, as the emitter recorded it: the tail
/// slot of the helper computing its value, and the binding it commits to.
/// A submission's initializers are listed in execution (dependency) order;
/// the session runs them one by one, so a throw commits the steps before it
/// and nothing after — which is why a submission's tail carries no `$init`.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct SessionInitializer {
    pub helper: u32,
    pub target: ItemPath,
}

/// One session submission's compiled output: the unit holds the
/// submission's own declarations and reaches the session package's earlier
/// ones as imports at [`DepSlot::SELF`]; the tail holds the submission's
/// `let` helpers, addressed by [`Self::initializers`].
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub struct EmittedSubmission {
    pub package: EmittedPackage,
    pub initializers: Vec<SessionInitializer>,
}
