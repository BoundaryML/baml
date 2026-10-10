//! Heap object types and runtime values.
//!
//! `Future` uses `AtomicU8` + `UnsafeCell<MaybeUninit<Value>>` to allow
//! the engine's spawned task and the VM to read/write the heap object
//! concurrently without a data race. See the `Future` doc for the safety
//! argument; the unsafe code here is intentional and necessary.

#![allow(unsafe_code)]
mod class;
mod const_value;
mod containers;
mod decl_path;
mod enums;
mod function;
mod future;
mod interface;
mod object;
mod owner;
mod package;
mod type_alias;
mod type_value;
mod value;

use std::collections::{BTreeSet, HashMap, HashSet};

/// Re-exported from `baml_type`, which owns the naming vocabulary; the
/// runtime spells it unqualified everywhere it builds a declaration.
pub use baml_type::DeclarationName;
use borsh::{BorshDeserialize, BorshSerialize};
pub use class::*;
pub use const_value::*;
pub use containers::*;
pub use decl_path::*;
pub use enums::*;
pub use function::*;
pub use future::*;
use indexmap::IndexMap;
pub use interface::*;
pub use object::*;
pub use owner::*;
pub use package::*;
pub use tokio_util::sync::CancellationToken;
pub use type_alias::*;
pub use type_value::*;
pub use value::*;

use crate::{
    heap_ptr::HeapPtr,
    indexable::{GlobalIndex, ObjectIndex, ObjectPool},
};

// ============================================================================
// Type Tags for Jump Table Dispatch
// ============================================================================

/// Global type tag constants for runtime type identification.
///
/// Re-exported from `baml_typetags` crate to maintain backwards compatibility.
/// These are used by the `TypeTag` instruction to extract a type identifier
/// from any value for jump table dispatch on union types.
pub mod type_tags {
    pub use baml_type::typetag::*;
}

/// Compiled program ready for execution.
///
/// This is what `baml_compiler_emit` produces. It contains all the objects and globals
/// needed to run a BAML program.
///
/// Note: At compile time, globals use `ConstValue` (with `ObjectIndex` for object refs).
/// At load time (`BexEngine::new`), these are converted to `Value` (with `HeapPtr`).
#[derive(Clone, Debug, Default, BorshSerialize, BorshDeserialize)]
pub struct Program {
    /// Object pool containing functions, classes, strings, etc.
    pub objects: ObjectPool,

    /// Global variables (converted from `ConstValue` to Value at load time).
    ///
    /// # Init-only invariant
    /// Globals are populated by `$init` (top-level let bindings) at engine
    /// load time and **not** mutated again. The runtime freezes them into a
    /// shared `Arc<[Value]>` after `$init` finishes; only `$init` may emit a
    /// `StoreGlobal` against the still-mutable pool. See `Instruction::StoreGlobal`.
    pub globals: Vec<ConstValue>,

    /// Per-package program structure (global-index-keyed), by package
    /// ordinal. A package's identity in the executable is its position here;
    /// its `name` is display metadata (and the spelling the prelude is bound
    /// by at load). In program order — the language packages first, then the
    /// root and every package it reaches breadth-first by edge name: a
    /// function of the world graph, an order and not a key. Holds each
    /// package's classes, enums, interfaces, impl rules, and
    /// recursive type aliases. The loader allocates the heap `Object::Package`
    /// / `Object::Interface` / `Object::ImplRule` objects and the `vm.packages`
    /// index from this, resolving each `ObjectIndex` to a compile-time
    /// `HeapPtr` (every slot is pre-allocated, so cross-package references are
    /// order-independent). The single source of truth for interface dispatch,
    /// named-item lookup, and recursive-alias rendering.
    pub packages: Vec<ProgramPackage>,
    /// The packages whose `$init` runs, as ordinals into [`Self::packages`],
    /// in initialization order: every package after each package with `let`s
    /// it reaches, directly or through packages without any; packages free to
    /// initialize together go by name, then by position in the link set.
    pub init_order: Vec<u32>,
    /// The package the program was compiled for — the world's root — as an
    /// ordinal into [`Self::packages`]: the viewpoint every host-supplied
    /// name resolves from, and whose tests a test run collects. Always
    /// linked, even when it declares nothing yet.
    pub root: u32,

    /// Conservative source-content identity of the compiled file set
    /// (streams spec §2.3): SHA-256 over the domain string, compiler
    /// version, and every (path, bytes) pair. `None` when the compiling
    /// host chose not to provide one (e.g. mounted-unit links) — the
    /// profiler then falls back to a random per-engine `ProgramId`, which
    /// over-splits (the safe direction).
    ///
    /// In-memory metadata, NOT compiled content: it is `borsh(skip)`ped so
    /// program/unit byte-identity oracles compare compiled output only,
    /// and per-file `CompilationUnit`s never carry project-wide state. A
    /// host that materializes a `Program` from stored bytes restamps it
    /// (the CLI bytecode cache recomputes from the project files it just
    /// validated); a host that cannot leaves `None`.
    #[borsh(skip)]
    pub source_content_hash: Option<[u8; 32]>,
}

/// An executable that breaks the format's laws: an index outside its pool, a
/// table naming an object of another kind, a head or switch the loader
/// could not bind. See [`Program::validate`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid program: {0}")]
pub struct InvalidProgram(pub String);

impl Program {
    pub fn new() -> Self {
        Self::default()
    }

    /// Name a compiled type head without binding it to a heap.
    ///
    /// Like the loader, resolve the tag's static index only when the pooled
    /// declaration carries that exact tag. Never dereference a head's pointer.
    ///
    /// # Errors
    ///
    /// [`crate::UnnameableHead`] if the tag does not identify a named declaration
    /// in this program.
    pub fn type_head_name(
        &self,
        head: &crate::TypeHead,
    ) -> Result<baml_type::TypeName, crate::UnnameableHead> {
        let tag = head.tag();
        tag.static_index()
            .and_then(|index| self.objects.get(index))
            .filter(|object| object.declaration_tag() == Some(tag))
            .and_then(Object::declaration_name)
            .cloned()
            .ok_or(crate::UnnameableHead(tag))
    }

    /// Check the executable against the format's laws — every index within
    /// its pool, every table naming an object of the kind it says, every
    /// head bindable to the declaration its tag encodes, every type switch
    /// solved, the init order exactly the packages with an `$init` — so that
    /// nothing downstream indexes into a program that breaks them. A linked
    /// program satisfies them by construction; a decoded one is checked here,
    /// at the one road into a VM, so a corrupt or forged artifact is refused
    /// rather than a panic in the loader.
    ///
    /// # Errors
    ///
    /// The first law broken, named.
    pub fn validate(&self) -> Result<(), InvalidProgram> {
        let invalid = |message: String| InvalidProgram(message);
        let object_count = self.objects.len();
        let object = |index: ObjectIndex, role: &str| -> Result<&Object, InvalidProgram> {
            self.objects.get(index.raw()).ok_or_else(|| {
                invalid(format!(
                    "{role} names object {} of {object_count}",
                    index.raw()
                ))
            })
        };
        let function =
            |index: ObjectIndex, role: &str, is_body: bool| -> Result<(), InvalidProgram> {
                match object(index, role)? {
                    Object::Function(function) if function.is_interface_body == is_body => Ok(()),
                    other => Err(invalid(format!(
                        "{role} names a {:?} object where {} is required",
                        ObjectType::of(other),
                        if is_body {
                            "an interface body"
                        } else {
                            "a function"
                        }
                    ))),
                }
            };

        let package_count = self.packages.len();
        if self.root as usize >= package_count {
            return Err(invalid(format!(
                "the root is package {} of {package_count}",
                self.root
            )));
        }
        for package in &self.packages {
            let name = &package.name;
            let mut edge_names = HashSet::new();
            for edge in &package.edges {
                if edge.target as usize >= package_count {
                    return Err(invalid(format!(
                        "package `{name}` edge `{}` names package {} of {package_count}",
                        edge.name, edge.target
                    )));
                }
                if !edge_names.insert(&edge.name) {
                    return Err(invalid(format!(
                        "package `{name}` reaches two packages as `{}`",
                        edge.name
                    )));
                }
            }
            let declarations = |kind: &str,
                                table: &IndexMap<LocalName, ObjectIndex>,
                                holds: fn(&Object) -> bool|
             -> Result<(), InvalidProgram> {
                for (item, &index) in table {
                    let role = format!("package `{name}` {kind} `{item}`");
                    let pooled = object(index, &role)?;
                    if !holds(pooled) {
                        return Err(invalid(format!(
                            "{role} names a {:?} object",
                            ObjectType::of(pooled)
                        )));
                    }
                }
                Ok(())
            };
            declarations("class", &package.classes, |object| {
                matches!(object, Object::Class(_))
            })?;
            declarations("enum", &package.enums, |object| {
                matches!(object, Object::Enum(_))
            })?;
            declarations("interface", &package.interfaces, |object| {
                matches!(object, Object::Interface(_))
            })?;
            declarations("type alias", &package.type_aliases, |object| {
                matches!(object, Object::TypeAlias(_))
            })?;
            for (&interface, rules) in &package.impl_rules {
                let role = format!("package `{name}` implements object {}", interface.raw());
                if !matches!(object(interface, &role)?, Object::Interface(_)) {
                    return Err(invalid(format!("{role}, which is not an interface")));
                }
                for rule in rules {
                    if rule.interface_head != interface {
                        return Err(invalid(format!(
                            "{role} with a rule whose head is object {}",
                            rule.interface_head.raw()
                        )));
                    }
                    for (method, body) in &rule.methods {
                        function(body.fqn, &format!("{role}, method `{method}`,"), true)?;
                    }
                }
            }
            if let Some(init) = package.init {
                function(init, &format!("package `{name}` `$init`"), false)?;
            }
            if let Some(test_init) = package.test_init {
                function(test_init, &format!("package `{name}` `$init_test`"), false)?;
            }
            for (path, &ordinal) in &package.globals {
                if !path.owns_global_slot() {
                    return Err(invalid(format!(
                        "package `{name}` slots {path}, which owns no cell"
                    )));
                }
                let slot = package.slot_base.raw() + ordinal as usize;
                let Some(cell) = self.globals.get(slot) else {
                    return Err(invalid(format!(
                        "package `{name}` slots {path} at cell {slot} of {}",
                        self.globals.len()
                    )));
                };
                match path {
                    DeclPath::Function(_) | DeclPath::InterfaceBody(_) => {
                        let ConstValue::Object(index) = cell else {
                            return Err(invalid(format!(
                                "package `{name}` cell for {path} holds no object"
                            )));
                        };
                        function(
                            *index,
                            &format!("package `{name}` cell for {path}"),
                            matches!(path, DeclPath::InterfaceBody(_)),
                        )?;
                    }
                    DeclPath::Let(_) => {}
                    DeclPath::Class(_)
                    | DeclPath::Enum(_)
                    | DeclPath::Interface(_)
                    | DeclPath::TypeAlias(_) => {
                        unreachable!("a type owns no cell: checked above")
                    }
                }
            }
        }

        let with_init: BTreeSet<usize> = self
            .packages
            .iter()
            .enumerate()
            .filter(|(_, package)| package.init.is_some())
            .map(|(ordinal, _)| ordinal)
            .collect();
        let mut ordered = BTreeSet::new();
        for &ordinal in &self.init_order {
            let ordinal = ordinal as usize;
            if ordinal >= package_count {
                return Err(invalid(format!(
                    "the init order names package {ordinal} of {package_count}"
                )));
            }
            if !ordered.insert(ordinal) {
                return Err(invalid(format!(
                    "the init order names package {ordinal} twice"
                )));
            }
        }
        if ordered != with_init {
            return Err(invalid(
                "the init order is not exactly the packages with an `$init`".to_string(),
            ));
        }

        for (slot, cell) in self.globals.iter().enumerate() {
            if let ConstValue::Object(index) = cell {
                object(*index, &format!("cell {slot}"))?;
            }
        }

        for (index, pooled) in self.objects.iter().enumerate() {
            let role = format!("object {index}");
            let mut failure = None;
            crate::head_walk::visit_object_heads(pooled, &mut |head| {
                if failure.is_some() {
                    return;
                }
                let tag = head.tag();
                let bound = tag
                    .static_index()
                    .and_then(|declaration| self.objects.get(declaration))
                    .is_some_and(|declaration| declaration.declaration_tag() == Some(tag));
                if !bound {
                    failure = Some(invalid(format!(
                        "{role} carries a head with tag {tag:?}, which names no declaration"
                    )));
                }
            });
            if let Some(failure) = failure {
                return Err(failure);
            }
            match pooled {
                Object::Function(function) => {
                    crate::relink::visit_index_operands_ref(function, |operand| {
                        if failure.is_some() {
                            return;
                        }
                        let in_range = match operand {
                            crate::relink::IndexOperandRef::Object(index) => {
                                index.raw() < object_count
                            }
                            crate::relink::IndexOperandRef::Global(slot) => {
                                slot.raw() < self.globals.len()
                            }
                        };
                        if !in_range {
                            failure = Some(invalid(format!(
                                "{role} (function `{}`) references an operand outside the program",
                                function.name
                            )));
                        }
                    });
                    if let Some(failure) = failure {
                        return Err(failure);
                    }
                    for instruction in &function.bytecode.instructions {
                        match instruction {
                            crate::bytecode::Instruction::LoadCurrentPackage(package)
                                if *package >= package_count =>
                            {
                                return Err(invalid(format!(
                                    "{role} (function `{}`) loads package {package} of \
                                     {package_count}",
                                    function.name
                                )));
                            }
                            // The VM reads the dispatch value at `args_offset +
                            // self_arg`; past `nargs` that is past the stack.
                            crate::bytecode::Instruction::VirtualCall {
                                nargs, self_arg, ..
                            } if self_arg >= nargs => {
                                return Err(invalid(format!(
                                    "{role} (function `{}`) dispatches a virtual call on \
                                     argument {self_arg} of {nargs}",
                                    function.name
                                )));
                            }
                            _ => {}
                        }
                    }
                    if let Some((table, _)) = function
                        .bytecode
                        .switch_tables
                        .iter()
                        .enumerate()
                        .find(|(_, table)| !table.dispatch.is_dispatchable())
                    {
                        return Err(invalid(format!(
                            "{role} (function `{}`) switch table {table} is not solved",
                            function.name
                        )));
                    }
                }
                Object::Interface(interface) => {
                    if let Some(default) = &interface.structural_default {
                        function(
                            default.function,
                            &format!("{role} structural default"),
                            false,
                        )?;
                    }
                    for method in &interface.methods {
                        if let Some(default) = method.default {
                            function(
                                default,
                                &format!("{role} (interface) default of `{}`", method.name),
                                true,
                            )?;
                        }
                    }
                }
                Object::Class(class) => {
                    for (name, method) in &class.methods {
                        function(
                            method.function,
                            &format!("{role} (class) method `{name}`"),
                            false,
                        )?;
                    }
                }
                Object::GenericFunction(generic)
                    if generic.function.raw() >= self.globals.len() =>
                {
                    return Err(invalid(format!(
                        "{role} (generic value) references cell {} of {}",
                        generic.function.raw(),
                        self.globals.len()
                    )));
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// Add an object to the pool and return its index.
    pub fn add_object(&mut self, object: Object) -> usize {
        let idx = self.objects.len();
        self.objects.push(object);
        idx
    }

    /// Flatten every package's recursive type aliases into one map keyed by
    /// declaration identity (only recursive aliases survive; non-recursive ones
    /// are expanded inline), reconstructing each alias's declared name from its
    /// package + `LocalName`. The shape output-format rendering consumes.
    ///
    /// Aliases are `Object::TypeAlias` declarations, so this dereferences each
    /// through the object pool rather than reading a side map — which is also
    /// where the identity comes from.
    pub fn recursive_type_aliases(&self) -> IndexMap<baml_type::TaggedTypeName, crate::RealizedTy> {
        let mut out = IndexMap::new();
        for package in &self.packages {
            let pkg_name = &package.name;
            for (local, idx) in &package.type_aliases {
                let Some(Object::TypeAlias(alias)) = self.objects.get(idx.raw()) else {
                    // An index that does not resolve to an alias means the pool
                    // and the package map disagree — skip rather than guess.
                    continue;
                };
                let qtn = baml_type::TypeName::new(
                    pkg_name.clone(),
                    local.namespace.clone(),
                    local.name.clone(),
                );
                out.insert(
                    baml_type::TaggedTypeName::new(
                        alias.type_tag,
                        baml_type::DeclarationName::Declared(qtn),
                    ),
                    alias.definition.clone(),
                );
            }
        }
        out
    }

    /// Add a global value (`ConstValue`, converted to Value at load time).
    pub fn add_global(&mut self, value: ConstValue) {
        self.globals.push(value);
    }

    /// The packages a host can name, as the root reaches them: the root under
    /// its own name, then each package under the root's edge to it. A package
    /// the root reaches only transitively has no host-facing name.
    fn root_viewpoint(&self) -> impl Iterator<Item = (&baml_base::Name, &ProgramPackage)> {
        let root = &self.packages[self.root as usize];
        std::iter::once((&root.name, root)).chain(
            root.edges
                .iter()
                .map(|edge| (&edge.name, &self.packages[edge.target as usize])),
        )
    }

    /// The executable's callables by rendered name — `pkg.ns.name` for a free
    /// function, `pkg.ns.Class.name` for a method, `pkg` being how the root
    /// reaches the package (the root by its own name, a dependency by the
    /// root's edge to it; a transitively reached package has no host-facing
    /// name). The view behind the
    /// surfaces that legitimately start from a host-supplied name (the run
    /// entry point, test discovery, reflection by name), derived from the
    /// package tables: the executable carries no such spelling, and nothing
    /// resolves through it internally.
    pub fn rendered_callables(&self) -> HashMap<String, RenderedCallable> {
        let mut out = HashMap::new();
        for (spelling, package) in self.root_viewpoint() {
            for path in package.globals.keys() {
                let name = match path {
                    DeclPath::Function(FnPath::Free(local)) => format!("{spelling}.{local}"),
                    DeclPath::Function(FnPath::Method { class, name }) => {
                        format!("{spelling}.{class}.{name}")
                    }
                    DeclPath::Let(_) | DeclPath::InterfaceBody(_) => continue,
                    DeclPath::Class(_)
                    | DeclPath::Enum(_)
                    | DeclPath::Interface(_)
                    | DeclPath::TypeAlias(_) => unreachable!("a type owns no cell"),
                };
                let slot = package
                    .global_slot(path)
                    .unwrap_or_else(|| unreachable!("the path was read from the slot map"));
                let ConstValue::Object(object) = self.globals[slot.raw()] else {
                    unreachable!("a callable's cell holds its object")
                };
                out.insert(name, RenderedCallable { object, slot });
            }
        }
        out
    }

    /// The executable's top-level `let`s by rendered name (`pkg.ns.name`) to
    /// the global slot each holds — derived like [`Self::rendered_callables`].
    pub fn rendered_lets(&self) -> HashMap<String, GlobalIndex> {
        let mut out = HashMap::new();
        for (spelling, package) in self.root_viewpoint() {
            for path in package.globals.keys() {
                if let DeclPath::Let(local) = path {
                    let slot = package
                        .global_slot(path)
                        .unwrap_or_else(|| unreachable!("the path was read from the slot map"));
                    out.insert(format!("{spelling}.{local}"), slot);
                }
            }
        }
        out
    }
}

/// A callable of the executable as a host names it: its function object and
/// the cell it links as, which holds that object.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RenderedCallable {
    pub object: ObjectIndex,
    pub slot: GlobalIndex,
}

// ============================================================================
// SysOp Error/Panic Contract Categories
// ============================================================================

/// Contract-level error categories for `sys_op` throw contracts.
///
/// These are the finite set of categories that `#[throws(...)]` annotations
/// reference. Each [`VmBamlError`](crate::errors::VmBamlError) variant maps
/// to exactly one category via `VmBamlError::category()`. Rich detail stays
/// in `VmBamlError`; this enum is purely for contract enforcement and
/// compiler analysis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, BorshSerialize, BorshDeserialize)]
pub enum SysOpErrorCategory {
    Io,
    Timeout,
    InvalidArgument,
    /// A parser surfaced a structurally-bad input (e.g. UTF-8 decode, JSON
    /// parse, base64 decode). Distinct from `InvalidArgument` so callers
    /// can distinguish "your argument shape was wrong" from "the bytes/
    /// stream we tried to parse are malformed".
    ParseError,
    Unsupported,
    AccessError,
    RenderPrompt,
    LlmClient,
    /// Runtime source compilation was rejected with compiler diagnostics.
    CompilationError,
    /// A live Session already has an evaluation in flight.
    SessionBusy,
    /// A host-language callable raised an exception or invalid-argument error.
    HostCallable,
}

impl std::fmt::Display for SysOpErrorCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io => write!(f, "Io"),
            Self::Timeout => write!(f, "Timeout"),
            Self::InvalidArgument => write!(f, "InvalidArgument"),
            Self::ParseError => write!(f, "ParseError"),
            Self::Unsupported => write!(f, "Unsupported"),
            Self::AccessError => write!(f, "AccessError"),
            Self::RenderPrompt => write!(f, "RenderPrompt"),
            Self::LlmClient => write!(f, "LlmClient"),
            Self::CompilationError => write!(f, "CompilationError"),
            Self::SessionBusy => write!(f, "SessionBusy"),
            Self::HostCallable => write!(f, "HostCallable"),
        }
    }
}

/// Contract-level panic categories for `sys_op` panic contracts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, BorshSerialize, BorshDeserialize)]
pub enum SysOpPanicCategory {
    HostPanic,
}

impl std::fmt::Display for SysOpPanicCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::HostPanic => write!(f, "HostPanic"),
        }
    }
}

// ============================================================================
// External Operations
// ============================================================================

// System operations that run outside the VM.
//
// Generated from `.baml` `$rust_io_function` definitions in `baml_builtins2`.
// The enum, `path()`, `sys_op_for_path()`, and `Display` are all generated
// by `baml_builtins2_codegen` at build time.
// SysOp enum, path(), allowed_error_categories(), allowed_panic_categories(),
// Display, and sys_op_for_path() — generated from .baml $rust_io_function definitions.
include!(concat!(env!("OUT_DIR"), "/sys_op_generated.rs"));

#[cfg(feature = "heap_debug")]
#[derive(Clone, Debug)]
pub enum SentinelKind {
    Uninit,
    TlabCanary {
        chunk_start: usize,
        chunk_end: usize,
    },
}

/// Box every unique `ConstValue::Float` reachable from `compile_time_objects`
/// (in `Object::Function` constants) and `globals` into a fresh
/// `Object::Float` entry appended to `compile_time_objects`, and rewrite the
/// function `ConstValue::Float` entries to `ConstValue::Object(idx)`. Returns
/// the bit-pattern → object-index map so callers can rewrite their globals.
///
/// Required because the tagged-pointer `Value` encoding can no longer hold a
/// float inline.
///
/// Globals are *not* rewritten in place — callers typically need to consume
/// them by value (to convert `ConstValue` → `Value`) and rewrite during that
/// pass.
pub fn box_compile_time_floats(
    compile_time_objects: &mut Vec<Object>,
    globals: &[crate::ConstValue],
) -> HashMap<u64, usize> {
    let mut float_indices: HashMap<u64, usize> = HashMap::new();
    // Pre-scan to discover unique floats.
    for obj in &*compile_time_objects {
        if let Object::Function(func) = obj {
            for cv in &func.bytecode.constants {
                if let ConstValue::Float(f) = cv {
                    let next_idx = compile_time_objects.len() + float_indices.len();
                    float_indices.entry(f.to_bits()).or_insert(next_idx);
                }
            }
        }
    }
    for cv in globals {
        if let ConstValue::Float(f) = cv {
            let next_idx = compile_time_objects.len() + float_indices.len();
            float_indices.entry(f.to_bits()).or_insert(next_idx);
        }
    }
    // Append boxes in index order.
    let mut float_entries: Vec<(u64, usize)> =
        float_indices.iter().map(|(k, v)| (*k, *v)).collect();
    float_entries.sort_by_key(|(_, idx)| *idx);
    for (bits, _) in float_entries {
        compile_time_objects.push(Object::Float(f64::from_bits(bits)));
    }
    // Rewrite each function-constant ConstValue::Float -> ConstValue::Object(idx).
    for obj in compile_time_objects.iter_mut() {
        if let Object::Function(func) = obj {
            for cv in &mut func.bytecode.constants {
                if let ConstValue::Float(f) = cv {
                    let idx = float_indices[&f.to_bits()];
                    *cv = ConstValue::Object(crate::indexable::ObjectIndex::from_raw(idx));
                }
            }
        }
    }
    float_indices
}

/// `float.to_string()`: see [`bex_lang::float::format`].
pub use bex_lang::float::format as format_float;

// Error class / instance enums — generated from `errors.baml` class definitions.
// ErrorClass (tag enum), ErrorInstance (with Value fields), associated methods.
include!(concat!(env!("OUT_DIR"), "/errors_generated.rs"));
// Panic class / instance enums — generated from `panics.baml` class definitions.
// PanicClass (tag enum), PanicInstance (with Value fields), associated methods.
include!(concat!(env!("OUT_DIR"), "/panics_generated.rs"));

/// A mutable cell wrapping a single captured value.
///
/// Variables that are closed over are heap-allocated as `Cell` objects so that
/// both the enclosing scope and any closures share the same storage.
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub struct Cell {
    pub value: AtomicValueSlot,
}

impl Cell {
    pub fn new(value: Value) -> Self {
        Self {
            value: AtomicValueSlot::new(value),
        }
    }

    #[inline]
    pub fn load(&self) -> Value {
        self.value.load()
    }

    #[inline]
    pub fn store(&self, value: Value) {
        self.value.store(value);
    }
}

/// Types of values.
///
/// Used for checking type errors at runtime. We can probably use some lib
/// that creates this automatically based on the [`Value`] enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum Type {
    OmittedArg,
    Int,
    Float,
    Bool,
    Object(ObjectType),
}

impl std::fmt::Display for Type {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Type::OmittedArg => write!(f, "omitted argument"),
            Type::Int => write!(f, "int"),
            Type::Float => write!(f, "float"),
            Type::Bool => write!(f, "bool"),
            Type::Object(object_type) => write!(f, "{object_type}"),
        }
    }
}

impl<O: Into<ObjectType>> From<O> for Type {
    fn from(obj: O) -> Self {
        Type::Object(obj.into())
    }
}

impl Type {
    /// Get the type of a value.
    ///
    /// Heap-boxed floats are normalised back to the top-level `Type::Float`
    /// so callers comparing `expected` against `got` see the same variant
    /// regardless of which side originated as an inline label vs. a
    /// runtime heap deref.
    pub fn of(value: &Value, when_object: impl FnOnce(HeapPtr) -> ObjectType) -> Self {
        match value.kind() {
            ValueKind::OmittedArg => Type::OmittedArg,
            ValueKind::Int(_) => Type::Int,
            ValueKind::Bool(_) => Type::Bool,
            ValueKind::Object(ptr) => match when_object(ptr) {
                ObjectType::Float => Type::Float,
                other => Type::Object(other),
            },
            // TODO: Actually?
            ValueKind::Null => Type::Object(ObjectType::Any),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ConstValue, HeapPtr, Instance, Program, Type, Value};

    #[test]
    fn compiled_head_names_require_a_matching_named_declaration() {
        use baml_type::{TypeName, typetag::TypeTag};

        use crate::{DeclarationName, Enum, Object, TypeHead, types::Owner};

        let mut program = Program::default();
        let tag = TypeTag::of_static_index(0);
        let head = TypeHead::unresolved(tag);
        assert!(program.type_head_name(&head).is_err());

        program
            .objects
            .push(Object::String("not a declaration".into()));
        assert!(program.type_head_name(&head).is_err());

        let name = TypeName::from_dotted_path("user.Status");
        program.objects[crate::ObjectIndex::from_raw(0)] = Object::Enum(Box::new(Enum {
            name: DeclarationName::Declared(name.clone()),
            type_tag: TypeTag::of_static_index(1),
            variants: vec![],
            description: None,
            alias: None,
            docstring: None,
            other: indexmap::IndexMap::new(),
            owner: Owner::anonymous(),
        }));
        assert!(program.type_head_name(&head).is_err());

        program.objects[crate::ObjectIndex::from_raw(0)].assign_declaration_tag(tag);
        assert_eq!(program.type_head_name(&head), Ok(name));
        assert!(!head.is_resolved());
        assert!(head.to_name().is_err());

        let Object::Enum(enm) = &mut program.objects[crate::ObjectIndex::from_raw(0)] else {
            unreachable!()
        };
        enm.name = DeclarationName::Anonymous(baml_base::Name::new("Status"));
        assert!(program.type_head_name(&head).is_err());
        assert!(
            program
                .type_head_name(&TypeHead::unresolved(TypeTag::fresh_dynamic()))
                .is_err()
        );
    }

    /// The empty program names a root it does not have; nothing may index
    /// into it, and the check says so instead.
    #[test]
    fn an_empty_program_is_refused_for_its_root() {
        let error = Program::default()
            .validate()
            .expect_err("no package for the root");
        assert!(error.0.contains("the root is package 0 of 0"), "{error}");
    }

    #[test]
    fn omitted_arg_roundtrip_stays_in_sync() {
        let value = ConstValue::OmittedArg.to_value(|_| unreachable!("no object expected"));

        assert_eq!(value, Value::OMITTED_ARG);
        assert_eq!(
            Type::of(&value, |_| unreachable!("omitted arg is not an object")),
            Type::OmittedArg
        );
        assert_eq!(value.to_string(), "<omitted>");
    }

    #[test]
    fn instance_field_helpers_load_and_store_checked_slots() {
        let instance = Instance::new(
            HeapPtr::null(),
            Box::new([]),
            vec![Value::int(10), Value::int(20)],
        );

        assert_eq!(instance.field_len(), 2);
        assert_eq!(instance.try_load_field(0), Some(Value::int(10)));
        assert_eq!(instance.try_load_field(2), None);
        assert_eq!(instance.load_field(1), Value::int(20));

        assert_eq!(instance.try_store_field(1, Value::int(99)), Ok(()));
        assert_eq!(instance.try_load_field(1), Some(Value::int(99)));
        assert_eq!(instance.try_store_field(2, Value::int(123)), Err(2));
    }

    #[test]
    #[should_panic(expected = "index out of bounds")]
    fn instance_load_field_panics_for_invalid_slot() {
        let instance = Instance::new(HeapPtr::null(), Box::new([]), vec![Value::int(10)]);

        let _ = instance.load_field(1);
    }
}
