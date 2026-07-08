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
//! This module is Stage 1 of the plan: it defines the format and derives borsh.
//! Nothing in the real compile path produces or consumes a unit yet.

use baml_base::Name;
use baml_type::{RuntimeTy, TyTemplate, throw_facts::FunctionThrowFacts};
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
    /// (mirrors `GenericFunction::type_args`).
    pub type_args: Vec<RuntimeTy>,
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

/// A top-level `let` binding carried symbolically for `$init` synthesis
/// (design §2b / §9 R2). Stage 1 only round-trips this; the linker's `$init`
/// pass (Stage 2) consumes it.
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub struct LetDef {
    /// Fully-qualified name of the `let` (owns a global slot in the linked
    /// image; same key space as `Program::let_global_indices`).
    pub fq_name: String,
    /// Package whose `$init` initializes this binding.
    pub package: Name,
    /// Fully-qualified names of the same-run `let`s this initializer reads. The
    /// linker topo-sorts on these edges to order the `$init` body (design §9 R2).
    pub depends_on: Vec<String>,
    /// The compiled initializer helper (an `Object::Function`) that computes
    /// this binding's value. Its internal index operands use the per-unit
    /// convention of §2a; the linker appends it during `$init` synthesis and
    /// wires a `Call` + `StoreGlobal` against the assigned slot. Minimal for
    /// Stage 1 — refined in Stage 2.
    pub init_helper: Object,
}

/// The symbolic (name-referencing, not `ObjectIndex`) twin of a `ProgramPackage`
/// fragment contributed by one unit. The linker merges every unit's fragment
/// into the image's `packages` map, resolving each fully-qualified name to an
/// absolute `ObjectIndex` (design §3b step 5). `Vec` preserves the deterministic
/// emit order the `IndexMap`s in `ProgramPackage` require. Stage 1 only
/// round-trips this; Stage 2 does the real merge.
#[derive(Clone, Debug, Default, BorshSerialize, BorshDeserialize)]
pub struct ProgramPackageFrag {
    /// Local class name to fully-qualified class name (resolved to `ObjectIndex`
    /// at link).
    pub classes: Vec<(LocalName, String)>,
    /// Local enum name to fully-qualified enum name.
    pub enums: Vec<(LocalName, String)>,
    /// Local interface name to fully-qualified interface name.
    pub interfaces: Vec<(LocalName, String)>,
    /// Implemented-interface fully-qualified name to the impl rules declared for
    /// it in this unit (the interface may live in a dependency package).
    pub impl_rules: Vec<(String, Vec<ProgramImplRuleFrag>)>,
    /// Recursive type aliases, carried verbatim (they hold no object refs to
    /// relocate).
    pub recursive_type_aliases: Vec<(LocalName, RuntimeTy)>,
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
/// inside these objects uses the per-unit convention of §2a; Stage 1 exercises
/// only local-only units (empty import tables).
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
    /// Top-level `let`s with their initializer helpers + dependency edges, for
    /// `$init` synthesis.
    pub lets: Vec<LetDef>,
    /// This unit's symbolic contribution to its package's structure.
    pub package_fragment: ProgramPackageFrag,
    /// Pass-5 template-string `{% macro %}` fragments defined in this file.
    pub template_macros: Vec<String>,
    /// Pass-8 compiled test cases defined in this file.
    pub test_cases: Vec<TestCase>,
    /// Per-function throw facts, carried for the seeded-throws dirty-set path.
    /// Not folded into `Program` — consumed by the cache/manifest layer.
    pub throw_facts: Vec<FunctionThrowFacts>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        GlobalIndex, Instruction, ObjectIndex,
        bytecode::Bytecode,
        types::{Class, Function, FunctionCaptureProps, FunctionKind, FunctionOrigin},
    };

    /// Build a minimal bytecode `Object::Function` with the given instructions.
    fn func(name: &str, instructions: Vec<Instruction>) -> Object {
        let bytecode = Bytecode {
            instructions,
            ..Bytecode::default()
        };
        Object::Function(Box::new(Function {
            name: name.to_string(),
            source_file: "user.baml".to_string(),
            arity: 0,
            real_local_count: 0,
            bytecode,
            kind: FunctionKind::Bytecode,
            local_names: Vec::new(),
            debug_locals: Vec::new(),
            span: baml_base::Span::fake(),
            return_type: baml_type::RuntimeTy::unknown(),
            param_names: Vec::new(),
            param_types: Vec::new(),
            param_has_default: Vec::new(),
            display_type_params: Vec::new(),
            display_param_types: Vec::new(),
            display_return_type: String::new(),
            throws_type: None,
            origin: FunctionOrigin::UserDefined,
            body_meta: None,
            capture: FunctionCaptureProps::disabled(),
            function_id: 0,
        }))
    }

    /// Build a minimal non-generic `Object::Class`.
    fn class(name: &str, type_tag: i64) -> Object {
        Object::Class(Box::new(Class {
            name: baml_type::TypeName::local(baml_base::Name::new(name)),
            fields: Vec::new(),
            description: None,
            alias: None,
            type_tag,
            ty_attr: baml_type::TyAttr::default(),
            has_cleanup: false,
            generic_param_count: 0,
        }))
    }

    fn sample_unit() -> CompilationUnit {
        use Instruction as I;
        CompilationUnit {
            source_file: "user.baml".to_string(),
            package: baml_base::Name::new("user"),
            classes: vec![class("MyClass", 100)],
            enums: Vec::new(),
            interfaces: Vec::new(),
            code: vec![
                func(
                    "user.foo",
                    vec![
                        I::AllocInstance {
                            class_obj: ObjectIndex::from_raw(0),
                            ntypeargs: 0,
                        },
                        I::Call {
                            callee: GlobalIndex::from_raw(1),
                            ntypeargs: 0,
                        },
                        I::Return,
                    ],
                ),
                func(
                    "user.bar",
                    vec![I::LoadGlobal(GlobalIndex::from_raw(0)), I::Return],
                ),
            ],
            object_imports: Vec::new(),
            global_imports: Vec::new(),
            exports: ExportTable {
                objects: vec![
                    ("user.MyClass".to_string(), LocalRef::Class(0)),
                    ("user.foo".to_string(), LocalRef::Code(0)),
                    ("user.bar".to_string(), LocalRef::Code(1)),
                ],
                globals: vec![("user.foo".to_string(), 0), ("user.bar".to_string(), 1)],
            },
            lets: Vec::new(),
            package_fragment: ProgramPackageFrag::default(),
            template_macros: Vec::new(),
            test_cases: Vec::new(),
            throw_facts: Vec::new(),
        }
    }

    /// The Stage 1 per-unit gate: a unit round-trips through borsh unchanged.
    ///
    /// `Object` has no `PartialEq`, so structural equality is proved on the
    /// borsh byte vector (the same oracle the design uses — §5).
    #[test]
    fn compilation_unit_borsh_round_trip() {
        let unit = sample_unit();
        let bytes = borsh::to_vec(&unit).expect("serialize unit");
        let round_tripped: CompilationUnit =
            borsh::from_slice(&bytes).expect("deserialize unit");
        let bytes_again = borsh::to_vec(&round_tripped).expect("re-serialize unit");
        assert_eq!(bytes, bytes_again, "unit must round-trip through borsh");
    }

    /// The symbolic side tables round-trip independently of the object buckets.
    #[test]
    fn symbol_and_export_tables_round_trip() {
        let symbol = Symbol {
            kind: SymbolKind::GenericFn,
            fq_name: "user.foo".to_string(),
            generic: Some(GenericFnKey {
                base_fn: "user.foo".to_string(),
                type_args: vec![baml_type::RuntimeTy::unknown()],
            }),
        };
        let bytes = borsh::to_vec(&symbol).expect("serialize symbol");
        let round_tripped: Symbol = borsh::from_slice(&bytes).expect("deserialize symbol");
        assert_eq!(symbol, round_tripped);

        let exports = ExportTable {
            objects: vec![("user.C".to_string(), LocalRef::Class(3))],
            globals: vec![("user.f".to_string(), 7)],
        };
        let bytes = borsh::to_vec(&exports).expect("serialize exports");
        let round_tripped: ExportTable = borsh::from_slice(&bytes).expect("deserialize exports");
        assert_eq!(exports, round_tripped);
    }
}
