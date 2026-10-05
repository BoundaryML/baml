//! Native implementations for the root `reflect` package: `reflect.signature`,
//! `reflect.call_any`, runtime package compilation, and sessions.
//!
//! Dispatch and the class constructors are generated from
//! `baml_std/reflect/reflect.baml` by `baml_builtins2_codegen`: declaring a
//! `$rust_function` there adds a required [`BamlPackageReflect`] method here,
//! and each class gets a `copy::` struct whose fields are compiler-checked.
//! `reflect.Type.of` is a compiler intrinsic; `reflect.Type.of_value` lives in
//! [`crate::package_reflect::type_class`].

use std::{
    collections::HashMap,
    sync::{Arc, atomic::AtomicBool},
};

use baml_compiler_diagnostics::{
    DiagnosticId, DiagnosticPhase,
    runtime_type::{self, InvalidIdentifierKind},
};
use baml_type::{normalize, normalize::TypeContext};
use bex_heap::TlabHolder;
use bex_vm_types::{
    DeclPath, FnPath, HeapPtr, Interface, Object, RealizedTy, SessionEvalLease, Ty, TyTemplate,
    types::{
        Edge, EdgeKind, ExportSurface, LocalName, Objects, Package, SessionState, Slots,
        TypeAliasDef, TypeValue, Value,
    },
};
use indexmap::IndexMap;

use super::{
    BamlClassPackage, BamlClassSession, BamlPackageReflect, Continuation, ImplResolver,
    NativeCallResult, PackageReflectImpl, copy,
    graft::{Commit, LoadTarget, load_package},
};
use crate::{
    BexVm,
    compile_artifact::{
        ArtifactKind, PinnedArtifact, RuntimeCompileArtifact, RuntimeCompileArtifactSlot,
        RuntimeSessionCompileArtifact, RuntimeSessionStepKind,
    },
    errors::{VmBamlError, VmRustFnError},
    vm::CallableSignature,
};

fn compilation_error(vm: &mut BexVm, id: DiagnosticId, message: String) -> VmRustFnError {
    let diagnostic = super::type_kinds::compiler_diagnostic(id, message);
    VmRustFnError::thrown_fresh(super::type_kinds::alloc_compilation_error(
        vm,
        &[diagnostic],
    ))
}

/// Element tag for the `Arg[]` / `map<string, Arg>` containers. The class
/// instances themselves are built through the generated `copy::` structs.
const ARG_FQN: &str = "reflect.Arg";

/// Materialize the public wrapper for the package selected by the lexical
/// `Package.current()` instruction. Dynamic code uses its owning package;
/// static code uses the package ordinal the linker wrote at the call site.
pub(crate) fn current_package_value(vm: &mut BexVm, ordinal: usize) -> Value {
    let runtime = vm.current_runtime_package();
    let package = if runtime.is_null() {
        vm.packages
            .package_at(ordinal)
            .expect("Package.current call site carries a linked package ordinal")
    } else {
        runtime
    };
    copy::Package {
        _inner: Value::object(package),
    }
    .to_value(vm)
}

impl BamlPackageReflect for PackageReflectImpl {
    fn _render_cause(vm: &mut BexVm, value: &Value) -> NativeCallResult {
        crate::package_baml::root::render_to_string_honoring_overrides(vm, *value)
    }

    fn signature(vm: &mut BexVm, f: &Value) -> Result<Value, VmRustFnError> {
        signature_impl(vm, *f)
    }

    fn call_any(
        vm: &mut BexVm,
        f: &Value,
        args: &IndexMap<bex_str::BexStr, Value>,
    ) -> NativeCallResult {
        call_any_impl(vm, *f, args)
    }
}

/// A runtime package's edge table: its declared dependencies under their
/// aliases, then the language packages under their fixed names as the host
/// image's root reaches them — the runtime mirror of the compiler's edge
/// table for a package without a manifest, so the package can name exactly
/// what its compile could.
fn runtime_edges(
    vm: &BexVm,
    declared: &IndexMap<String, HeapPtr>,
) -> IndexMap<baml_type::Name, Edge> {
    let mut edges: IndexMap<baml_type::Name, Edge> = declared
        .iter()
        .map(|(alias, ptr)| {
            (
                baml_type::Name::new(alias),
                Edge {
                    target: *ptr,
                    kind: EdgeKind::Declared,
                },
            )
        })
        .collect();
    add_prelude(vm, &mut edges);
    edges
}

/// A `with_types` view's edge table: `base` under `$base`, re-exported
/// (every export of the base is the view's); everywhere else the view's
/// additions are defined under a name of its own, `$0`, `$1`, … in
/// first-reach order — a package under a declared edge, an anonymous
/// declaration under an anonymous edge to the declaration itself; a name no
/// source can write, so a consumer spells such a declaration only through
/// the re-export that reaches it; then the prelude by its fixed names. A
/// prelude package is never an edge of its own, since every package reaches
/// it under its fixed name.
fn view_edges(
    vm: &BexVm,
    base: HeapPtr,
    reached: &[crate::reachable::Reached],
) -> IndexMap<baml_type::Name, Edge> {
    use crate::reachable::Reached;
    let mut edges = IndexMap::new();
    edges.insert(
        baml_type::Name::new(REEXPORTED_BASE_EDGE),
        Edge {
            target: base,
            kind: EdgeKind::ReExported,
        },
    );
    let mut next = 0;
    for &reached in reached {
        let (target, kind) = match reached {
            Reached::Package(package) => (package, EdgeKind::Declared),
            Reached::Anonymous(declaration) => (declaration, EdgeKind::Anonymous),
        };
        if target == base
            || (kind == EdgeKind::Declared && vm.packages.is_prelude(target))
            || edges.values().any(|edge| edge.target == target)
        {
            continue;
        }
        edges.insert(
            baml_type::Name::new(format!("${next}")),
            Edge { target, kind },
        );
        next += 1;
    }
    add_prelude(vm, &mut edges);
    edges
}

/// The edge name a view reaches its base under.
const REEXPORTED_BASE_EDGE: &str = "$base";

/// The language packages under their fixed names, as the host image's root
/// reaches them, added to `edges` where the name is free.
fn add_prelude(vm: &BexVm, edges: &mut IndexMap<baml_type::Name, Edge>) {
    for name in baml_builtins2::stdlib_package_names() {
        let name = baml_type::Name::new(name);
        if !edges.contains_key(&name)
            && let Some(target) = vm.packages.accessible(&name)
        {
            edges.insert(
                name,
                Edge {
                    target,
                    kind: EdgeKind::Prelude,
                },
            );
        }
    }
}

fn package_ptr(vm: &BexVm, value: Value) -> Result<HeapPtr, VmRustFnError> {
    let Some(wrapper_ptr) = value.as_object_ptr() else {
        return Err(VmBamlError::InvalidArgument {
            message: "reflect.Package receiver is not an instance".to_string(),
        }
        .into());
    };
    let Object::Instance(wrapper) = vm.get_object(wrapper_ptr) else {
        return Err(VmBamlError::InvalidArgument {
            message: "reflect.Package receiver is not an instance".to_string(),
        }
        .into());
    };
    let Some(ptr) = wrapper.load_field(0).as_object_ptr() else {
        return Err(VmBamlError::InvalidArgument {
            message: "reflect.Package is not initialized".to_string(),
        }
        .into());
    };
    if !matches!(vm.get_object(ptr), Object::Package(_)) {
        return Err(VmBamlError::InvalidArgument {
            message: "reflect.Package has an invalid runtime payload".to_string(),
        }
        .into());
    }
    Ok(ptr)
}

fn take_compile_artifact(
    vm: &BexVm,
    value: Value,
    invalid_message: &str,
    consumed_message: &str,
) -> Result<PinnedArtifact, VmRustFnError> {
    let Some(inner) = value
        .as_object_ptr()
        .and_then(|pointer| match vm.get_object(pointer) {
            Object::Instance(instance) => Some(instance.load_field(0)),
            _ => None,
        })
    else {
        return Err(VmBamlError::InvalidArgument {
            message: invalid_message.to_string(),
        }
        .into());
    };
    let slot = vm
        .as_rust_data::<RuntimeCompileArtifactSlot>(&inner)
        .map_err(|_| VmBamlError::InvalidArgument {
            message: invalid_message.to_string(),
        })?;
    let mut slot = slot.lock().map_err(|_| VmBamlError::InvalidArgument {
        message: format!("{invalid_message}: artifact state is unavailable"),
    })?;
    slot.take().ok_or_else(|| {
        VmRustFnError::from(VmBamlError::InvalidArgument {
            message: consumed_message.to_string(),
        })
    })
}

fn local_name(path: &str) -> Option<LocalName> {
    let path = path.strip_prefix("root.").unwrap_or(path);
    let mut parts = path
        .split('.')
        .map(baml_type::Name::new)
        .collect::<Vec<_>>();
    let name = parts.pop()?;
    Some(LocalName {
        namespace: parts,
        name,
    })
}

fn display_local_name(name: &LocalName) -> String {
    std::iter::once("root")
        .chain(name.namespace.iter().map(baml_type::Name::as_str))
        .chain(std::iter::once(name.name.as_str()))
        .collect::<Vec<_>>()
        .join(".")
}

/// The packages whose exports are `package`'s: itself, then — for a
/// `with_types` view — its re-exported base, transitively, in first-visit
/// order.
fn export_chain(vm: &BexVm, package_ptr: HeapPtr) -> Vec<HeapPtr> {
    let mut chain = vec![package_ptr];
    let mut index = 0;
    while index < chain.len() {
        let current = chain[index];
        index += 1;
        if let Object::Package(package) = vm.get_object(current) {
            for target in package.reexported() {
                if !chain.contains(&target) {
                    chain.push(target);
                }
            }
        }
    }
    chain
}

/// The first answer `f` gives along `package`'s export chain: what a
/// package exports under a name is its own entry, else the entry of the
/// base it re-exports.
fn find_export<T>(
    vm: &BexVm,
    package_ptr: HeapPtr,
    mut f: impl FnMut(&BexVm, HeapPtr, &Package) -> Option<T>,
) -> Option<T> {
    export_chain(vm, package_ptr)
        .into_iter()
        .find_map(|ptr| match vm.get_object(ptr) {
            Object::Package(package) => f(vm, ptr, package),
            _ => None,
        })
}

/// The name `package` keeps the top-level binding under that its source
/// writes as `written`.
///
/// A package keeps a binding under the name it was written with. A Session
/// does not: each submission's bindings are stored under hygienic names, and
/// the Session maps the name a submission writes to the newest of them. A
/// lookup by name answers for the written name only, so a Session's binding
/// is found under the name an identifier in a submission reaches it by, and
/// its hygienic name finds nothing. A Session's bindings sit at its root.
fn stored_binding_name(package: &Package, written: &LocalName) -> Option<LocalName> {
    let Some(session) = package.session.as_deref() else {
        return Some(written.clone());
    };
    if !written.namespace.is_empty() {
        return None;
    }
    let symbol = session.visible.get(written.name.as_str())?;
    Some(LocalName {
        namespace: Vec::new(),
        name: baml_type::Name::new(&symbol.internal),
    })
}

/// What a package exports under `name`, if anything: a declaration by its
/// pointer, or a cell (a function or a `let`).
fn exported_under(package: &Package, name: &LocalName) -> Option<Export> {
    package
        .classes
        .get(name)
        .or_else(|| package.enums.get(name))
        .or_else(|| package.interfaces.get(name))
        .or_else(|| package.type_aliases.get(name))
        .map(|&ptr| Export::Declaration(ptr))
        .or_else(|| {
            (package
                .globals
                .contains_key(&DeclPath::Function(FnPath::Free(name.clone())))
                || package.globals.contains_key(&DeclPath::Let(name.clone())))
            .then_some(Export::Cell)
        })
}

/// One export of a package, by what it is.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Export {
    Declaration(HeapPtr),
    Cell,
}

/// The declarations of one kind a package exports, along its export chain:
/// each name under the first package that exports it.
fn exported_declarations(
    vm: &BexVm,
    package_ptr: HeapPtr,
    table: impl Fn(&Package) -> &IndexMap<LocalName, HeapPtr>,
) -> IndexMap<LocalName, HeapPtr> {
    let mut entries: IndexMap<LocalName, HeapPtr> = IndexMap::new();
    for ptr in export_chain(vm, package_ptr) {
        if let Object::Package(package) = vm.get_object(ptr) {
            for (name, &declaration) in table(package) {
                entries.entry(name.clone()).or_insert(declaration);
            }
        }
    }
    entries
}

/// The declaration a `with_types` value re-exports, if it is a bare nominal:
/// a class, enum, or interface at no arguments, or an alias declaration.
/// Any other type is declared as an alias of the view.
fn reexported_declaration(ty: &RealizedTy) -> Option<HeapPtr> {
    match ty {
        RealizedTy::Class(head, args) if args.is_empty() => Some(head.ptr()),
        RealizedTy::Interface(head, args, assoc) if args.is_empty() && assoc.is_empty() => {
            Some(head.ptr())
        }
        RealizedTy::Enum(head) | RealizedTy::TypeAlias(head) => Some(head.ptr()),
        _ => None,
    }
}

/// What a `with_types` view adds to its base's exports.
#[derive(Default)]
struct Additions {
    classes: IndexMap<LocalName, HeapPtr>,
    enums: IndexMap<LocalName, HeapPtr>,
    interfaces: IndexMap<LocalName, HeapPtr>,
    type_aliases: IndexMap<LocalName, HeapPtr>,
    /// The aliases the view declares, allocated once the view exists to own
    /// them.
    aliases: Vec<(LocalName, RealizedTy)>,
}

enum Addition {
    /// A declaration re-exported under the name.
    Declaration(HeapPtr),
    /// An alias of the type, declared by the view.
    Alias(RealizedTy),
}

/// Where a name stands when a `with_types` call states an export under it.
enum Standing {
    /// Nothing exports the name.
    New,
    /// The name already exports exactly this, where stating it again is
    /// allowed.
    Restated,
    /// The name is taken.
    Taken,
}

/// How a `with_types` call comes to export a name.
#[derive(Clone, Copy)]
enum Stated {
    /// By a key of the call's map.
    ByKey,
    /// By a type the call mounts reaching the declaration, under the
    /// declaration's own name.
    ByReach,
}

impl Additions {
    fn exported(&self, name: &LocalName) -> Option<Export> {
        self.classes
            .get(name)
            .or_else(|| self.enums.get(name))
            .or_else(|| self.interfaces.get(name))
            .or_else(|| self.type_aliases.get(name))
            .map(|&ptr| Export::Declaration(ptr))
            .or_else(|| {
                self.aliases
                    .iter()
                    .any(|(alias, _)| alias == name)
                    .then_some(Export::Cell)
            })
    }

    /// Add `addition` under `name`, stated as `stated`.
    ///
    /// Among one call's additions, an export stated twice is stated once: a
    /// key naming the declaration a sibling entry reached, or one declaration
    /// reached through two types, is one export whichever the call met
    /// first. A name the additions give to anything else is a collision.
    ///
    /// Against the base's chain (`chain`), a key is refused for every name
    /// the base already exports, and a reached declaration only for a name
    /// the base exports as something else — the base's own class named by a
    /// field adds nothing. No addition is ever made under a name the chain
    /// exports, so the two tables never disagree.
    fn add(
        &mut self,
        vm: &mut BexVm,
        chain: &[HeapPtr],
        name: LocalName,
        addition: Addition,
        stated: Stated,
    ) -> Result<(), VmRustFnError> {
        let same_declaration = |export: Export| match (&addition, export) {
            (Addition::Declaration(declaration), Export::Declaration(exported)) => {
                *declaration == exported
            }
            (Addition::Declaration(_), Export::Cell) | (Addition::Alias(_), _) => false,
        };
        let based = || {
            chain.iter().find_map(|&ptr| match vm.get_object(ptr) {
                Object::Package(package) => exported_under(package, &name),
                _ => None,
            })
        };
        let standing = match self.exported(&name) {
            Some(added) if same_declaration(added) => Standing::Restated,
            Some(_) => Standing::Taken,
            None => match (based(), stated) {
                (None, Stated::ByKey | Stated::ByReach) => Standing::New,
                (Some(based), Stated::ByReach) if same_declaration(based) => Standing::Restated,
                (Some(_), Stated::ByKey | Stated::ByReach) => Standing::Taken,
            },
        };
        match standing {
            Standing::New => {}
            Standing::Restated => return Ok(()),
            Standing::Taken => {
                return Err(compilation_error(
                    vm,
                    DiagnosticId::DuplicateName,
                    format!("duplicate exported type name `{name}`"),
                ));
            }
        }
        match addition {
            Addition::Declaration(declaration) => {
                let table = match vm.get_object(declaration) {
                    Object::Class(_) => &mut self.classes,
                    Object::Enum(_) => &mut self.enums,
                    Object::Interface(_) => &mut self.interfaces,
                    Object::TypeAlias(_) => &mut self.type_aliases,
                    _ => unreachable!("a resolved head points at a declaration"),
                };
                table.insert(name, declaration);
            }
            Addition::Alias(ty) => self.aliases.push((name, ty)),
        }
        Ok(())
    }
}

/// Runtime type facts rooted at the Package being inspected, not at the code
/// that happened to call the reflection API. This distinction is observable
/// when a generated package's return class implements an interface imported
/// from one of its live dependencies.
struct PackageSubtypeContext<'a> {
    vm: &'a BexVm,
    package: HeapPtr,
}

impl TypeContext<bex_vm_types::TypeHead> for PackageSubtypeContext<'_> {
    /// Resolution is the VM's: a head is a pointer into the one heap this
    /// package lives on, so scoping the *facts* to a package does not change
    /// how a name becomes a head.
    fn well_known(
        &self,
        head: baml_type::normalize::WellKnownHead,
    ) -> Option<bex_vm_types::TypeHead> {
        TypeContext::well_known(self.vm, head)
    }

    fn alias_def(&self, head: &bex_vm_types::TypeHead) -> Option<Ty> {
        TypeContext::alias_def(self.vm, head)
    }

    fn implements_interface(&self, concrete: &Ty, interface: &Interface) -> bool {
        let Ok(concrete) = RealizedTy::try_from(concrete) else {
            return false;
        };
        let Ok(args) = interface
            .generics
            .iter()
            .map(RealizedTy::try_from)
            .collect::<Result<Vec<_>, _>>()
        else {
            return false;
        };
        let Ok(assoc) = interface
            .associated_types
            .iter()
            .map(|(name, ty)| RealizedTy::try_from(ty).map(|ty| (name.clone(), ty)))
            .collect::<Result<Vec<_>, _>>()
        else {
            return false;
        };
        ImplResolver::for_package(self.vm, self.package).type_implements(
            &concrete,
            interface.name,
            &args,
            &assoc,
        )
    }

    fn type_var_bound(&self, param: &baml_type::ParamTy) -> Vec<Interface> {
        TypeContext::type_var_bound(self.vm, param)
    }

    fn interface_requires(&self, sub: &Interface, sup: &Interface) -> bool {
        TypeContext::interface_requires(self.vm, sub, sup)
    }

    fn enum_variants(&self, head: &bex_vm_types::TypeHead) -> Option<Vec<baml_type::Name>> {
        TypeContext::enum_variants(self.vm, head)
    }

    fn associated_type_bound(
        &self,
        interface: &Interface,
        assoc: baml_type::Name,
    ) -> Vec<Interface> {
        TypeContext::associated_type_bound(self.vm, interface, assoc)
    }

    fn project(
        &self,
        base: &Ty,
        interface: &Interface,
        member: &baml_type::Name,
        fuel: u32,
    ) -> baml_type::normalize::ProjectionStep<bex_vm_types::TypeHead> {
        TypeContext::project(self.vm, base, interface, member, fuel)
    }
}

fn package_class_type(vm: &mut BexVm, class_ptr: HeapPtr) -> Value {
    let Object::Class(class) = vm.get_object(class_ptr) else {
        unreachable!("Package.classes only contains class pointers")
    };
    let ty = RealizedTy::Class(
        bex_vm_types::TypeHead::new(class_ptr, class.type_tag),
        Box::new([]),
    );
    let ty_value = Value::object(vm.tlab.alloc_type(TypeValue::new(ty)));
    super::type_kinds::alloc_kind_view(vm, baml_type::type_kind::TypeKind::Class, ty_value)
}

fn package_enum_type(vm: &mut BexVm, enum_ptr: HeapPtr) -> Value {
    let Object::Enum(enm) = vm.get_object(enum_ptr) else {
        unreachable!("Package.enums only contains enum pointers")
    };
    let ty = RealizedTy::Enum(bex_vm_types::TypeHead::new(enum_ptr, enm.type_tag));
    let ty_value = Value::object(vm.tlab.alloc_type(TypeValue::new(ty)));
    super::type_kinds::alloc_kind_view(vm, baml_type::type_kind::TypeKind::Enum, ty_value)
}

fn package_interface_type(vm: &mut BexVm, interface_ptr: HeapPtr) -> Value {
    let Object::Interface(interface) = vm.get_object(interface_ptr) else {
        unreachable!("Package.interfaces only contains interface pointers")
    };
    let ty = RealizedTy::Interface(
        bex_vm_types::TypeHead::new(interface_ptr, interface.type_tag),
        Box::new([]),
        Box::new([]),
    );
    let ty_value = Value::object(vm.tlab.alloc_type(TypeValue::new(ty)));
    super::type_kinds::alloc_kind_view(vm, baml_type::type_kind::TypeKind::Interface, ty_value)
}

/// `testing.TestCollector.new`, through the prelude package's tables: the
/// stdlib package by its fixed name, the class by its item path, the
/// constructor through the class's method table.
fn test_collector_constructor(vm: &BexVm) -> Option<HeapPtr> {
    let package_ptr = vm.packages.accessible(&baml_type::Name::new("testing"))?;
    let Object::Package(package) = vm.get_object(package_ptr) else {
        return None;
    };
    let class_ptr = *package.classes.get(&LocalName {
        namespace: Vec::new(),
        name: baml_type::Name::new("TestCollector"),
    })?;
    let Object::Class(class) = vm.get_object(class_ptr) else {
        return None;
    };
    let constructor = class
        .methods
        .get(&baml_type::Name::new("new"))?
        .function_ptr;
    (!constructor.is_null()).then_some(constructor)
}

fn package_function_value(vm: &mut BexVm, package_ptr: HeapPtr, name: &LocalName) -> Option<Value> {
    // The slot map is the one road for both kinds of package; `slots` says
    // which table its ordinal addresses.
    let (slot, runtime_package) = match vm.get_object(package_ptr) {
        Object::Package(package) => {
            let slot = *package
                .globals
                .get(&DeclPath::Function(FnPath::Free(name.clone())))?;
            // The index addresses whichever table the package's cells live
            // in: the program pool (no owner) or the package's own (owner).
            let (index, owner) = match &package.slots {
                Slots::Program { base } => (base.raw() + slot as usize, HeapPtr::null()),
                Slots::Own { .. } => (slot as usize, package_ptr),
            };
            (bex_vm_types::GlobalIndex::from_raw(index), owner)
        }
        _ => return None,
    };
    Some(Value::object(vm.alloc(Object::GenericFunction(
        bex_vm_types::GenericFunction {
            function: slot,
            type_args: Box::new([]),
            runtime_package,
        },
    ))))
}

fn function_type(vm: &mut BexVm, package: HeapPtr, name: &LocalName) -> Option<Value> {
    let callable = package_function_value(vm, package, name)?;
    let signature = vm.callable_signature(callable)?;
    let ty = callee_fn_ty(&signature);
    let ty_value = Value::object(vm.alloc_type(bex_vm_types::types::TypeValue::new(ty)));
    Some(super::type_kinds::alloc_kind_view(
        vm,
        baml_type::type_kind::TypeKind::Function,
        ty_value,
    ))
}

/// The `F` contract check `Package.get_function` runs: the callable's
/// reconstructed function type must be a subtype of the contract the caller
/// asked for.
///
/// A runtime package roots the subtype context at itself so its own impls are
/// visible; a statically compiled callable has no such root and uses the VM's
/// lexical world.
fn check_function_contract(
    vm: &mut BexVm,
    package: HeapPtr,
    name: &str,
    signature: &CallableSignature,
) -> Result<(), VmRustFnError> {
    let actual = callee_fn_ty(signature);
    // The caller's `F`. Erasing a missing one to `unknown` would make the
    // subtype check below vacuously true — every function would satisfy every
    // requested signature — so an absent type argument is reported as the
    // frame-seeding bug it is.
    let Some(expected) = vm.current_call_type_args().first().cloned() else {
        return Err(VmRustFnError::InternalError(
            bex_vm_types::errors::VmInternalError::MissingNativeFunction {
                name: "reflect.Package.get_function: missing type argument".to_string(),
            },
        ));
    };
    let sub = Ty::from(actual.clone());
    let sup = Ty::from(expected.clone());
    let matches = if package.is_null() {
        normalize::is_subtype(&sub, &sup, vm)
    } else {
        normalize::is_subtype(&sub, &sup, &PackageSubtypeContext { vm, package })
    };
    if matches {
        return Ok(());
    }
    Err(compilation_error(
        vm,
        DiagnosticId::TypeMismatch,
        format!(
            "function `{name}` has type `{actual}`, which is not a subtype of requested contract `{expected}`"
        ),
    ))
}

fn reflected_class_ty(vm: &BexVm, fqn: &str) -> RealizedTy {
    let qtn = baml_type::QualifiedTypeName::from_dotted_path(fqn);
    let head = vm
        .declaration_head(&qtn)
        .unwrap_or_else(|| unreachable!("`{fqn}` is declared by the stdlib"));
    RealizedTy::Class(head, Box::new([]))
}

fn diagnostic_span_value(vm: &mut BexVm, span: &bex_vm_types::RuntimeSourceSpan) -> Value {
    let file = Value::object(vm.alloc_string(span.file.as_str()));
    copy::Span {
        file,
        start: i64::try_from(span.start).expect("source offsets fit BAML int"),
        end: i64::try_from(span.end).expect("source offsets fit BAML int"),
    }
    .to_value(vm)
}

fn diagnostic_highlights_value(
    vm: &mut BexVm,
    highlights: &[bex_vm_types::RuntimeDiagnosticHighlight],
) -> Value {
    let values = highlights
        .iter()
        .map(|highlight| {
            let kind = match highlight.kind {
                bex_vm_types::RuntimeDiagnosticHighlightKind::IdentifierType => "identifier.type",
                bex_vm_types::RuntimeDiagnosticHighlightKind::IdentifierFunction => {
                    "identifier.function"
                }
                bex_vm_types::RuntimeDiagnosticHighlightKind::IdentifierField => "identifier.field",
                bex_vm_types::RuntimeDiagnosticHighlightKind::IdentifierVariable => {
                    "identifier.variable"
                }
                bex_vm_types::RuntimeDiagnosticHighlightKind::IdentifierEnumVariant => {
                    "identifier.enum_variant"
                }
                bex_vm_types::RuntimeDiagnosticHighlightKind::IdentifierAttribute => {
                    "identifier.attribute"
                }
                bex_vm_types::RuntimeDiagnosticHighlightKind::TypeExpression => "type_expression",
                bex_vm_types::RuntimeDiagnosticHighlightKind::Code => "code",
            };
            let kind = Value::object(vm.alloc_string(kind));
            copy::DiagnosticHighlight {
                start: i64::from(highlight.start),
                end: i64::from(highlight.end),
                kind,
            }
            .to_value(vm)
        })
        .collect();
    Value::object(vm.tlab.alloc_array(
        reflected_class_ty(vm, "reflect.DiagnosticHighlight"),
        values,
    ))
}

pub(super) fn diagnostic_value(
    vm: &mut BexVm,
    diagnostic: &bex_vm_types::RuntimeCompileDiagnostic,
) -> Value {
    let span = diagnostic
        .span
        .as_ref()
        .map_or(Value::NULL, |span| diagnostic_span_value(vm, span));
    let code = Value::object(vm.alloc_string(diagnostic.code.as_str()));
    let message = Value::object(vm.alloc_string(diagnostic.message.as_str()));
    let severity = match diagnostic.severity {
        bex_vm_types::RuntimeDiagnosticSeverity::Error => "error",
        bex_vm_types::RuntimeDiagnosticSeverity::Warning => "warning",
        bex_vm_types::RuntimeDiagnosticSeverity::Info => "info",
    };
    let severity = Value::object(vm.alloc_string(severity));
    let (phase, headline, primary_label, message_highlights, annotations, related_info) =
        match diagnostic.details.as_ref() {
            None => {
                let annotation_ty = reflected_class_ty(vm, "reflect.DiagnosticAnnotation");
                let related_ty = reflected_class_ty(vm, "reflect.DiagnosticRelatedInfo");
                (
                    Value::NULL,
                    Value::object(vm.alloc_string(diagnostic.message.as_str())),
                    Value::NULL,
                    diagnostic_highlights_value(vm, &[]),
                    Value::object(vm.tlab.alloc_array(annotation_ty, Vec::new())),
                    Value::object(vm.tlab.alloc_array(related_ty, Vec::new())),
                )
            }
            Some(details) => {
                let phase = match details.phase {
                    bex_vm_types::RuntimeDiagnosticPhase::Parse => "parse",
                    bex_vm_types::RuntimeDiagnosticPhase::Hir => "hir",
                    bex_vm_types::RuntimeDiagnosticPhase::Validation => "validation",
                    bex_vm_types::RuntimeDiagnosticPhase::Type => "type",
                };
                let phase = Value::object(vm.alloc_string(phase));
                let headline = Value::object(vm.alloc_string(details.headline.as_str()));
                let primary_label = details.primary_label.as_ref().map_or(Value::NULL, |label| {
                    Value::object(vm.alloc_string(label.as_str()))
                });
                let message_highlights =
                    diagnostic_highlights_value(vm, &details.message_highlights);
                let annotation_values = details
                    .annotations
                    .iter()
                    .map(|annotation| {
                        let span = diagnostic_span_value(vm, &annotation.span);
                        let message = annotation.message.as_ref().map_or(Value::NULL, |message| {
                            Value::object(vm.alloc_string(message.as_str()))
                        });
                        let message_highlights =
                            diagnostic_highlights_value(vm, &annotation.message_highlights);
                        copy::DiagnosticAnnotation {
                            span,
                            message,
                            message_highlights,
                            is_primary: annotation.is_primary,
                        }
                        .to_value(vm)
                    })
                    .collect();
                let annotation_ty = reflected_class_ty(vm, "reflect.DiagnosticAnnotation");
                let annotations =
                    Value::object(vm.tlab.alloc_array(annotation_ty, annotation_values));
                let related_values = details
                    .related_info
                    .iter()
                    .map(|related| {
                        let span = diagnostic_span_value(vm, &related.span);
                        let message = Value::object(vm.alloc_string(related.message.as_str()));
                        let message_highlights =
                            diagnostic_highlights_value(vm, &related.message_highlights);
                        let file_path = related.file_path.as_ref().map_or(Value::NULL, |path| {
                            Value::object(vm.alloc_string(path.as_str()))
                        });
                        copy::DiagnosticRelatedInfo {
                            span,
                            message,
                            message_highlights,
                            file_path,
                        }
                        .to_value(vm)
                    })
                    .collect();
                let related_ty = reflected_class_ty(vm, "reflect.DiagnosticRelatedInfo");
                let related_info = Value::object(vm.tlab.alloc_array(related_ty, related_values));
                (
                    phase,
                    headline,
                    primary_label,
                    message_highlights,
                    annotations,
                    related_info,
                )
            }
        };
    copy::Diagnostic {
        code,
        span,
        message,
        severity,
        phase,
        headline,
        primary_label,
        message_highlights,
        annotations,
        related_info,
    }
    .to_value(vm)
}

struct FinishPackage {
    wrapper: HeapPtr,
    package: HeapPtr,
}

impl Continuation for FinishPackage {
    fn call(self: Box<Self>, vm: &mut BexVm, _value: Value) -> NativeCallResult {
        let Object::Package(package) = vm.get_object_mut(self.package) else {
            unreachable!("finish continuation retained a Package")
        };
        let Slots::Own { initialized, .. } = &mut package.slots else {
            unreachable!("a runtime package owns its cells")
        };
        *initialized = true;
        NativeCallResult::Done(Value::object(self.wrapper))
    }

    fn gc_roots(&self) -> Vec<HeapPtr> {
        vec![self.wrapper, self.package]
    }

    fn apply_forwarding(&mut self, forwarding: &std::collections::HashMap<HeapPtr, HeapPtr>) {
        if let Some(&ptr) = forwarding.get(&self.wrapper) {
            self.wrapper = ptr;
        }
        if let Some(&ptr) = forwarding.get(&self.package) {
            self.package = ptr;
        }
    }
}

fn test_function_ty() -> RealizedTy {
    RealizedTy::Function {
        params: Box::new([]),
        ret: Box::new(RealizedTy::null()),
        throws: Box::new(RealizedTy::unknown()),
    }
}

fn empty_tests(vm: &mut BexVm) -> Value {
    Value::object(
        vm.tlab
            .alloc_map(RealizedTy::string(), test_function_ty(), IndexMap::new()),
    )
}

struct RegisterPackageTests {
    test_init: HeapPtr,
}

impl Continuation for RegisterPackageTests {
    fn call(self: Box<Self>, _vm: &mut BexVm, collector: Value) -> NativeCallResult {
        let Some(collector_ptr) = collector.as_object_ptr() else {
            return VmRustFnError::BamlError(VmBamlError::InvalidArgument {
                message: "testing.TestCollector.new returned a non-instance".to_string(),
            })
            .into();
        };
        NativeCallResult::YieldToCall {
            callee: self.test_init,
            args: vec![collector],
            type_args: Vec::new(),
            continuation: Box::new(FinishPackageTests { collector_ptr }),
        }
    }

    fn gc_roots(&self) -> Vec<HeapPtr> {
        vec![self.test_init]
    }

    fn apply_forwarding(&mut self, forwarding: &std::collections::HashMap<HeapPtr, HeapPtr>) {
        if let Some(&ptr) = forwarding.get(&self.test_init) {
            self.test_init = ptr;
        }
    }
}

struct FinishPackageTests {
    collector_ptr: HeapPtr,
}

impl Continuation for FinishPackageTests {
    fn call(self: Box<Self>, vm: &mut BexVm, _value: Value) -> NativeCallResult {
        let tests_value = match vm.get_object(self.collector_ptr) {
            Object::Instance(collector) => collector.load_field(1),
            _ => {
                return VmRustFnError::BamlError(VmBamlError::InvalidArgument {
                    message: "test collector changed kind during registration".to_string(),
                })
                .into();
            }
        };
        let registrations = match vm.as_array(&tests_value) {
            Ok(tests) => tests.to_vec(),
            Err(error) => return error.into(),
        };
        let mut tests = IndexMap::new();
        for registration in registrations {
            let Some(ptr) = registration.as_object_ptr() else {
                continue;
            };
            let Object::Instance(registration) = vm.get_object(ptr) else {
                continue;
            };
            let name_value = registration.load_field(0);
            let body = registration.load_field(1);
            let Ok(name) = vm.as_string(&name_value) else {
                continue;
            };
            tests.insert(name.clone(), body);
        }
        NativeCallResult::Done(Value::object(vm.tlab.alloc_map(
            RealizedTy::string(),
            test_function_ty(),
            tests,
        )))
    }

    fn gc_roots(&self) -> Vec<HeapPtr> {
        vec![self.collector_ptr]
    }

    fn apply_forwarding(&mut self, forwarding: &std::collections::HashMap<HeapPtr, HeapPtr>) {
        if let Some(&ptr) = forwarding.get(&self.collector_ptr) {
            self.collector_ptr = ptr;
        }
    }
}

impl BamlClassPackage for PackageReflectImpl {
    fn _finish(
        vm: &mut BexVm,
        artifact: &Value,
        packages: &IndexMap<bex_str::BexStr, Value>,
    ) -> NativeCallResult {
        let PinnedArtifact { artifact, pins } = match take_compile_artifact(
            vm,
            *artifact,
            "reflect.Package._finish received an invalid artifact",
            "Package compile artifact has already been consumed",
        ) {
            Ok(artifact) => artifact,
            Err(error) => return error.into(),
        };
        if !matches!(&artifact.kind, ArtifactKind::Package) {
            return VmRustFnError::BamlError(VmBamlError::InvalidArgument {
                message: "reflect.Package._finish received a Session.eval artifact".to_string(),
            })
            .into();
        }
        let dependencies = match pinned_dependencies(vm, &pins, packages) {
            Ok(dependencies) => dependencies,
            Err(error) => return error.into(),
        };
        let package = Package {
            name: baml_type::Name::new(baml_type::RESERVED_USER_PACKAGE),
            edges: runtime_edges(vm, &dependencies),
            classes: IndexMap::new(),
            enums: IndexMap::new(),
            interfaces: IndexMap::new(),
            impl_rules: IndexMap::new(),
            type_aliases: IndexMap::new(),
            globals: IndexMap::new(),
            slots: Slots::Own {
                cells: Box::new([]),
                initialized: false,
            },
            objects: Objects::Own(Box::new([])),
            surface: ExportSurface::Compiled(artifact.interface_blob),
            init: None,
            test_init: None,
            diagnostics: artifact.diagnostics,
            session: None,
        };
        let package_ptr = vm.alloc(Object::Package(Box::new(package)));
        let loaded =
            match load_package(vm, package_ptr, LoadTarget::Package, &artifact.emitted, &[]) {
                Ok(loaded) => loaded,
                Err(error) => return error.into(),
            };

        // Linking is complete. First telemetry observation may now register
        // owned definitions; imported functions keep their IDs and registration.
        let wrapper = copy::Package {
            _inner: Value::object(package_ptr),
        }
        .to_value(vm);
        let wrapper_ptr = wrapper
            .as_object_ptr()
            .expect("Package copy helper allocates an instance");
        if let Some(init) = loaded.init {
            NativeCallResult::YieldToCall {
                callee: init,
                args: Vec::new(),
                type_args: Vec::new(),
                continuation: Box::new(FinishPackage {
                    wrapper: wrapper_ptr,
                    package: package_ptr,
                }),
            }
        } else {
            let Object::Package(package) = vm.get_object_mut(package_ptr) else {
                unreachable!()
            };
            let Slots::Own { initialized, .. } = &mut package.slots else {
                unreachable!("a runtime package owns its cells")
            };
            *initialized = true;
            NativeCallResult::Done(wrapper)
        }
    }

    fn get_class(vm: &mut BexVm, package: &Value, name: &bex_str::BexStr) -> Option<Value> {
        let ptr = package_ptr(vm, *package).ok()?;
        let local = local_name(name.as_str())?;
        let class_ptr = find_export(vm, ptr, |_, _, package| {
            package.classes.get(&local).copied()
        })?;
        Some(package_class_type(vm, class_ptr))
    }

    fn get_enum(vm: &mut BexVm, package: &Value, name: &bex_str::BexStr) -> Option<Value> {
        let ptr = package_ptr(vm, *package).ok()?;
        let local = local_name(name.as_str())?;
        let enum_ptr = find_export(vm, ptr, |_, _, package| package.enums.get(&local).copied())?;
        Some(package_enum_type(vm, enum_ptr))
    }

    fn get_interface(vm: &mut BexVm, package: &Value, name: &bex_str::BexStr) -> Option<Value> {
        let ptr = package_ptr(vm, *package).ok()?;
        let local = local_name(name.as_str())?;
        let interface_ptr = find_export(vm, ptr, |_, _, package| {
            package.interfaces.get(&local).copied()
        })?;
        Some(package_interface_type(vm, interface_ptr))
    }

    fn with_types(
        vm: &mut BexVm,
        package: &Value,
        types: &IndexMap<bex_str::BexStr, Value>,
    ) -> Result<Value, VmRustFnError> {
        let base_ptr = package_ptr(vm, *package)?;
        // A view is a new package that re-exports its base wholesale and
        // adds the given names: a bare nominal is re-exported, possibly
        // renamed, under its export name; any other type is an alias the
        // view declares; and every runtime declaration a mounted type
        // reaches is re-exported under its item name, as the type reaches
        // it. Nothing of the base is copied — a name the view does not add
        // is read from the base at the time it is asked for.
        let chain = export_chain(vm, base_ptr);
        let mut additions = Additions::default();
        let mut reached: Vec<crate::reachable::Reached> = Vec::new();
        for (export, value) in types {
            let export = export.to_string();
            if !super::type_kinds::is_baml_identifier(&export) {
                let diagnostic =
                    runtime_type::invalid_identifier(InvalidIdentifierKind::ExportedType, &export)
                        .with_phase(DiagnosticPhase::Hir);
                return Err(VmRustFnError::thrown_fresh(
                    super::type_kinds::alloc_compilation_error(vm, &[diagnostic]),
                ));
            }
            let local = LocalName {
                namespace: Vec::new(),
                name: baml_type::Name::new(&export),
            };
            let Some(mut type_ptr) = value.as_object_ptr() else {
                return Err(compilation_error(
                    vm,
                    DiagnosticId::TypeMismatch,
                    format!("with_types value for `{export}` must be a type"),
                ));
            };
            // A kind view mounts the `type` value it wraps.
            if let Some(ty_value) = super::type_kinds::as_view_type_value(vm, *value) {
                let Some(inner) = ty_value.as_object_ptr() else {
                    return Err(compilation_error(
                        vm,
                        DiagnosticId::TypeMismatch,
                        format!("with_types value for `{export}` must be a type"),
                    ));
                };
                type_ptr = inner;
            }
            let Object::Type(type_value) = vm.get_object(type_ptr) else {
                return Err(compilation_error(
                    vm,
                    DiagnosticId::TypeMismatch,
                    format!("with_types value for `{export}` must be a type"),
                ));
            };
            let ty = type_value.ty.clone();
            let addition = match reexported_declaration(&ty) {
                Some(declaration) => Addition::Declaration(declaration),
                None => Addition::Alias(ty.clone()),
            };
            additions.add(vm, &chain, local, addition, Stated::ByKey)?;
            for declaration in crate::reachable::runtime_definitions(vm, &ty) {
                let item = match vm.get_object(declaration) {
                    Object::Class(class) => class.name.item_name().clone(),
                    Object::Enum(enm) => enm.name.item_name().clone(),
                    Object::Interface(interface) => interface.name.name().clone(),
                    Object::TypeAlias(alias) => alias.name.name().clone(),
                    _ => continue,
                };
                additions.add(
                    vm,
                    &chain,
                    LocalName {
                        namespace: Vec::new(),
                        name: item,
                    },
                    Addition::Declaration(declaration),
                    Stated::ByReach,
                )?;
            }
            for where_ in crate::reachable::reached(vm, &ty) {
                if !reached.contains(&where_) {
                    reached.push(where_);
                }
            }
        }
        let Object::Package(base) = vm.get_object(base_ptr) else {
            unreachable!("package_ptr validates Object::Package")
        };
        let view = Package {
            name: base.name.clone(),
            edges: view_edges(vm, base_ptr, &reached),
            classes: additions.classes,
            enums: additions.enums,
            interfaces: additions.interfaces,
            type_aliases: additions.type_aliases,
            impl_rules: IndexMap::new(),
            globals: IndexMap::new(),
            // A view runs nothing: its base's functions and `let`s run in
            // the base.
            slots: Slots::Own {
                cells: Box::new([]),
                initialized: true,
            },
            objects: Objects::Own(Box::new([])),
            surface: ExportSurface::Projected,
            init: None,
            test_init: None,
            diagnostics: Vec::new(),
            session: None,
        };
        let view_ptr = vm.alloc(Object::Package(Box::new(view)));
        for (local, definition) in additions.aliases {
            let alias = vm.alloc(Object::TypeAlias(Box::new(TypeAliasDef {
                name: baml_type::TypeName::local(local.name.clone()),
                type_tag: baml_type::typetag::TypeTag::fresh_dynamic(),
                definition,
                owner: view_ptr,
            })));
            vm.tlab.heap().write_barrier(view_ptr, Value::object(alias));
            let Object::Package(view) = vm.get_object_mut(view_ptr) else {
                unreachable!("the view was just allocated")
            };
            view.type_aliases.insert(local, alias);
        }
        Ok(copy::Package {
            _inner: Value::object(view_ptr),
        }
        .to_value(vm))
    }

    fn get_function(
        vm: &mut BexVm,
        package: &Value,
        name: &bex_str::BexStr,
    ) -> Result<Option<Value>, VmRustFnError> {
        let package_ptr = package_ptr(vm, *package)?;
        let Some(local) = local_name(name.as_str()) else {
            return Ok(None);
        };
        // The package the function lives in: a view answers with its base's.
        let path = DeclPath::Function(FnPath::Free(local.clone()));
        let Some((package_ptr, function)) = find_export(vm, package_ptr, |vm, ptr, package| {
            let cell = *package.globals.get(&path)?;
            let function = super::graft::cell_value(vm, package, cell)?.as_object_ptr()?;
            Some((ptr, function))
        }) else {
            return Ok(None);
        };
        if matches!(
            vm.get_object(function),
            Object::Function(function)
                if function.origin == bex_vm_types::FunctionOrigin::Internal
        ) {
            return Ok(None);
        }
        let Some(function_value) = package_function_value(vm, package_ptr, &local) else {
            return Ok(None);
        };
        let Some(signature) = vm.callable_signature(function_value) else {
            if vm
                .unspecialized_generic_callable_name(function_value)
                .is_some()
            {
                let diagnostic =
                    runtime_type::unspecialized_reflected_generic(&display_local_name(&local));
                return Err(VmRustFnError::thrown_fresh(
                    super::type_kinds::alloc_compilation_error(vm, &[diagnostic]),
                ));
            }
            return Ok(None);
        };
        // Signature reconstruction only sees the declared surface. A companion
        // whose surface is free of its parent's `T` gets this far and would be
        // handed out as an ordinary function value — and calling one directly
        // fails inside its body as a VM internal error no `catch` can see. Refuse
        // it here, where the caller still has a diagnostic channel.
        if let Some(name) = vm.generic_callable_body_needs_type_args(function_value) {
            let diagnostic = runtime_type::unspecialized_reflected_generic_call(&name);
            return Err(VmRustFnError::thrown_fresh(
                super::type_kinds::alloc_compilation_error(vm, &[diagnostic]),
            ));
        }
        check_function_contract(vm, package_ptr, name.as_str(), &signature)?;
        Ok(Some(function_value))
    }

    fn get_let(vm: &mut BexVm, package: &Value, name: &bex_str::BexStr) -> Value {
        let Ok(package_ptr) = package_ptr(vm, *package) else {
            return Value::NULL;
        };
        let Some(written) = local_name(name.as_str()) else {
            return Value::NULL;
        };
        find_export(vm, package_ptr, |vm, _, package| {
            let path = DeclPath::Let(stored_binding_name(package, &written)?);
            let cell = *package.globals.get(&path)?;
            super::graft::cell_value(vm, package, cell)
        })
        .unwrap_or(Value::NULL)
    }

    fn classes(vm: &mut BexVm, package: &Value) -> IndexMap<bex_str::BexStr, Value> {
        let Ok(ptr) = package_ptr(vm, *package) else {
            return IndexMap::new();
        };
        exported_declarations(vm, ptr, |package| &package.classes)
            .into_iter()
            .map(|(name, class)| {
                (
                    display_local_name(&name).into(),
                    package_class_type(vm, class),
                )
            })
            .collect()
    }

    fn enums(vm: &mut BexVm, package: &Value) -> IndexMap<bex_str::BexStr, Value> {
        let Ok(ptr) = package_ptr(vm, *package) else {
            return IndexMap::new();
        };
        exported_declarations(vm, ptr, |package| &package.enums)
            .into_iter()
            .map(|(name, enm)| (display_local_name(&name).into(), package_enum_type(vm, enm)))
            .collect()
    }

    fn interfaces(vm: &mut BexVm, package: &Value) -> IndexMap<bex_str::BexStr, Value> {
        let Ok(ptr) = package_ptr(vm, *package) else {
            return IndexMap::new();
        };
        exported_declarations(vm, ptr, |package| &package.interfaces)
            .into_iter()
            .map(|(name, interface)| {
                (
                    display_local_name(&name).into(),
                    package_interface_type(vm, interface),
                )
            })
            .collect()
    }

    fn functions(vm: &mut BexVm, package: &Value) -> IndexMap<bex_str::BexStr, Value> {
        let Ok(ptr) = package_ptr(vm, *package) else {
            return IndexMap::new();
        };
        // Every free function along the export chain, each under the first
        // package that exports its name, with the package it lives in.
        let mut functions: IndexMap<LocalName, HeapPtr> = IndexMap::new();
        for package_ptr in export_chain(vm, ptr) {
            let Object::Package(package) = vm.get_object(package_ptr) else {
                continue;
            };
            for (path, &cell) in &package.globals {
                let DeclPath::Function(FnPath::Free(name)) = path else {
                    continue;
                };
                if functions.contains_key(name) {
                    continue;
                }
                let Some(function) = super::graft::cell_value(vm, package, cell)
                    .and_then(|value| value.as_object_ptr())
                else {
                    continue;
                };
                if matches!(
                    vm.get_object(function),
                    Object::Function(function)
                        if function.origin == bex_vm_types::FunctionOrigin::Internal
                ) {
                    continue;
                }
                functions.insert(name.clone(), package_ptr);
            }
        }
        functions
            .into_iter()
            .filter_map(|(name, owner)| {
                function_type(vm, owner, &name).map(|ty| (display_local_name(&name).into(), ty))
            })
            .collect()
    }

    fn tests(vm: &mut BexVm, package: &Value) -> NativeCallResult {
        let package_ptr = match package_ptr(vm, *package) {
            Ok(package) => package,
            Err(error) => return error.into(),
        };
        // A view's tests are its base's.
        let test_init = find_export(vm, package_ptr, |_, _, package| package.test_init);
        let Some(test_init) = test_init else {
            return NativeCallResult::Done(empty_tests(vm));
        };
        let Some(constructor) = test_collector_constructor(vm) else {
            return NativeCallResult::Done(empty_tests(vm));
        };
        let prefix = Value::object(vm.alloc_string(""));
        NativeCallResult::YieldToCall {
            callee: constructor,
            args: vec![prefix],
            type_args: Vec::new(),
            continuation: Box::new(RegisterPackageTests { test_init }),
        }
    }

    fn diagnostics(vm: &mut BexVm, package: &Value) -> Vec<Value> {
        let Ok(ptr) = package_ptr(vm, *package) else {
            return Vec::new();
        };
        let diagnostics = match vm.get_object(ptr) {
            Object::Package(package) => package.diagnostics.clone(),
            _ => Vec::new(),
        };
        diagnostics
            .iter()
            .map(|diagnostic| diagnostic_value(vm, diagnostic))
            .collect()
    }
}

/// The dependencies a `Package._finish` binds: the packages the artifact was
/// compiled against, by the alias the compile spelled each under. The
/// caller's `packages` must be exactly that map — same aliases, same package
/// objects — or the artifact's references would bind to declarations the
/// compiler never read.
fn pinned_dependencies(
    vm: &BexVm,
    pins: &IndexMap<String, bex_heap::Handle>,
    packages: &IndexMap<bex_str::BexStr, Value>,
) -> Result<IndexMap<String, HeapPtr>, VmRustFnError> {
    use bex_heap::WeakHeapRef as _;
    let mismatch = |message: String| {
        VmRustFnError::from(VmBamlError::InvalidArgument {
            message: format!("reflect.Package._finish: {message}"),
        })
    };
    let mut dependencies = IndexMap::with_capacity(pins.len());
    for (alias, handle) in pins {
        let pinned = vm
            .heap
            .resolve_handle_ptr(handle.slab_key())
            .ok_or_else(|| mismatch(format!("the pin of package `{alias}` no longer resolves")))?;
        let Some(given) = packages.get(alias.as_str()) else {
            return Err(mismatch(format!(
                "the artifact was compiled against package `{alias}`, which `packages` omits"
            )));
        };
        if package_ptr(vm, *given)? != pinned {
            return Err(mismatch(format!(
                "package `{alias}` is not the one the artifact was compiled against"
            )));
        }
        dependencies.insert(alias.clone(), pinned);
    }
    if let Some(extra) = packages
        .keys()
        .find(|alias| !pins.contains_key(alias.as_str()))
    {
        return Err(mismatch(format!(
            "the artifact was not compiled against package `{extra}`"
        )));
    }
    Ok(dependencies)
}

#[derive(Clone, Copy)]
struct SessionAction {
    helper: HeapPtr,
    target: usize,
    step: Option<usize>,
}

struct SessionExecution {
    package: HeapPtr,
    actions: Vec<SessionAction>,
    current: usize,
    metadata: RuntimeSessionCompileArtifact,
    result: Value,
    lease: SessionEvalLease,
}

impl Drop for SessionExecution {
    fn drop(&mut self) {
        // A callback throw unwinds and drops the native continuation without
        // calling it again. Release here as well as on normal completion so
        // the RustData artifact's cloned lease cannot leave the Session busy.
        self.lease.release();
    }
}

impl SessionExecution {
    fn next(self: Box<Self>) -> NativeCallResult {
        let action = self.actions[self.current];
        NativeCallResult::YieldToCall {
            callee: action.helper,
            args: Vec::new(),
            type_args: Vec::new(),
            continuation: self,
        }
    }
}

impl Continuation for SessionExecution {
    fn call(mut self: Box<Self>, vm: &mut BexVm, value: Value) -> NativeCallResult {
        let action = self.actions[self.current];
        {
            let Object::Package(package) = vm.get_object_mut(self.package) else {
                unreachable!("Session continuation retained its package")
            };
            let bex_vm_types::types::Slots::Own { cells, .. } = &package.slots else {
                unreachable!("a Session owns its cells")
            };
            cells[action.target].store(value);
            let Some(state) = package.session.as_deref_mut() else {
                unreachable!("Session continuation retained a Session package")
            };

            if let Some(step_index) = action.step {
                let step = &self.metadata.steps[step_index];
                if let RuntimeSessionStepKind::Binding {
                    name,
                    symbol,
                    replay_source,
                } = &step.kind
                {
                    state
                        .history
                        .entry(self.metadata.submission_name.clone())
                        .or_default()
                        .push_str(replay_source);
                    state.visible.insert(name.clone(), symbol.clone());
                }
                if self.metadata.result_step == Some(step_index) {
                    self.result = value;
                }
            }
        }
        vm.heap.write_barrier(self.package, value);

        self.current += 1;
        if self.current < self.actions.len() {
            return self.next();
        }
        self.lease.release();
        NativeCallResult::Done(self.result)
    }

    fn gc_roots(&self) -> Vec<HeapPtr> {
        std::iter::once(self.package)
            .chain(self.actions.iter().map(|action| action.helper))
            .chain(self.result.as_object_ptr())
            .collect()
    }

    fn apply_forwarding(&mut self, forwarding: &HashMap<HeapPtr, HeapPtr>) {
        if let Some(&pointer) = forwarding.get(&self.package) {
            self.package = pointer;
        }
        for action in &mut self.actions {
            if let Some(&pointer) = forwarding.get(&action.helper) {
                action.helper = pointer;
            }
        }
        if let Some(pointer) = self.result.as_object_ptr()
            && let Some(&forwarded) = forwarding.get(&pointer)
        {
            self.result = Value::object(forwarded);
        }
    }
}

/// Graft one submission onto the session package: load it (earlier
/// submissions bind as `SELF` imports, each recorded initializer to the cell
/// it commits into), then pair each commit with the step it belongs to.
fn graft_session_submission(
    vm: &mut BexVm,
    package_ptr: HeapPtr,
    artifact: &RuntimeCompileArtifact,
    metadata: &RuntimeSessionCompileArtifact,
) -> Result<Vec<SessionAction>, VmRustFnError> {
    let step_by_global: HashMap<&LocalName, usize> = metadata
        .steps
        .iter()
        .enumerate()
        .map(|(index, step)| (&step.global, index))
        .collect();
    // An assignment step commits into the visible binding it names, not the
    // generated `let` receiving its value; the loader binds each commit to
    // a cell the session owns, or refuses the submission.
    let steps: Vec<Option<usize>> = metadata
        .initializers
        .iter()
        .map(|initializer| step_by_global.get(&initializer.target).copied())
        .collect();
    let commits: Vec<Commit<'_>> = metadata
        .initializers
        .iter()
        .zip(&steps)
        .map(|(initializer, step)| Commit {
            helper: initializer.helper,
            target: step
                .and_then(|index| metadata.steps[index].commit_global.as_ref())
                .unwrap_or(&initializer.target),
        })
        .collect();
    let loaded = load_package(
        vm,
        package_ptr,
        LoadTarget::Session,
        &artifact.emitted,
        &commits,
    )?;
    let actions = loaded
        .commits
        .iter()
        .zip(&steps)
        .map(|(&(helper, target), &step)| SessionAction {
            helper,
            target,
            step,
        })
        .collect();
    // The load published the image; what follows holds no heap pointers.
    let Object::Package(package) = vm.get_object_mut(package_ptr) else {
        unreachable!("Session payload is a Package")
    };
    package.surface = ExportSurface::Compiled(artifact.interface_blob.clone());
    let Some(state) = package.session.as_deref_mut() else {
        unreachable!("Session graft target is a Session package")
    };
    if !metadata.declaration_source.trim().is_empty() {
        state.history.insert(
            metadata.submission_name.clone(),
            metadata.declaration_source.clone(),
        );
    }
    state.visible.extend(metadata.declarations.clone());
    package.diagnostics.clone_from(&artifact.diagnostics);
    Ok(actions)
}

impl BamlClassSession for PackageReflectImpl {
    fn _new(
        vm: &mut BexVm,
        packages: &IndexMap<bex_str::BexStr, Value>,
    ) -> Result<Value, VmRustFnError> {
        let mut dependencies = IndexMap::new();
        for (alias, value) in packages {
            // Keep runtime rejection single-sourced with compiler mount filtering.
            if baml_builtins2::reserved_edge_names().contains(&alias.as_str()) {
                let diagnostic = super::type_kinds::compiler_diagnostic(
                    DiagnosticId::InvalidSyntax,
                    format!("package alias `{alias}` is reserved"),
                );
                return Err(VmRustFnError::thrown_fresh(
                    super::type_kinds::alloc_compilation_error(vm, &[diagnostic]),
                ));
            }
            dependencies.insert(alias.to_string(), package_ptr(vm, *value)?);
        }
        let package = Package {
            name: baml_type::Name::new(baml_type::RESERVED_USER_PACKAGE),
            edges: runtime_edges(vm, &dependencies),
            classes: IndexMap::new(),
            enums: IndexMap::new(),
            interfaces: IndexMap::new(),
            impl_rules: IndexMap::new(),
            type_aliases: IndexMap::new(),
            globals: IndexMap::new(),
            slots: Slots::Own {
                cells: Box::new([]),
                // Session cells intentionally stay mutable between evals.
                initialized: false,
            },
            objects: Objects::Own(Box::new([])),
            // Every submission's compile exports the whole session; the first
            // replaces this before any code of the session can observe it.
            surface: ExportSurface::Compiled(Vec::new()),
            init: None,
            test_init: None,
            diagnostics: Vec::new(),
            session: Some(Box::new(SessionState {
                history: IndexMap::new(),
                visible: IndexMap::new(),
                busy: Arc::new(AtomicBool::new(false)),
                submission_counter: 0,
            })),
        };
        let package = vm.alloc(Object::Package(Box::new(package)));
        Ok(copy::Session {
            _inner: Value::object(package),
        }
        .to_value(vm))
    }

    fn _finish(vm: &mut BexVm, session: &Value, artifact: &Value) -> NativeCallResult {
        let package = match package_ptr(vm, *session) {
            Ok(package) => package,
            Err(error) => return error.into(),
        };
        let PinnedArtifact { mut artifact, pins } = match take_compile_artifact(
            vm,
            *artifact,
            "Session._finish received an invalid artifact",
            "Session artifact has already been consumed",
        ) {
            Ok(artifact) => artifact,
            Err(error) => return error.into(),
        };
        debug_assert!(
            pins.is_empty(),
            "a session artifact binds its dependencies through the session package, not pins"
        );
        let kind = std::mem::replace(&mut artifact.kind, ArtifactKind::Package);
        let (metadata, lease) = match kind {
            ArtifactKind::Session { meta, lease } => (meta, lease),
            ArtifactKind::Package => {
                return VmRustFnError::BamlError(VmBamlError::InvalidArgument {
                    message: "Session._finish received a Package.compile artifact".to_string(),
                })
                .into();
            }
        };
        let actions = match graft_session_submission(vm, package, &artifact, &metadata) {
            Ok(actions) => actions,
            Err(error) => {
                lease.release();
                return error.into();
            }
        };
        if actions.is_empty() {
            lease.release();
            return NativeCallResult::Done(Value::NULL);
        }
        Box::new(SessionExecution {
            package,
            actions,
            current: 0,
            metadata,
            result: Value::NULL,
            lease,
        })
        .next()
    }

    fn diagnostics(vm: &mut BexVm, session: &Value) -> Vec<Value> {
        let Ok(package) = package_ptr(vm, *session) else {
            return Vec::new();
        };
        let diagnostics = match vm.get_object(package) {
            Object::Package(package) => package.diagnostics.clone(),
            _ => Vec::new(),
        };
        diagnostics
            .iter()
            .map(|diagnostic| diagnostic_value(vm, diagnostic))
            .collect()
    }
}

fn ty_never() -> RealizedTy {
    RealizedTy::Never
}

/// The two natives' parameters are statically `reflect.AnyFunction`, so a
/// non-callable here means the coercion rule and the runtime disagree — an
/// internal invariant break, not a user error.
/// A reflection entry point was handed a value that is not callable.
///
/// Both callers take an `AnyFunction`-typed parameter, so the type checker has
/// already proved the argument is callable: `reflect.signature` declares
/// `throws never` and `reflect.call_any`'s only argument-shaped throw is
/// `reflect.InvalidArgumentError` (which describes an argument that does not
/// fit a *parameter*, not a non-callable callee). Neither contract can carry
/// this, and neither is user-reachable, so it is an internal inconsistency.
fn non_callable_error(what: &str) -> VmRustFnError {
    VmRustFnError::InternalError(crate::errors::VmInternalError::MissingNativeFunction {
        name: format!("{what} expects a function value"),
    })
}

/// The `reflect.Arg` class type, for array/map element tags.
///
/// A stdlib FQN constant resolving to a head — one of the sanctioned name
/// boundaries; the head comes off the declaration, never from the name's hash.
fn ty_arg(vm: &BexVm) -> RealizedTy {
    let qtn = baml_type::QualifiedTypeName::from_dotted_path(ARG_FQN);
    let head = vm
        .declaration_head(&qtn)
        .unwrap_or_else(|| unreachable!("`{ARG_FQN}` is declared by the stdlib"));
    RealizedTy::Class(head, Box::new([]))
}

/// Build one `reflect.Arg`. A nameless positional (a host callable from a
/// language without parameter-name introspection) gets the `$argN`
/// placeholder for its position: `$` is unwritable in user identifiers, so a
/// placeholder can never collide with a declared parameter or a
/// named-argument key.
fn alloc_arg(
    vm: &mut BexVm,
    name: Option<&baml_type::Name>,
    position: usize,
    ty: RealizedTy,
) -> Value {
    let name = match name {
        Some(n) => Value::object(vm.alloc_string(n.as_str())),
        None => Value::object(vm.alloc_string(format!("$arg{position}"))),
    };
    let ty = Value::object(vm.alloc_type(bex_vm_types::types::TypeValue::new(ty)));
    copy::Arg { name, r#type: ty }.to_value(vm)
}

/// A `string?` field: the string, or null.
fn opt_string(vm: &mut BexVm, value: Option<&String>) -> Value {
    match value {
        Some(v) => Value::object(vm.alloc_string(v.as_str())),
        None => Value::NULL,
    }
}

/// `reflect.signature(f) -> reflect.Signature`.
fn signature_impl(vm: &mut BexVm, f_val: Value) -> Result<Value, VmRustFnError> {
    use baml_type::FunctionParamMode;
    let Some(sig) = vm.callable_signature(f_val) else {
        return Err(non_callable_error("reflect.signature"));
    };
    let mut positional = Vec::new();
    let mut opts: IndexMap<bex_str::BexStr, Value> = IndexMap::new();
    for param in &sig.params {
        match param.mode {
            FunctionParamMode::Required => {
                let position = positional.len();
                let arg = alloc_arg(vm, param.name.as_ref(), position, param.ty.clone());
                positional.push(arg);
            }
            FunctionParamMode::Optional => {
                // An optional parameter always has a source name; a nameless
                // one is unaddressable by callers (there is nothing to pass
                // it by), so it is simply absent from `opts`. Placeholders
                // are for positionals only and never enter by-name matching.
                if let Some(name) = &param.name {
                    let arg = alloc_arg(vm, Some(name), positional.len(), param.ty.clone());
                    opts.insert(bex_str::BexStr::from(name.as_str()), arg);
                }
            }
        }
    }
    let arg_ty = ty_arg(vm);
    let args = Value::object(vm.tlab.alloc_array(arg_ty.clone(), positional));
    let opts = Value::object(vm.tlab.alloc_map(RealizedTy::string(), arg_ty, opts));
    let returns =
        Value::object(vm.alloc_type(bex_vm_types::types::TypeValue::new(sig.ret.clone())));
    let errors = Value::object(vm.alloc_type(bex_vm_types::types::TypeValue::new(sig.throws)));
    let docstring = opt_string(vm, sig.docstring.as_ref());
    let name = opt_string(vm, sig.name.as_ref());
    Ok(copy::Signature {
        name,
        args,
        opts,
        returns,
        errors,
        docstring,
    }
    .to_value(vm))
}

/// Throw `reflect.InvalidArgumentError { argument, expected, got }`.
fn raise_invalid_argument(
    vm: &mut BexVm,
    argument: &str,
    expected: RealizedTy,
    got: RealizedTy,
) -> NativeCallResult {
    let argument = Value::object(vm.alloc_string(argument));
    let expected = Value::object(vm.alloc_type(bex_vm_types::types::TypeValue::new(expected)));
    let got = Value::object(vm.alloc_type(bex_vm_types::types::TypeValue::new(got)));
    let err = copy::InvalidArgumentError {
        argument,
        expected,
        got,
    }
    .to_value(vm);
    NativeCallResult::Error(VmRustFnError::thrown_fresh(err))
}

/// The callee's whole function type, for arity / unknown-name mismatches.
fn callee_fn_ty(sig: &CallableSignature) -> RealizedTy {
    RealizedTy::Function {
        params: sig.params.clone(),
        ret: Box::new(sig.ret.clone()),
        throws: Box::new(sig.throws.clone()),
    }
}

/// A value's reconstructed type, `unknown` when it has none (a bound method, an
/// opaque handle). A future reconstructs to the `Future<T, E>` it was spawned at.
fn value_realized_ty(vm: &BexVm, value: Value) -> RealizedTy {
    vm.value_concrete_ty(value)
        .map_or_else(RealizedTy::unknown, RealizedTy::from)
}

/// Whether `value` fits the parameter type `expected`, by the canonical
/// algebra over the runtime context. The value is compared at its most precise
/// type (`value_singleton_ty`), so the string `"auto"` fits a parameter typed
/// `"auto" | "manual"`: reconstructed as `string` it would fit no literal type.
/// Fails OPEN when the value's type cannot be reconstructed — an opaque native
/// handle (see `value_concrete_ty`) has no BAML type to compare against, and
/// refusing what we cannot check would reject working calls; the callee
/// remains dynamically safe either way (values stay tagged).
fn value_fits(vm: &BexVm, value: Value, expected: &RealizedTy) -> bool {
    let Some(actual) = vm.value_singleton_ty(value) else {
        return true;
    };
    // No convention patching is needed on the way in: a reconstructed
    // signature spells "cannot throw" as `never`, exactly as the static
    // algebra does.
    let actual: Ty = actual.into();
    let expected: Ty = expected.clone().into();
    // The VM itself is the runtime `TypeContext`.
    normalize::is_subtype(&actual, &expected, vm)
}

/// `reflect.call_any` mirrors the ordinary call boundary's one numeric
/// conversion: an exactly representable `int` may enter a `float` (or
/// `float?`) slot. Materialize the boxed float before dispatch so the callee
/// receives the runtime representation its signature promises.
#[expect(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    reason = "the round trip deliberately detects lossy i64-to-f64 conversions"
)]
fn prepare_call_any_argument(vm: &mut BexVm, value: Value, expected: &RealizedTy) -> Option<Value> {
    fn is_float_slot(ty: &RealizedTy) -> bool {
        match ty {
            RealizedTy::Float => true,
            RealizedTy::Union(members) => {
                members.iter().any(is_float_slot)
                    && members
                        .iter()
                        .all(|member| member.is_null() || is_float_slot(member))
            }
            _ => false,
        }
    }

    if let bex_vm_types::ValueKind::Int(number) = value.kind()
        && is_float_slot(expected)
    {
        let widened = number as f64;
        if widened as i64 != number {
            return None;
        }
        return Some(Value::object(vm.tlab.alloc(Object::Float(widened))));
    }
    value_fits(vm, value, expected).then_some(value)
}

/// Checks the result of the dynamically dispatched call against the `R` that
/// typed `reflect.call_any<R, E>`. The callee runs after the native yields, so
/// this continuation is the first point where both the promised type and the
/// returned value are available together.
struct CallAnyContinuation {
    expected: RealizedTy,
}

impl Continuation for CallAnyContinuation {
    fn call(self: Box<Self>, vm: &mut BexVm, value: Value) -> NativeCallResult {
        if matches!(self.expected, RealizedTy::Unknown) {
            return NativeCallResult::Done(value);
        }

        let matches = crate::type_match::value_matches_template(
            vm,
            value,
            &TyTemplate::from(self.expected.clone()),
            &[],
        );
        match matches {
            Ok(true) => NativeCallResult::Done(value),
            Ok(false) => raise_invalid_argument(
                vm,
                "reflect.call_any return value",
                self.expected,
                value_realized_ty(vm, value),
            ),
            Err(error) => error.into(),
        }
    }

    fn gc_roots(&self) -> Vec<HeapPtr> {
        let mut roots = Vec::new();
        self.expected.visit_heads(&mut |head| {
            if head.is_resolved() {
                roots.push(head.ptr());
            }
        });
        roots
    }

    fn apply_forwarding(&mut self, forwarding: &HashMap<HeapPtr, HeapPtr>) {
        self.expected.visit_heads_mut(&mut |head| {
            if head.is_resolved()
                && let Some(&moved) = forwarding.get(&head.ptr())
            {
                head.forward_to(moved);
            }
        });
    }
}

/// `reflect.call_any<R, E>(f, args) -> R throws E | InvalidArgumentError | CompilationError`.
///
/// Every argument is keyed by parameter name; a nameless positional is
/// addressed by the same `$argN` placeholder `reflect.signature` reports, so
/// the signature's keys are exactly the accepted keys. Checks the map
/// against `f`'s runtime signature (a missing required parameter, a key
/// naming no parameter, or an ill-typed value throws `InvalidArgumentError`),
/// then dispatches through the CPS trampoline. Absent optionals are passed
/// as `OMITTED_ARG`, so a bytecode callee's own default prologue fires; the
/// callee's throw unwinds transparently past the native frame to the caller,
/// which is exactly the declared `throws E` channel.
fn call_any_impl(
    vm: &mut BexVm,
    f_val: Value,
    provided: &IndexMap<bex_str::BexStr, Value>,
) -> NativeCallResult {
    use baml_type::FunctionParamMode;
    let Some(f_ptr) = f_val.as_object_ptr() else {
        return non_callable_error("reflect.call_any").into();
    };
    let Some(sig) = vm.callable_signature(f_val) else {
        if let Some(name) = vm.unspecialized_generic_callable_name(f_val) {
            let diagnostic = runtime_type::unspecialized_reflected_generic(&name);
            return VmRustFnError::thrown_fresh(super::type_kinds::alloc_compilation_error(
                vm,
                &[diagnostic],
            ))
            .into();
        }
        return non_callable_error("reflect.call_any").into();
    };
    // A generic whose signature happens to be free of its own type parameters
    // reconstructs above and would otherwise be entered with an empty frame,
    // failing inside its body as a VM internal error.
    if let Some(name) = vm.generic_callable_body_needs_type_args(f_val) {
        let diagnostic = runtime_type::unspecialized_reflected_generic_call(&name);
        return VmRustFnError::thrown_fresh(super::type_kinds::alloc_compilation_error(
            vm,
            &[diagnostic],
        ))
        .into();
    }

    // Walk the parameters in declaration order, resolving each from the map
    // by its addressable name and assembling the callee's frame as we go.
    // Absent optionals become `OMITTED_ARG`: a bytecode callee's default
    // prologue replaces them; a native callee's glue reads them as "not
    // supplied". A nameless optional is unaddressable and always omitted.
    let mut final_args = Vec::with_capacity(sig.params.len());
    let mut addressable: Vec<String> = Vec::with_capacity(sig.params.len());
    let mut matched = 0usize;
    let mut positional_idx = 0usize;
    for param in &sig.params {
        let key = match (&param.name, param.mode) {
            (Some(name), _) => Some(name.as_str().to_string()),
            (None, FunctionParamMode::Required) => Some(format!("$arg{positional_idx}")),
            (None, FunctionParamMode::Optional) => None,
        };
        if param.mode == FunctionParamMode::Required {
            positional_idx += 1;
        }
        let value = key.as_deref().and_then(|k| provided.get(k).copied());
        if let Some(k) = key.clone() {
            addressable.push(k);
        }
        match (param.mode, value) {
            (_, Some(value)) => {
                let Some(value) = prepare_call_any_argument(vm, value, &param.ty) else {
                    let expected = param.ty.clone();
                    let got = value_realized_ty(vm, value);
                    return raise_invalid_argument(vm, key.as_deref().unwrap_or(""), expected, got);
                };
                matched += 1;
                final_args.push(value);
            }
            (FunctionParamMode::Required, None) => {
                // Missing required parameter: its type against `never` (no
                // value was supplied at all).
                return raise_invalid_argument(
                    vm,
                    key.as_deref().unwrap_or(""),
                    param.ty.clone(),
                    ty_never(),
                );
            }
            (FunctionParamMode::Optional, None) => final_args.push(Value::OMITTED_ARG),
        }
    }

    // Every provided key must have matched a parameter (names are unique, so
    // the counts agree exactly when no key was extraneous). Name the first
    // key that addresses no parameter.
    if matched != provided.len() {
        let unknown = provided
            .keys()
            .find(|k| !addressable.iter().any(|a| a == k.as_str()))
            .map(|k| k.as_str().to_string())
            .unwrap_or_default();
        let got = provided
            .get(unknown.as_str())
            .copied()
            .map_or_else(RealizedTy::unknown, |v| value_realized_ty(vm, v));
        let expected = callee_fn_ty(&sig);
        return raise_invalid_argument(vm, &unknown, expected, got);
    }

    // Read `R` only after argument validation has finished allocating: once
    // captured below, the continuation owns and roots every declaration head
    // the realized type refers to while the callee is running.
    let expected_return = vm
        .current_call_type_args()
        .first()
        .cloned()
        .unwrap_or_else(RealizedTy::unknown);
    NativeCallResult::YieldToCall {
        callee: f_ptr,
        args: final_args,
        type_args: vec![],
        continuation: Box::new(CallAnyContinuation {
            expected: expected_return,
        }),
    }
}
