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
//! ([`crate::link`]) binds each dependency slot to a package, matches every
//! import against the bound package's own exports, and lays the units out
//! into one runnable [`Program`](crate::Program).
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
//! their `ObjectIndex` / `GlobalIndex` VALUES:
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
use borsh::{BorshDeserialize, BorshSerialize};

use crate::{
    Object, ObjectIndex, TyTemplate,
    types::{ImplCoherenceKey, InterfaceBound, LocalName},
};

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

/// A function's path within its package.
#[derive(Clone, Debug, PartialEq, Eq, Hash, BorshSerialize, BorshDeserialize)]
pub enum FnPath {
    /// A free function.
    Free(ItemPath),
    /// A class-inherent method (static or instance), reached through the
    /// class's method table.
    Method { class: ItemPath, name: Name },
}

/// The interface an impl-provided body belongs to, spelled from the body's
/// own package: [`baml_type::Package::Local`] for an interface of that
/// package, [`baml_type::Package::Dep`] by manifest edge otherwise.
///
/// This is an identity COMPONENT of the body, not a reference the linker
/// binds (a body is reached rule-relatively; the direct-call import resolves
/// the body itself). Edges are manifest-fixed, so every submission of a
/// session spells the same interface the same way — which a dependency-table
/// slot, numbered per table in first-use order, would not.
#[derive(Clone, Debug, PartialEq, Eq, Hash, BorshSerialize, BorshDeserialize)]
pub struct InterfaceKey {
    pub package: baml_type::Package,
    pub path: ItemPath,
}

/// The identity of an interface-machinery body. A body has no runtime name:
/// an adopted default is reached through its interface object, a provided
/// method through its impl rule. These keys exist so a later session
/// submission can call an earlier submission's body directly.
#[derive(Clone, Debug, PartialEq, Eq, Hash, BorshSerialize, BorshDeserialize)]
pub enum BodyKey {
    /// An interface's default-method body; the interface is declared by the
    /// body's own package.
    Default { interface: ItemPath, method: Name },
    /// A method an `implements` block provides. Importable only at
    /// [`DepSlot::SELF`]: an impl-provided body is never addressed across
    /// packages (dispatch is virtual). Boxed: the coherence key carries type
    /// templates, and a [`DeclPath`] is a map key everywhere else.
    ImplMethod(Box<ImplBodyKey>),
}

/// The identity of an impl-provided body: the interface, the impl's coherence
/// key (injective over admitted impls), and the method name.
#[derive(Clone, Debug, PartialEq, Eq, Hash, BorshSerialize, BorshDeserialize)]
pub struct ImplBodyKey {
    pub interface: InterfaceKey,
    pub coherence: ImplCoherenceKey,
    pub method: Name,
}

/// A declaration's coordinates within its package. The variant IS the kind,
/// so an import of one kind can never bind an export of another.
#[derive(Clone, Debug, PartialEq, Eq, Hash, BorshSerialize, BorshDeserialize)]
pub enum DeclPath {
    Class(ItemPath),
    Enum(ItemPath),
    Interface(ItemPath),
    /// A recursive alias (the only kind pooled; others expand at lowering).
    TypeAlias(ItemPath),
    /// A top-level `let`. Owns a global slot and no pool object.
    Let(ItemPath),
    /// A named function. Owns a pool object and a global slot.
    Function(FnPath),
    /// An interface-machinery body. Owns a pool object and a global slot,
    /// and appears in no runtime name table.
    InterfaceBody(BodyKey),
}

impl DeclPath {
    /// Is this a type declaration — one whose references bake a type tag?
    #[must_use]
    pub fn is_type(&self) -> bool {
        matches!(
            self,
            Self::Class(_) | Self::Enum(_) | Self::Interface(_) | Self::TypeAlias(_)
        )
    }

    /// Does this declaration own a global slot?
    #[must_use]
    pub fn owns_global_slot(&self) -> bool {
        matches!(
            self,
            Self::Let(_) | Self::Function(_) | Self::InterfaceBody(_)
        )
    }

    /// Does this declaration own a pool object?
    #[must_use]
    pub fn owns_object(&self) -> bool {
        !matches!(self, Self::Let(_))
    }

    /// Does `local_ref` name the bucket a declaration of this kind is pooled
    /// in? Never true for a `let`, which owns no pool object.
    #[must_use]
    pub fn matches_bucket(&self, local_ref: LocalRef) -> bool {
        matches!(
            (self, local_ref),
            (Self::Class(_), LocalRef::Class(_))
                | (Self::Enum(_), LocalRef::Enum(_))
                | (Self::Interface(_), LocalRef::Interface(_))
                | (Self::TypeAlias(_), LocalRef::TypeAlias(_))
                | (
                    Self::Function(_) | Self::InterfaceBody(_),
                    LocalRef::Code(_)
                )
        )
    }
}

impl std::fmt::Display for DeclPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Class(path) => write!(f, "class {path}"),
            Self::Enum(path) => write!(f, "enum {path}"),
            Self::Interface(path) => write!(f, "interface {path}"),
            Self::TypeAlias(path) => write!(f, "type alias {path}"),
            Self::Let(path) => write!(f, "let {path}"),
            Self::Function(FnPath::Free(path)) => write!(f, "function {path}"),
            Self::Function(FnPath::Method { class, name }) => {
                write!(f, "method {class}.{name}")
            }
            Self::InterfaceBody(BodyKey::Default { interface, method }) => {
                write!(f, "default body {interface}.{method}")
            }
            Self::InterfaceBody(BodyKey::ImplMethod(body)) => write!(
                f,
                "impl body <(_ as {}.{})>.{}",
                body.interface.package.as_name(),
                body.interface.path,
                body.method
            ),
        }
    }
}

/// An import: the declaration a unit references, addressed inside the package
/// bound to `dep`.
#[derive(Clone, Debug, PartialEq, Eq, Hash, BorshSerialize, BorshDeserialize)]
pub struct DeclKey {
    pub dep: DepSlot,
    pub path: DeclPath,
}

/// One entry of a unit's import table. The table is TOTAL over the foreign
/// declarations the unit references in any way — by an index operand, or only
/// by a type head baked into a type operand — so a reference the executable
/// must bind is never implied by an operand the linker cannot see.
#[derive(Clone, Debug, PartialEq, Eq, Hash, BorshSerialize, BorshDeserialize)]
pub struct ImportEntry {
    pub key: DeclKey,
    /// For a type declaration, the tag this unit's bytecode carries for it
    /// (its heads, jump tables, and type operands) — a relocation record the
    /// runtime binder maps to the bound declaration's live head. `Some` iff
    /// `key.path` is a type kind.
    pub baked_tag: Option<baml_type::typetag::TypeTag>,
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
    /// See [`RuntimeImplRule::field_links`](crate::types::RuntimeImplRule::field_links).
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
    /// The `implements` rules this unit declares.
    pub impl_rules: Vec<ProgramImplRuleFrag>,
}

/// The whole-package products of a compile, produced beside the unit
/// (rustc's crate metadata beside its codegen units): what the package exports
/// by name, and the interface a dependent compiles against.
#[derive(Clone, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PackageRecord {
    /// All source-visible declaration names (types, aliases, and values),
    /// including aliases that have no pool object of their own.
    pub exported_names: Vec<LocalName>,
    /// Exported callables by their package-local name, to the function each
    /// resolves to, in interface order.
    pub functions: Vec<(LocalName, FnPath)>,
    /// Versioned artifact containing the whole-package enriched interface.
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
