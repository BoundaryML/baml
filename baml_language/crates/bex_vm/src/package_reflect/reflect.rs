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
    collections::{HashMap, HashSet},
    sync::{Arc, atomic::AtomicBool},
};

use baml_compiler_diagnostics::{
    DiagnosticId, DiagnosticPhase,
    runtime_type::{self, InvalidIdentifierKind},
};
use baml_type::{TyAttr, normalize, normalize::TypeContext};
use bex_heap::TlabHolder;
use bex_vm_types::{
    ArtifactKind, AtomicValueSlot, HeapPtr, Interface, Object, RealizedTy, RuntimeCompileArtifact,
    RuntimeCompileArtifactSlot, RuntimeSessionCompileArtifact, RuntimeSessionStepKind,
    SessionEvalLease, Ty, TyTemplate,
    link::link_dynamic,
    relink::{IndexOperand, visit_object_operands},
    types::{
        LocalName, MethodImpl, Package, PackageKind, RuntimeImplRule, RuntimePackage, SessionState,
        TypeValue, Value,
    },
};
use indexmap::IndexMap;

use super::{
    BamlClassPackage, BamlClassSession, BamlPackageReflect, Continuation, ImplResolver,
    NativeCallResult, PackageReflectImpl, copy,
};
use crate::{
    BexVm,
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
/// static code uses the package name baked at the call site.
pub(crate) fn current_package_value(vm: &mut BexVm, static_package: &str) -> Value {
    let runtime = vm.current_runtime_package();
    let package = if runtime.is_null() {
        vm.packages
            .package_ptr(&baml_type::Name::new(static_package))
            .expect("Package.current call site names a loaded package")
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
) -> Result<RuntimeCompileArtifact, VmRustFnError> {
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

fn stored_package_type(package: &Package, name: &LocalName) -> Option<HeapPtr> {
    name.namespace
        .is_empty()
        .then(|| package.mounted_types.get(name.name.as_str()).copied())
        .flatten()
        .or_else(|| {
            // Two hops, each keyed by what it is actually indexed on: the
            // package's export namespace resolves the source-visible name to a
            // declaration, and the created-once table is keyed by that
            // declaration.
            let declaration = package
                .classes
                .get(name)
                .or_else(|| package.enums.get(name))
                .or_else(|| package.interfaces.get(name))?;
            package.runtime()?.type_values.get(declaration).copied()
        })
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
    fn head_lookup(&self, qtn: &baml_type::QualifiedTypeName) -> Option<bex_vm_types::TypeHead> {
        TypeContext::head_lookup(self.vm, qtn)
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

fn package_class_type(vm: &mut BexVm, runtime_type: Option<HeapPtr>, class_ptr: HeapPtr) -> Value {
    if let Some(runtime_type) = runtime_type {
        let ty_value = Value::object(runtime_type);
        return super::type_kinds::alloc_kind_view(
            vm,
            baml_type::type_kind::TypeKind::Class,
            ty_value,
        );
    }
    let Object::Class(class) = vm.get_object(class_ptr) else {
        unreachable!("Package.classes only contains class pointers")
    };
    let ty = RealizedTy::Class(
        bex_vm_types::TypeHead::new(class_ptr, class.type_tag),
        Vec::new(),
        class.ty_attr.clone(),
    );
    let ty_value = Value::object(vm.tlab.alloc_type(TypeValue::new(ty)));
    super::type_kinds::alloc_kind_view(vm, baml_type::type_kind::TypeKind::Class, ty_value)
}

fn package_enum_type(vm: &mut BexVm, runtime_type: Option<HeapPtr>, enum_ptr: HeapPtr) -> Value {
    if let Some(runtime_type) = runtime_type {
        let ty_value = Value::object(runtime_type);
        return super::type_kinds::alloc_kind_view(
            vm,
            baml_type::type_kind::TypeKind::Enum,
            ty_value,
        );
    }
    let Object::Enum(enm) = vm.get_object(enum_ptr) else {
        unreachable!("Package.enums only contains enum pointers")
    };
    let ty = RealizedTy::Enum(
        bex_vm_types::TypeHead::new(enum_ptr, enm.type_tag),
        enm.ty_attr.clone(),
    );
    let ty_value = Value::object(vm.tlab.alloc_type(TypeValue::new(ty)));
    super::type_kinds::alloc_kind_view(vm, baml_type::type_kind::TypeKind::Enum, ty_value)
}

fn package_interface_type(
    vm: &mut BexVm,
    runtime_type: Option<HeapPtr>,
    interface_ptr: HeapPtr,
) -> Value {
    if let Some(runtime_type) = runtime_type {
        let ty_value = Value::object(runtime_type);
        return super::type_kinds::alloc_kind_view(
            vm,
            baml_type::type_kind::TypeKind::Interface,
            ty_value,
        );
    }
    let Object::Interface(interface) = vm.get_object(interface_ptr) else {
        unreachable!("Package.interfaces only contains interface pointers")
    };
    let ty = RealizedTy::Interface(
        bex_vm_types::TypeHead::new(interface_ptr, interface.type_tag),
        Vec::new(),
        Vec::new(),
        TyAttr::default(),
    );
    let ty_value = Value::object(vm.tlab.alloc_type(TypeValue::new(ty)));
    super::type_kinds::alloc_kind_view(vm, baml_type::type_kind::TypeKind::Interface, ty_value)
}

fn allocate_runtime_declaration_types(
    vm: &mut BexVm,
    package_ptr: HeapPtr,
    classes: &IndexMap<LocalName, HeapPtr>,
    enums: &IndexMap<LocalName, HeapPtr>,
    interfaces: &IndexMap<LocalName, HeapPtr>,
) -> IndexMap<HeapPtr, HeapPtr> {
    let class_rows = classes
        .values()
        .filter_map(|&class_ptr| match vm.get_object(class_ptr) {
            Object::Class(class) => Some((
                class_ptr,
                RealizedTy::Class(
                    bex_vm_types::TypeHead::new(class_ptr, class.type_tag),
                    Vec::new(),
                    class.ty_attr.clone(),
                ),
            )),
            _ => None,
        })
        .collect::<Vec<_>>();
    let enum_rows = enums
        .values()
        .filter_map(|&enum_ptr| match vm.get_object(enum_ptr) {
            Object::Enum(enm) => Some((
                enum_ptr,
                RealizedTy::Enum(
                    bex_vm_types::TypeHead::new(enum_ptr, enm.type_tag),
                    enm.ty_attr.clone(),
                ),
            )),
            _ => None,
        })
        .collect::<Vec<_>>();
    let interface_rows = interfaces
        .values()
        .filter_map(|&interface_ptr| match vm.get_object(interface_ptr) {
            Object::Interface(interface) => Some((
                interface_ptr,
                RealizedTy::Interface(
                    bex_vm_types::TypeHead::new(interface_ptr, interface.type_tag),
                    Vec::new(),
                    Vec::new(),
                    TyAttr::default(),
                ),
            )),
            _ => None,
        })
        .collect::<Vec<_>>();

    let mut type_values = IndexMap::new();
    // Each type value's head points at the declaration it names, so the value
    // reaches its definition without a side table; the declaration's own `owner`
    // is what keeps the package (and so its globals and dependencies) alive.
    for (class_ptr, ty) in class_rows {
        let type_ptr = vm.alloc_type(TypeValue::new(ty));
        let Object::Class(class) = vm.get_object_mut(class_ptr) else {
            unreachable!("runtime package class pointer changed kind")
        };
        class.owner = package_ptr;
        type_values.insert(class_ptr, type_ptr);
    }
    for (enum_ptr, ty) in enum_rows {
        let type_ptr = vm.alloc_type(TypeValue::new(ty));
        let Object::Enum(enm) = vm.get_object_mut(enum_ptr) else {
            unreachable!("runtime package enum pointer changed kind")
        };
        enm.owner = package_ptr;
        type_values.insert(enum_ptr, type_ptr);
    }
    for (interface_ptr, ty) in interface_rows {
        let type_ptr = vm.alloc_type(TypeValue::new(ty));
        type_values.insert(interface_ptr, type_ptr);
    }
    type_values
}

fn package_function_value(vm: &mut BexVm, package_ptr: HeapPtr, name: &LocalName) -> Option<Value> {
    let local = name
        .namespace
        .iter()
        .map(baml_type::Name::as_str)
        .chain(std::iter::once(name.name.as_str()))
        .collect::<Vec<_>>()
        .join(".");
    let (slot, runtime_package) = match vm.get_object(package_ptr) {
        Object::Package(package) => match package.runtime() {
            Some(runtime) => {
                let slot = runtime
                    .global_names
                    .get(&format!("user.{local}"))
                    .or_else(|| runtime.global_names.get(&local));
                let slot = slot?;
                (bex_vm_types::GlobalIndex::from_raw(*slot), package_ptr)
            }
            None => {
                let package = vm.packages.package_name(package_ptr)?;
                let slot = vm.packages.global_by_name(&format!("{package}.{local}"))?;
                (slot, HeapPtr::null())
            }
        },
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

fn dependency_object(vm: &BexVm, package_ptr: HeapPtr, local: &str) -> Option<HeapPtr> {
    let Object::Package(package) = vm.get_object(package_ptr) else {
        return None;
    };
    let local_name = local_name(local)?;
    package
        .classes
        .get(&local_name)
        .or_else(|| package.enums.get(&local_name))
        .or_else(|| package.interfaces.get(&local_name))
        .or_else(|| package.functions.get(&local_name))
        .copied()
        .or_else(|| mounted_declaration(vm, package, local))
        .or_else(|| dependency_named_object(vm, package_ptr, package, local))
}

/// Resolve `local` against a package's canonical object names: the lane for a
/// declaration the package's item tables do not row under its own name. A
/// class-inherent method is the case today — a consumer's static call imports
/// it as `<alias>.<Class>.<method>`, the class-qualified spelling the emitter
/// registered the function under — so it resolves exactly as the global lane
/// resolves an alias-qualified global: `user.<local>` for a runtime image,
/// `<canonical package>.<local>` for a static one.
fn dependency_named_object(
    vm: &BexVm,
    package_ptr: HeapPtr,
    package: &bex_vm_types::types::Package,
    local: &str,
) -> Option<HeapPtr> {
    if let Some(runtime) = package.runtime() {
        runtime
            .object_names
            .get(&format!("user.{local}"))
            .or_else(|| runtime.object_names.get(local))
            .copied()
    } else {
        let canonical = vm.packages.package_name(package_ptr)?;
        vm.packages.object_by_name(&format!("{canonical}.{local}"))
    }
}

/// Resolve `local` against a package's mount surface: the export name of a
/// mounted type resolves to the declaration it is headed at, and the item name
/// of any runtime declaration a mounted type reaches resolves to that
/// declaration. This is the linker's name boundary for declarations that have
/// no spelling of their own — the mount surface is the only place a consumer
/// compile can name them, and its blob rows are spelled `alias.<item name>`.
fn mounted_declaration(
    vm: &BexVm,
    package: &bex_vm_types::types::Package,
    local: &str,
) -> Option<HeapPtr> {
    let head_declaration = |value: &bex_vm_types::types::TypeValue| match &value.ty {
        RealizedTy::Class(head, ..) | RealizedTy::Enum(head, ..) => {
            head.is_resolved().then(|| head.ptr())
        }
        _ => None,
    };
    if let Some(&type_ptr) = package.mounted_types.get(local)
        && let Object::Type(value) = vm.get_object(type_ptr)
        && let Some(ptr) = head_declaration(value)
    {
        return Some(ptr);
    }
    for &type_ptr in package.mounted_types.values() {
        let Object::Type(value) = vm.get_object(type_ptr) else {
            continue;
        };
        for ptr in crate::reachable::runtime_definitions(vm, &value.ty) {
            let item = match vm.get_object(ptr) {
                Object::Class(class) => class.name.item_name(),
                Object::Enum(enm) => enm.name.item_name(),
                _ => continue,
            };
            if item.as_str() == local {
                return Some(ptr);
            }
        }
    }
    None
}

fn diagnostic_value(vm: &mut BexVm, diagnostic: &bex_vm_types::RuntimeCompileDiagnostic) -> Value {
    let span = diagnostic.span.as_ref().map_or(Value::NULL, |span| {
        let file = Value::object(vm.alloc_string(span.file.as_str()));
        copy::Span {
            file,
            start: i64::try_from(span.start).expect("source offsets fit BAML int"),
            end: i64::try_from(span.end).expect("source offsets fit BAML int"),
        }
        .to_value(vm)
    });
    let code = Value::object(vm.alloc_string(diagnostic.code.as_str()));
    let message = Value::object(vm.alloc_string(diagnostic.message.as_str()));
    copy::Diagnostic {
        code,
        span,
        message,
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
        package
            .runtime_mut()
            .expect("runtime package has an image")
            .initialized = true;
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
        params: Vec::new(),
        ret: Box::new(RealizedTy::null()),
        throws: Box::new(RealizedTy::unknown()),
        attr: TyAttr::default(),
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
    #[allow(clippy::too_many_lines)]
    fn _finish(
        vm: &mut BexVm,
        artifact: &Value,
        packages: &IndexMap<bex_str::BexStr, Value>,
    ) -> NativeCallResult {
        let artifact = match take_compile_artifact(
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
        let plan = match link_dynamic(&artifact.units) {
            Ok(plan) => plan,
            Err(error) => {
                return VmRustFnError::BamlError(VmBamlError::InvalidArgument {
                    message: format!("runtime link failed: {error}"),
                })
                .into();
            }
        };
        let mut dependencies = IndexMap::<String, HeapPtr>::new();
        for (alias, value) in packages {
            // Keep runtime rejection single-sourced with compiler mount filtering.
            if baml_builtins2::reserved_package_names().contains(&alias.as_str()) {
                let diagnostic = super::type_kinds::compiler_diagnostic(
                    DiagnosticId::InvalidSyntax,
                    format!("package alias `{alias}` is reserved"),
                );
                return VmRustFnError::thrown_fresh(super::type_kinds::alloc_compilation_error(
                    vm,
                    &[diagnostic],
                ))
                .into();
            }
            match package_ptr(vm, *value) {
                Ok(ptr) => {
                    dependencies.insert(alias.to_string(), ptr);
                }
                Err(error) => return error.into(),
            }
        }

        let program_package = plan
            .program
            .packages
            .get(&baml_type::Name::new("user"))
            .cloned()
            .unwrap_or_default();
        let package = Package {
            exported_names: program_package.exported_names.clone(),
            classes: IndexMap::new(),
            enums: IndexMap::new(),
            interfaces: IndexMap::new(),
            impl_rules: IndexMap::new(),
            functions: IndexMap::new(),
            type_aliases: IndexMap::new(),
            interface_blob: artifact.interface_blob,
            test_init: None,
            mounted_types: IndexMap::new(),
            kind: PackageKind::Runtime(Box::new(RuntimePackage {
                objects: Box::new([]),
                object_names: IndexMap::new(),
                globals: Box::new([]),
                global_names: IndexMap::new(),
                type_values: IndexMap::new(),
                diagnostics: artifact.diagnostics,
                dependencies: dependencies.values().copied().collect(),
                dependency_names: dependencies.clone(),
                init: None,
                initialized: false,
            })),
        };
        let package_ptr = vm.alloc(Object::Package(Box::new(package)));

        let external_objects: std::collections::HashMap<usize, _> = plan
            .external_objects
            .iter()
            .map(|(index, symbol)| (index.raw(), symbol))
            .collect();
        let mut objects = Vec::with_capacity(plan.program.objects.len());
        for (index, object) in plan.program.objects.iter().enumerate() {
            // A plan-declared external MUST resolve (generic functions are
            // the carve-out: their value objects re-intern locally). The
            // import planner only mints a symbol for a name the compile
            // could see, so a miss is link skew — grafting the linker's
            // `"<runtime-import>"` placeholder as a live object would
            // surface later as an inscrutable error at first use.
            if let Some(symbol) = external_objects.get(&index)
                && !matches!(symbol.kind, bex_vm_types::SymbolKind::GenericFn)
            {
                let resolved = vm.packages.object_by_name(&symbol.fq_name).or_else(|| {
                    // Alias-qualified names — a dependency's own exports and
                    // its mount surface — resolve through the dependency.
                    let (alias, local) = symbol.fq_name.split_once('.')?;
                    dependency_object(vm, *dependencies.get(alias)?, local)
                });
                let Some(ptr) = resolved else {
                    return VmRustFnError::BamlError(VmBamlError::InvalidArgument {
                        message: format!(
                            "Package link could not resolve object `{}`",
                            symbol.fq_name
                        ),
                    })
                    .into();
                };
                objects.push(ptr);
                continue;
            }
            let mut object = object.clone();
            match &mut object {
                Object::Function(function) => {
                    function.runtime_package = package_ptr;
                    function.bytecode.compact = Some(function.bytecode.lower_to_compact());
                }
                Object::GenericFunction(function) => {
                    function.runtime_package = package_ptr;
                }
                // Member back-edges: reaching a declaration keeps its package
                // alive (globals, dependencies, sibling declarations).
                Object::Interface(interface) => interface.owner = package_ptr,
                Object::TypeAlias(alias) => alias.owner = package_ptr,
                _ => {}
            }
            objects.push(vm.alloc(object));
        }
        // Runtime identities are generative: remint before anything reads a
        // declaration's tag, so every downstream read sees the real identity.
        let owned_declarations: Vec<HeapPtr> = objects
            .iter()
            .enumerate()
            .filter(|(index, _)| !external_objects.contains_key(index))
            .map(|(_, ptr)| *ptr)
            .collect();
        let reminted = remint_grafted_declarations(vm, &owned_declarations);

        // Compile-time heap construction resolves `ConstValue::Object` into
        // stable pointers before execution. Dynamic functions need the same
        // pass after every linked object has been grafted. Keep any boxed float
        // constants in the package-owned object graph as well.
        let function_ptrs = objects
            .iter()
            .enumerate()
            // External prefix entries are already-live functions. Their
            // constants are resolved against the owning static/dynamic image,
            // not against this newly linked candidate's object vector.
            .filter(|(index, _)| !external_objects.contains_key(index))
            .map(|(_, ptr)| *ptr)
            .filter(|ptr| matches!(vm.get_object(*ptr), Object::Function(_)))
            .collect::<Vec<_>>();
        for function_ptr in function_ptrs {
            let constants = match vm.get_object(function_ptr) {
                Object::Function(function) => function.bytecode.constants.clone(),
                _ => unreachable!(),
            };
            let mut resolved = Vec::with_capacity(constants.len());
            for constant in constants {
                let value = match constant {
                    bex_vm_types::ConstValue::Type(_)
                    | bex_vm_types::ConstValue::ClassWithTypeArgs { .. }
                    | bex_vm_types::ConstValue::Literal(_) => Value::NULL,
                    bex_vm_types::ConstValue::Float(value) => {
                        let ptr = vm.alloc_float(value);
                        objects.push(ptr);
                        Value::object(ptr)
                    }
                    other => other.to_value(|index| objects[index.raw()]),
                };
                resolved.push(value);
            }
            let Object::Function(function) = vm.get_object_mut(function_ptr) else {
                unreachable!()
            };
            function.bytecode.resolved_constants = resolved;
        }
        bind_interface_defaults(
            vm,
            objects
                .iter()
                .enumerate()
                .filter(|(index, _)| !external_objects.contains_key(index))
                .map(|(_, ptr)| *ptr),
            |index| objects[index.raw()],
        );

        let external_globals: std::collections::HashMap<usize, _> = plan
            .external_globals
            .iter()
            .map(|(index, symbol)| (index.raw(), symbol))
            .collect();
        let mut globals = Vec::with_capacity(plan.program.globals.len());
        for (index, value) in plan.program.globals.iter().enumerate() {
            // Same law as the object lane: a plan-declared external global
            // MUST resolve — falling through would materialize the plan's
            // placeholder const as a silent `null` global.
            if let Some(symbol) = external_globals.get(&index) {
                let resolved = vm
                    .packages
                    .global_by_name(&symbol.fq_name)
                    .map(|index| vm.globals.get(vm.proof(), index))
                    .or_else(|| {
                        let (alias, local) = symbol.fq_name.split_once('.')?;
                        let Object::Package(package) = vm.get_object(*dependencies.get(alias)?)
                        else {
                            return None;
                        };
                        if let Some(runtime) = package.runtime() {
                            let index = runtime
                                .global_names
                                .get(&format!("user.{local}"))
                                .or_else(|| runtime.global_names.get(local))?;
                            runtime.load_global(*index)
                        } else {
                            let canonical = vm.packages.package_name(*dependencies.get(alias)?)?;
                            let index = vm
                                .packages
                                .global_by_name(&format!("{canonical}.{local}"))?;
                            Some(vm.globals.get(vm.proof(), index))
                        }
                    });
                let Some(value) = resolved else {
                    return VmRustFnError::BamlError(VmBamlError::InvalidArgument {
                        message: format!(
                            "Package link could not resolve global `{}`",
                            symbol.fq_name
                        ),
                    })
                    .into();
                };
                globals.push(AtomicValueSlot::new(value));
                continue;
            }
            let value = match value {
                bex_vm_types::ConstValue::Float(value) => Value::object(vm.alloc_float(*value)),
                other => other.to_value(|index| objects[index.raw()]),
            };
            globals.push(AtomicValueSlot::new(value));
        }

        let classes = program_package
            .classes
            .iter()
            .map(|(name, index)| (name.clone(), objects[index.raw()]))
            .collect::<IndexMap<_, _>>();
        let enums = program_package
            .enums
            .iter()
            .map(|(name, index)| (name.clone(), objects[index.raw()]))
            .collect::<IndexMap<_, _>>();
        let interfaces = program_package
            .interfaces
            .iter()
            .map(|(name, index)| (name.clone(), objects[index.raw()]))
            .collect::<IndexMap<_, _>>();
        let mut impl_rules = IndexMap::new();
        for (interface_index, rules) in &program_package.impl_rules {
            let interface_ptr = objects[interface_index.raw()];
            let mut pointers = Vec::with_capacity(rules.len());
            for rule in rules {
                let runtime_rule = RuntimeImplRule {
                    interface_head: objects[rule.interface_head.raw()],
                    for_ty_pattern: rule.for_ty_pattern.clone(),
                    generic_param_bounds: rule.generic_param_bounds.clone(),
                    interface_args: rule.interface_args.clone(),
                    interface_assoc: rule.interface_assoc.clone(),
                    methods: rule
                        .methods
                        .iter()
                        .map(|(name, method)| {
                            (
                                name.clone(),
                                MethodImpl {
                                    fqn: objects[method.fqn.raw()],
                                    frame: method.frame.clone(),
                                },
                            )
                        })
                        .collect(),
                    field_links: rule.field_links.clone(),
                };
                let pointer = vm.alloc(Object::ImplRule(Box::new(runtime_rule)));
                objects.push(pointer);
                pointers.push(pointer);
            }
            impl_rules.insert(interface_ptr, pointers);
        }
        // Every owned object now exists — including the impl rules, whose
        // patterns carry heads — so the graft can bind and prove totality.
        let owned_for_bind: Vec<HeapPtr> = objects
            .iter()
            .enumerate()
            .filter(|(index, _)| !external_objects.contains_key(index))
            .map(|(_, ptr)| *ptr)
            .collect();
        let mut named_surfaces = Vec::new();
        for (alias, &dep_ptr) in &dependencies {
            dependency_named_declarations(vm, alias, dep_ptr, &mut named_surfaces);
        }
        if let Err(error) =
            bind_graft_type_heads(vm, &objects, &owned_for_bind, &named_surfaces, &reminted)
        {
            return error.into();
        }
        let functions = program_package
            .functions
            .iter()
            .map(|(name, index)| (name.clone(), objects[index.raw()]))
            .collect::<IndexMap<_, _>>();
        // `global_names` is the runtime package's *link* table: a consumer
        // mounting this package resolves its imports here. Interface bodies
        // do not publish: a consumer reaches this package's impl methods
        // through the virtual road (rule tables carry body pointers; adopted
        // defaults resolve through the interface's `default_fn`) and checks
        // their signatures through the package-interface blob — no current
        // lowering emits a name-addressed body reference against a mounted
        // surface. A future devirtualized direct call must reference the
        // body rule-relatively, never by name.
        let global_names = plan
            .program
            .function_global_indices
            .iter()
            .chain(&plan.program.let_global_indices)
            .filter(|(name, _)| name.starts_with("user."))
            .map(|(name, index)| (name.clone(), *index))
            .collect::<IndexMap<_, _>>();
        // The object-lane twin of `global_names`: every named function this
        // package compiled, under its canonical spelling. A consumer's static
        // call to a class-inherent method imports the function object by that
        // class-qualified name, which no item table rows (see
        // `dependency_named_object`).
        let object_names = plan
            .program
            .function_indices
            .iter()
            .filter(|(name, _)| name.starts_with("user."))
            .map(|(name, index)| (name.clone(), objects[*index]))
            .collect::<IndexMap<_, _>>();
        let init = plan
            .program
            .package_init_order
            .iter()
            .find(|name| name.as_str() == "$init" || name.starts_with("user.$init"))
            .and_then(|name| plan.program.function_indices.get(name))
            .map(|index| objects[*index]);
        let test_init = program_package.test_init.map(|index| objects[index.raw()]);

        let type_values =
            allocate_runtime_declaration_types(vm, package_ptr, &classes, &enums, &interfaces);

        let Object::Package(package) = vm.get_object_mut(package_ptr) else {
            unreachable!("package was just allocated")
        };
        package.classes = classes;
        package.enums = enums;
        package.interfaces = interfaces;
        package.impl_rules = impl_rules;
        package.functions = functions;
        package.test_init = test_init;
        let runtime = package.runtime_mut().expect("runtime package image");
        runtime.objects = objects.into_boxed_slice();
        runtime.object_names = object_names;
        runtime.globals = globals.into_boxed_slice();
        runtime.global_names = global_names;
        runtime.type_values = type_values;
        runtime.init = init;

        let wrapper = copy::Package {
            _inner: Value::object(package_ptr),
        }
        .to_value(vm);
        let wrapper_ptr = wrapper
            .as_object_ptr()
            .expect("Package copy helper allocates an instance");
        if let Some(init) = init {
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
            package.runtime_mut().expect("runtime image").initialized = true;
            NativeCallResult::Done(wrapper)
        }
    }

    fn get_class(vm: &mut BexVm, package: &Value, name: &bex_str::BexStr) -> Option<Value> {
        let ptr = package_ptr(vm, *package).ok()?;
        let Object::Package(package) = vm.get_object(ptr) else {
            return None;
        };
        let local = local_name(name.as_str())?;
        if local.name.as_str().ends_with("$stream") {
            return None;
        }
        let class_ptr = package.classes.get(&local).copied()?;
        let runtime_type = stored_package_type(package, &local);
        Some(package_class_type(vm, runtime_type, class_ptr))
    }

    fn get_enum(vm: &mut BexVm, package: &Value, name: &bex_str::BexStr) -> Option<Value> {
        let ptr = package_ptr(vm, *package).ok()?;
        let Object::Package(package) = vm.get_object(ptr) else {
            return None;
        };
        let local = local_name(name.as_str())?;
        let enum_ptr = package.enums.get(&local).copied()?;
        let runtime_type = stored_package_type(package, &local);
        Some(package_enum_type(vm, runtime_type, enum_ptr))
    }

    fn get_interface(vm: &mut BexVm, package: &Value, name: &bex_str::BexStr) -> Option<Value> {
        let ptr = package_ptr(vm, *package).ok()?;
        let Object::Package(package) = vm.get_object(ptr) else {
            return None;
        };
        let local = local_name(name.as_str())?;
        let interface_ptr = package.interfaces.get(&local).copied()?;
        let runtime_type = stored_package_type(package, &local);
        Some(package_interface_type(vm, runtime_type, interface_ptr))
    }

    fn with_types(
        vm: &mut BexVm,
        package: &Value,
        types: &IndexMap<bex_str::BexStr, Value>,
    ) -> Result<Value, VmRustFnError> {
        let source_ptr = package_ptr(vm, *package)?;
        let Object::Package(source) = vm.get_object(source_ptr) else {
            unreachable!("package_ptr validates Object::Package")
        };
        let mut derived = (**source).clone();

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
            if derived.classes.contains_key(&local)
                || derived.enums.contains_key(&local)
                || derived.interfaces.contains_key(&local)
                || derived.exported_names.contains(&local)
                || derived.mounted_types.contains_key(&export)
            {
                return Err(compilation_error(
                    vm,
                    DiagnosticId::DuplicateName,
                    format!("duplicate exported type name `{export}`"),
                ));
            }
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
            // The head is the declaration being mounted — no lookup, no
            // owner-package scan, and no name that could resolve to a
            // same-named declaration from somewhere else.
            match &type_value.ty {
                RealizedTy::Class(head, _, _) => {
                    derived.classes.insert(local.clone(), head.ptr());
                }
                RealizedTy::Enum(head, _) => {
                    derived.enums.insert(local.clone(), head.ptr());
                }
                RealizedTy::Interface(head, _, _, _) => {
                    derived.interfaces.insert(local.clone(), head.ptr());
                }
                _ => {}
            }
            derived.mounted_types.insert(export, type_ptr);
            derived.exported_names.push(local);
        }

        let derived_ptr = vm.alloc(Object::Package(Box::new(derived)));
        Ok(copy::Package {
            _inner: Value::object(derived_ptr),
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
        let function = match vm.get_object(package_ptr) {
            Object::Package(package) => package.functions.get(&local).copied(),
            _ => None,
        };
        let Some(function) = function else {
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

    fn classes(vm: &mut BexVm, package: &Value) -> IndexMap<bex_str::BexStr, Value> {
        let Ok(ptr) = package_ptr(vm, *package) else {
            return IndexMap::new();
        };
        let Object::Package(package) = vm.get_object(ptr) else {
            return IndexMap::new();
        };
        let entries = package
            .classes
            .iter()
            .filter(|(name, _)| !name.name.as_str().ends_with("$stream"))
            .map(|(name, &class)| (name.clone(), class, stored_package_type(package, name)))
            .collect::<Vec<_>>();
        entries
            .into_iter()
            .map(|(name, class, runtime_type)| {
                (
                    display_local_name(&name).into(),
                    package_class_type(vm, runtime_type, class),
                )
            })
            .collect()
    }

    fn enums(vm: &mut BexVm, package: &Value) -> IndexMap<bex_str::BexStr, Value> {
        let Ok(ptr) = package_ptr(vm, *package) else {
            return IndexMap::new();
        };
        let Object::Package(package) = vm.get_object(ptr) else {
            return IndexMap::new();
        };
        let entries = package
            .enums
            .iter()
            .map(|(name, &enm)| (name.clone(), enm, stored_package_type(package, name)))
            .collect::<Vec<_>>();
        entries
            .into_iter()
            .map(|(name, enm, runtime_type)| {
                (
                    display_local_name(&name).into(),
                    package_enum_type(vm, runtime_type, enm),
                )
            })
            .collect()
    }

    fn interfaces(vm: &mut BexVm, package: &Value) -> IndexMap<bex_str::BexStr, Value> {
        let Ok(ptr) = package_ptr(vm, *package) else {
            return IndexMap::new();
        };
        let Object::Package(package) = vm.get_object(ptr) else {
            return IndexMap::new();
        };
        let entries = package
            .interfaces
            .iter()
            .map(|(name, &interface)| (name.clone(), interface, stored_package_type(package, name)))
            .collect::<Vec<_>>();
        entries
            .into_iter()
            .map(|(name, interface, runtime_type)| {
                (
                    display_local_name(&name).into(),
                    package_interface_type(vm, runtime_type, interface),
                )
            })
            .collect()
    }

    fn functions(vm: &mut BexVm, package: &Value) -> IndexMap<bex_str::BexStr, Value> {
        let Ok(ptr) = package_ptr(vm, *package) else {
            return IndexMap::new();
        };
        let Object::Package(package) = vm.get_object(ptr) else {
            return IndexMap::new();
        };
        let functions = package
            .functions
            .iter()
            .map(|(name, &function)| (name.clone(), function))
            .collect::<Vec<_>>();
        let functions = functions
            .into_iter()
            .filter(|(_, function)| {
                !matches!(
                    vm.get_object(*function),
                    Object::Function(function)
                        if function.origin == bex_vm_types::FunctionOrigin::Internal
                )
            })
            .map(|(name, _)| name)
            .collect::<Vec<_>>();
        functions
            .into_iter()
            .filter_map(|name| {
                function_type(vm, ptr, &name).map(|ty| (display_local_name(&name).into(), ty))
            })
            .collect()
    }

    fn tests(vm: &mut BexVm, package: &Value) -> NativeCallResult {
        let package_ptr = match package_ptr(vm, *package) {
            Ok(package) => package,
            Err(error) => return error.into(),
        };
        let test_init = match vm.get_object(package_ptr) {
            Object::Package(package) => package.test_init,
            _ => None,
        };
        let Some(test_init) = test_init else {
            return NativeCallResult::Done(empty_tests(vm));
        };
        let Some(constructor) = vm.packages.object_by_name("testing.TestCollector.new") else {
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
            Object::Package(package) => package
                .runtime()
                .map(|runtime| runtime.diagnostics.clone())
                .unwrap_or_default(),
            _ => Vec::new(),
        };
        diagnostics
            .iter()
            .map(|diagnostic| diagnostic_value(vm, diagnostic))
            .collect()
    }
}

/// Give every owned grafted declaration its own runtime identity.
///
/// A runtime-compiled declaration is generative: two compiles of one source
/// are two types, and a declaration spelled like a static one is not that
/// static type (`TYPE_SYSTEM.md` — nominal identity is the declaration, not the
/// spelling). Emit content-addresses tags from names, so without this two
/// same-named declarations would be tag-equal — reminting to a counter tag
/// makes each graft's declarations identity-distinct by construction.
///
/// Safe against baked bytecode: jump tables carry only the coarse kind tags
/// (`realized_type_tag` answers `None` for declared heads), and class/enum
/// match arms compare `IsType` pointers, which the graft resolved to these
/// same objects.
///
/// Returns the `old content tag → reminted head` rows the head bind uses to
/// bridge the plan's internal references onto the new identities.
fn remint_grafted_declarations(
    vm: &mut BexVm,
    owned: &[HeapPtr],
) -> Vec<(baml_type::typetag::TypeTag, bex_vm_types::TypeHead)> {
    let mut reminted = Vec::new();
    for &ptr in owned {
        let fresh = baml_type::typetag::TypeTag::fresh_dynamic();
        let old = match vm.get_object_mut(ptr) {
            Object::Class(class) => std::mem::replace(&mut class.type_tag, fresh),
            Object::Enum(enm) => std::mem::replace(&mut enm.type_tag, fresh),
            Object::Interface(interface) => std::mem::replace(&mut interface.type_tag, fresh),
            Object::TypeAlias(alias) => std::mem::replace(&mut alias.type_tag, fresh),
            _ => continue,
        };
        reminted.push((old, bex_vm_types::TypeHead::new(ptr, fresh)));
    }
    reminted
}

/// Collect every declaration a graft can legitimately name through `alias`,
/// as `(fq_name, declaration)` rows for [`bind_graft_type_heads`]'s
/// named-surface bridge: the dependency's exported declarations and its
/// mounted types (whose values are `Object::Type`s wrapping the declaration's
/// own head). Non-declarations are filtered by the bind itself.
fn dependency_named_declarations(
    vm: &BexVm,
    alias: &str,
    package_ptr: HeapPtr,
    out: &mut Vec<(String, HeapPtr)>,
) {
    let Object::Package(package) = vm.get_object(package_ptr) else {
        return;
    };
    let fq = |local: &bex_vm_types::types::LocalName| -> String {
        std::iter::once(alias)
            .chain(local.namespace.iter().map(baml_type::Name::as_str))
            .chain(std::iter::once(local.name.as_str()))
            .collect::<Vec<_>>()
            .join(".")
    };
    for (local, &ptr) in package
        .classes
        .iter()
        .chain(&package.enums)
        .chain(&package.interfaces)
        .chain(&package.type_aliases)
    {
        out.push((fq(local), ptr));
    }
    for (name, &type_ptr) in &package.mounted_types {
        let Object::Type(value) = vm.get_object(type_ptr) else {
            continue;
        };
        let head = match &value.ty {
            RealizedTy::Class(head, _, _) => head,
            RealizedTy::Enum(head, _) => head,
            RealizedTy::Interface(head, _, _, _) => head,
            RealizedTy::TypeAlias(head, _) => head,
            _ => continue,
        };
        if head.is_resolved() {
            out.push((format!("{alias}.{name}"), head.ptr()));
        }
        // Every runtime declaration the mounted type reaches is spelled
        // `alias.<item name>` in the consumer compile world (the blob rows
        // name it that way), so index those spellings too.
        for ptr in crate::reachable::runtime_definitions(vm, &value.ty) {
            let item = match vm.get_object(ptr) {
                Object::Class(class) => class.name.item_name(),
                Object::Enum(enm) => enm.name.item_name(),
                _ => continue,
            };
            out.push((format!("{alias}.{item}"), ptr));
        }
    }
}

/// Bind every type head an owned grafted object carries to the declaration it
/// names — the runtime-package twin of [`bex_heap::BexHeap::bind_type_heads`].
///
/// A runtime compile's emit mints heads tag-only, exactly like the static
/// emit, so each grafted object arrives carrying unresolved heads. Tags
/// resolve against the plan pool first (`plan_objects` spans it — its own
/// declarations *and* the live external ones its symbols imported, so a
/// reference into an earlier eval lands on that eval's object), then against
/// a transient index over the compile-time pool for the type-only references
/// no import symbol carries. Both indices are built here and dropped here.
///
/// A tag nothing declares is a link error, not a head to leave dangling: an
/// unresolved head is untraceable by the collector and unresolvable by
/// dispatch.
fn bind_graft_type_heads(
    vm: &mut BexVm,
    plan_objects: &[HeapPtr],
    owned: &[HeapPtr],
    named_surfaces: &[(String, HeapPtr)],
    reminted: &[(baml_type::typetag::TypeTag, bex_vm_types::TypeHead)],
) -> Result<(), VmRustFnError> {
    // Every entry carries BOTH halves off the target declaration: a head that
    // lands here adopts the declaration's identity, not just its address. For
    // a plan-local or compile-time target the two tags coincide; for a
    // runtime-created target reached by name they do not, and the live tag is
    // the identity.
    let mut by_tag: std::collections::HashMap<baml_type::typetag::TypeTag, bex_vm_types::TypeHead> =
        std::collections::HashMap::new();
    // Reminted rows first: a plan-internal reference spells its sibling by the
    // emit-time content tag, and must land on the reminted identity — the
    // plan's own declaration wins that spelling over any same-named surface.
    for &(old_tag, head) in reminted {
        by_tag.insert(old_tag, head);
        by_tag.insert(head.tag(), head);
    }
    for &ptr in plan_objects {
        if let Some(tag) = bex_heap::BexHeap::declaration_tag(vm.get_object(ptr)) {
            by_tag
                .entry(tag)
                .or_insert_with(|| bex_vm_types::TypeHead::new(ptr, tag));
        }
    }
    // A declaration reachable by *name* — a dependency's export, a mounted
    // type, an earlier eval's declaration — may be runtime-created, in which
    // case its live tag is a counter mint while the head naming it carries a
    // content tag. Emit spells such a reference by whichever name it resolved
    // to — the surface's mount name or the declaration's own (synthesized)
    // name — so each declaration is indexed under both spellings' tags, plus
    // its live tag for a head that already carries the identity.
    for (fq_name, ptr) in named_surfaces {
        let object = vm.get_object(*ptr);
        let Some(live_tag) = bex_heap::BexHeap::declaration_tag(object) else {
            continue;
        };
        let bound = bex_vm_types::TypeHead::new(*ptr, live_tag);
        // An anonymous declaration has no spelling of its own — it is only
        // reachable through the surface names it was mounted under.
        let declared = match object {
            Object::Class(class) => class.name.declared().cloned(),
            Object::Enum(enm) => enm.name.declared().cloned(),
            Object::Interface(interface) => Some(interface.name.clone()),
            Object::TypeAlias(alias) => Some(alias.name.clone()),
            _ => None,
        };
        if let Some(name) = declared {
            by_tag
                .entry(baml_type::typetag::TypeTag::of_head(
                    &name.render_dotted(false),
                ))
                .or_insert(bound);
        }
        by_tag
            .entry(baml_type::typetag::TypeTag::of_head(fq_name))
            .or_insert(bound);
        by_tag.entry(live_tag).or_insert(bound);
    }
    let compile_time = vm.heap.compile_time_declaration_index();
    let mut unbound: Vec<baml_type::typetag::TypeTag> = Vec::new();
    for &ptr in owned {
        bex_vm_types::head_walk::visit_object_heads_mut(vm.get_object_mut(ptr), &mut |head| {
            if head.is_resolved() {
                return;
            }
            if let Some(&bound) = by_tag.get(&head.tag()) {
                *head = bound;
            } else if let Some(&declaration) = compile_time.get(&head.tag()) {
                // The compile-time index is keyed by each declaration's own
                // tag, so the head's tag already is the identity.
                head.resolve(declaration);
            } else {
                unbound.push(head.tag());
            }
        });
    }
    if unbound.is_empty() {
        return Ok(());
    }
    unbound.sort_unstable();
    unbound.dedup();
    Err(VmRustFnError::BamlError(VmBamlError::InvalidArgument {
        message: format!(
            "runtime link produced type references nothing declares (tags {unbound:?})"
        ),
    }))
}

/// Bind each freshly grafted interface's default-method bodies from the pool
/// index its emit wrote to the heap pointer the object landed at — the
/// runtime-package twin of the compile-time heap's own resolution pass.
/// `resolve` maps a plan-local `ObjectIndex` to its live pointer.
fn bind_interface_defaults(
    vm: &mut BexVm,
    candidates: impl Iterator<Item = HeapPtr>,
    resolve: impl Fn(bex_vm_types::ObjectIndex) -> HeapPtr,
) {
    let interfaces = candidates
        .filter(|ptr| matches!(vm.get_object(*ptr), Object::Interface(_)))
        .collect::<Vec<_>>();
    for interface_ptr in interfaces {
        let Object::Interface(interface) = vm.get_object_mut(interface_ptr) else {
            unreachable!()
        };
        for method in &mut interface.methods {
            if let Some(default) = method.default {
                method.default_fn = resolve(default);
            }
        }
    }
}

fn session_external_object(
    vm: &BexVm,
    session: HeapPtr,
    dependencies: &IndexMap<String, HeapPtr>,
    name: &str,
) -> Option<HeapPtr> {
    if let Object::Package(package) = vm.get_object(session)
        && let Some(pointer) = package
            .runtime()
            .and_then(|runtime| runtime.object_names.get(name))
    {
        return Some(*pointer);
    }
    vm.packages.object_by_name(name).or_else(|| {
        let (alias, local) = name.split_once('.')?;
        dependency_object(vm, *dependencies.get(alias)?, local)
    })
}

fn session_external_global(
    vm: &BexVm,
    session: HeapPtr,
    dependencies: &IndexMap<String, HeapPtr>,
    name: &str,
) -> Option<Value> {
    if let Object::Package(package) = vm.get_object(session)
        && let Some(runtime) = package.runtime()
        && let Some(index) = runtime.global_names.get(name)
    {
        return runtime.load_global(*index);
    }
    vm.packages
        .global_by_name(name)
        .map(|index| vm.globals.get(vm.proof(), index))
        .or_else(|| {
            let (alias, local) = name.split_once('.')?;
            let dependency = *dependencies.get(alias)?;
            let Object::Package(package) = vm.get_object(dependency) else {
                return None;
            };
            if let Some(runtime) = package.runtime() {
                let index = runtime
                    .global_names
                    .get(&format!("user.{local}"))
                    .or_else(|| runtime.global_names.get(local))?;
                runtime.load_global(*index)
            } else {
                let package_name = vm.packages.package_name(dependency)?;
                let index = vm
                    .packages
                    .global_by_name(&format!("{package_name}.{local}"))?;
                Some(vm.globals.get(vm.proof(), index))
            }
        })
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
            let PackageKind::Session { runtime, state } = &mut package.kind else {
                unreachable!("Session continuation retained a Session package")
            };
            runtime.globals[action.target].store(value);

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

fn graft_session_submission(
    vm: &mut BexVm,
    package_ptr: HeapPtr,
    artifact: &RuntimeCompileArtifact,
    metadata: &RuntimeSessionCompileArtifact,
) -> Result<Vec<SessionAction>, VmRustFnError> {
    let plan = link_dynamic(&artifact.units).map_err(|error| VmBamlError::InvalidArgument {
        message: format!("Session runtime link failed: {error}"),
    })?;
    let (dependencies, existing_globals, existing_objects, existing_len, runtime_objects) = {
        let Object::Package(package) = vm.get_object(package_ptr) else {
            unreachable!("Session payload is a Package")
        };
        let runtime = package.runtime().expect("Session runtime image");
        (
            runtime.dependency_names.clone(),
            runtime.global_names.clone(),
            runtime.object_names.clone(),
            runtime.globals.len(),
            runtime.objects.to_vec(),
        )
    };
    let external_objects = plan
        .external_objects
        .iter()
        .map(|(index, symbol)| (index.raw(), symbol))
        .collect::<HashMap<_, _>>();
    let external_globals = plan
        .external_globals
        .iter()
        .map(|(index, symbol)| (index.raw(), symbol))
        .collect::<HashMap<_, _>>();

    // Assign stable Session slots first. Imports of prior Session cells reuse
    // their old slot; every new definition gets a fresh slot, which is the
    // cell/shadowing boundary.
    let mut global_map = vec![0usize; plan.program.globals.len()];
    let mut next_global = existing_len;
    let mut imported_values = HashMap::<usize, Value>::new();
    let mut cached_import_names = Vec::<(String, usize)>::new();
    for (plan_index, stable_slot) in global_map.iter_mut().enumerate() {
        if let Some(symbol) = external_globals.get(&plan_index) {
            if let Some(&stable) = existing_globals.get(&symbol.fq_name) {
                *stable_slot = stable;
                continue;
            }
            let value = session_external_global(vm, package_ptr, &dependencies, &symbol.fq_name)
                .ok_or_else(|| VmBamlError::InvalidArgument {
                    message: format!("Session link could not resolve global `{}`", symbol.fq_name),
                })?;
            *stable_slot = next_global;
            imported_values.insert(plan_index, value);
            cached_import_names.push((symbol.fq_name.clone(), next_global));
            next_global += 1;
        } else {
            *stable_slot = next_global;
            next_global += 1;
        }
    }

    // Allocate all owned objects before patching any object operands so cycles
    // and mutually-recursive declarations have a complete pointer map.
    let mut objects = Vec::with_capacity(plan.program.objects.len());
    let mut owned = Vec::new();
    for (index, object) in plan.program.objects.iter().enumerate() {
        // A plan-declared external MUST resolve (generic functions are the
        // carve-out: their value objects re-intern locally) — the same law
        // the global loop above already enforces. Falling through would
        // graft the linker's `"<runtime-import>"` placeholder as a live
        // object.
        if let Some(symbol) = external_objects.get(&index)
            && !matches!(symbol.kind, bex_vm_types::SymbolKind::GenericFn)
        {
            let pointer = existing_objects
                .get(&symbol.fq_name)
                .copied()
                .or_else(|| {
                    session_external_object(vm, package_ptr, &dependencies, &symbol.fq_name)
                })
                .ok_or_else(|| VmBamlError::InvalidArgument {
                    message: format!("Session link could not resolve object `{}`", symbol.fq_name),
                })?;
            objects.push(pointer);
            continue;
        }
        let pointer = vm.alloc(object.clone());
        objects.push(pointer);
        owned.push(pointer);
    }
    // Runtime identities are generative: remint before anything reads a
    // declaration's tag (`allocate_runtime_declaration_types` builds this
    // eval's `type` values off them below).
    let reminted = remint_grafted_declarations(vm, &owned);
    let mut object_map = vec![0usize; objects.len()];
    let mut appended_objects = Vec::new();
    for (index, pointer) in objects.iter().copied().enumerate() {
        if let Some(stable) = runtime_objects
            .iter()
            .position(|existing| *existing == pointer)
        {
            object_map[index] = stable;
        } else {
            object_map[index] = runtime_objects.len() + appended_objects.len();
            appended_objects.push(pointer);
        }
    }
    let stable_objects = runtime_objects
        .iter()
        .copied()
        .chain(appended_objects.iter().copied())
        .collect::<Vec<_>>();
    for pointer in &owned {
        let object = vm.get_object_mut(*pointer);
        visit_object_operands(object, |operand| match operand {
            IndexOperand::Object(index) => {
                *index = bex_vm_types::ObjectIndex::from_raw(object_map[index.raw()]);
            }
            IndexOperand::Global(index) => {
                *index = bex_vm_types::GlobalIndex::from_raw(global_map[index.raw()]);
            }
        });
        match object {
            Object::Function(function) => {
                function.runtime_package = package_ptr;
                function.bytecode.compact = Some(function.bytecode.lower_to_compact());
            }
            Object::GenericFunction(function) => function.runtime_package = package_ptr,
            // Member back-edges, as in `Package.compile` above.
            Object::Interface(interface) => interface.owner = package_ptr,
            Object::TypeAlias(alias) => alias.owner = package_ptr,
            _ => {}
        }
    }

    let function_ptrs = owned
        .iter()
        .copied()
        .filter(|pointer| matches!(vm.get_object(*pointer), Object::Function(_)))
        .collect::<Vec<_>>();
    let mut extra_owned = Vec::new();
    for function_ptr in function_ptrs {
        let constants = match vm.get_object(function_ptr) {
            Object::Function(function) => function.bytecode.constants.clone(),
            _ => unreachable!(),
        };
        let mut resolved = Vec::with_capacity(constants.len());
        for constant in constants {
            let value = match constant {
                bex_vm_types::ConstValue::Type(_)
                | bex_vm_types::ConstValue::ClassWithTypeArgs { .. }
                | bex_vm_types::ConstValue::Literal(_) => Value::NULL,
                bex_vm_types::ConstValue::Float(value) => {
                    let pointer = vm.alloc_float(value);
                    extra_owned.push(pointer);
                    Value::object(pointer)
                }
                other => other.to_value(|index| stable_objects[index.raw()]),
            };
            resolved.push(value);
        }
        let Object::Function(function) = vm.get_object_mut(function_ptr) else {
            unreachable!()
        };
        function.bytecode.resolved_constants = resolved;
    }
    bind_interface_defaults(vm, owned.iter().copied(), |index| {
        stable_objects[index.raw()]
    });

    let mut appended = vec![Value::NULL; next_global - existing_len];
    for (plan_index, constant) in plan.program.globals.iter().enumerate() {
        let stable = global_map[plan_index];
        if stable < existing_len {
            continue;
        }
        let value = if let Some(value) = imported_values.get(&plan_index) {
            *value
        } else {
            match constant {
                bex_vm_types::ConstValue::Float(value) => {
                    let pointer = vm.alloc_float(*value);
                    extra_owned.push(pointer);
                    Value::object(pointer)
                }
                other => other.to_value(|index| objects[index.raw()]),
            }
        };
        appended[stable - existing_len] = value;
    }

    let program_package = plan
        .program
        .packages
        .get(&baml_type::Name::new("user"))
        .cloned()
        .unwrap_or_default();
    let new_classes = program_package
        .classes
        .iter()
        .map(|(name, index)| (name.clone(), objects[index.raw()]))
        .collect::<IndexMap<_, _>>();
    let new_enums = program_package
        .enums
        .iter()
        .map(|(name, index)| (name.clone(), objects[index.raw()]))
        .collect::<IndexMap<_, _>>();
    let new_interfaces = program_package
        .interfaces
        .iter()
        .map(|(name, index)| (name.clone(), objects[index.raw()]))
        .collect::<IndexMap<_, _>>();
    // Recursive aliases are pooled declarations now, so they relocate exactly
    // like classes: resolve each index against the freshly linked image.
    let new_type_aliases = program_package
        .type_aliases
        .iter()
        .map(|(name, index)| (name.clone(), objects[index.raw()]))
        .collect::<IndexMap<_, _>>();
    let new_type_values = allocate_runtime_declaration_types(
        vm,
        package_ptr,
        &new_classes,
        &new_enums,
        &new_interfaces,
    );
    let new_functions = program_package
        .functions
        .iter()
        .map(|(name, index)| (name.clone(), objects[index.raw()]))
        .collect::<IndexMap<_, _>>();
    let mut new_impl_rules = IndexMap::<HeapPtr, Vec<HeapPtr>>::new();
    for (interface_index, rules) in &program_package.impl_rules {
        let interface = objects[interface_index.raw()];
        let mut pointers = Vec::new();
        for rule in rules {
            let runtime_rule = RuntimeImplRule {
                interface_head: objects[rule.interface_head.raw()],
                for_ty_pattern: rule.for_ty_pattern.clone(),
                generic_param_bounds: rule.generic_param_bounds.clone(),
                interface_args: rule.interface_args.clone(),
                interface_assoc: rule.interface_assoc.clone(),
                methods: rule
                    .methods
                    .iter()
                    .map(|(name, method)| {
                        (
                            name.clone(),
                            MethodImpl {
                                fqn: objects[method.fqn.raw()],
                                frame: method.frame.clone(),
                            },
                        )
                    })
                    .collect(),
                field_links: rule.field_links.clone(),
            };
            let pointer = vm.alloc(Object::ImplRule(Box::new(runtime_rule)));
            extra_owned.push(pointer);
            pointers.push(pointer);
        }
        new_impl_rules.insert(interface, pointers);
    }
    // Every owned object now exists — including the impl rules, whose patterns
    // carry heads — so the graft can bind and prove totality. `extra_owned`'s
    // boxed floats carry no heads and pass through the walk untouched.
    let bind_set: Vec<HeapPtr> = owned.iter().chain(&extra_owned).copied().collect();
    let mut named_surfaces = Vec::new();
    for (alias, &dep_ptr) in &dependencies {
        dependency_named_declarations(vm, alias, dep_ptr, &mut named_surfaces);
    }
    // A session's earlier evals published their declarations under fully
    // qualified names; a later eval's type positions name them the same way.
    if let Object::Package(package) = vm.get_object(package_ptr)
        && let Some(runtime) = package.runtime()
    {
        named_surfaces.extend(
            runtime
                .object_names
                .iter()
                .map(|(name, &ptr)| (name.clone(), ptr)),
        );
    }
    bind_graft_type_heads(vm, &objects, &bind_set, &named_surfaces, &reminted)?;

    let mut object_name_updates = IndexMap::new();
    for (name, index) in &plan.program.function_indices {
        if name.starts_with("user.") {
            object_name_updates.insert(name.clone(), objects[*index]);
        }
    }
    for (name, pointer) in new_classes.iter().chain(&new_enums).chain(&new_interfaces) {
        object_name_updates.insert(
            format!(
                "user.{}",
                display_local_name(name).trim_start_matches("root.")
            ),
            *pointer,
        );
    }
    // Interface bodies do not publish (see the `Package.compile` twin): a
    // later eval reaches this eval's impl methods only through the virtual
    // road — earlier-eval declarations are mounted surfaces with no source
    // lane, so no static body reference can even be lowered against them.
    let global_name_updates = plan
        .program
        .function_global_indices
        .iter()
        .chain(&plan.program.let_global_indices)
        .filter(|(name, _)| name.starts_with("user."))
        .map(|(name, index)| (name.clone(), global_map[*index]))
        .collect::<Vec<_>>();

    let named_slots = plan
        .program
        .function_global_indices
        .values()
        .chain(plan.program.let_global_indices.values())
        .copied()
        .collect::<HashSet<_>>();
    let helper_globals = plan
        .program
        .globals
        .iter()
        .enumerate()
        .filter(|(index, _)| !named_slots.contains(index))
        .filter_map(|(_, value)| match value {
            bex_vm_types::ConstValue::Object(index) => match &plan.program.objects[*index] {
                // An interface body owns an unnamed slot but is not an init
                // helper — including it would shift every positional
                // `helper_slot` ordinal the compile side assigned.
                Object::Function(function)
                    if function.source_file == metadata.submission_name
                        && !function.is_interface_body =>
                {
                    Some(objects[index.raw()])
                }
                _ => None,
            },
            _ => None,
        })
        .collect::<Vec<_>>();
    let step_by_global = metadata
        .steps
        .iter()
        .enumerate()
        .map(|(index, step)| (step.global.as_str(), index))
        .collect::<HashMap<_, _>>();
    let actions = metadata
        .initializers
        .iter()
        .map(|initializer| {
            let helper = helper_globals
                .get(initializer.helper_slot as usize)
                .copied()
                .ok_or_else(|| VmBamlError::InvalidArgument {
                    message: "Session initializer helper was not linked".to_string(),
                })?;
            let plan_global = plan
                .program
                .let_global_indices
                .get(&initializer.target_global)
                .ok_or_else(|| VmBamlError::InvalidArgument {
                    message: format!(
                        "Session initializer target `{}` was not linked",
                        initializer.target_global
                    ),
                })?;
            let step = step_by_global
                .get(initializer.target_global.as_str())
                .copied();
            let target = step
                .and_then(|index| metadata.steps[index].commit_global.as_ref())
                .and_then(|name| existing_globals.get(name))
                .copied()
                .unwrap_or(global_map[*plan_global]);
            Ok(SessionAction {
                helper,
                target,
                step,
            })
        })
        .collect::<Result<Vec<_>, VmBamlError>>()?;
    let Object::Package(package) = vm.get_object_mut(package_ptr) else {
        unreachable!()
    };
    package.classes.extend(new_classes);
    package.enums.extend(new_enums);
    package.interfaces.extend(new_interfaces);
    package.functions.extend(new_functions);
    package.type_aliases.extend(new_type_aliases);
    for (interface, rules) in new_impl_rules {
        package
            .impl_rules
            .entry(interface)
            .or_default()
            .extend(rules);
    }
    package.interface_blob.clone_from(&artifact.interface_blob);
    let PackageKind::Session { runtime, state } = &mut package.kind else {
        unreachable!("Session graft target changed package kind")
    };
    if !metadata.declaration_source.trim().is_empty() {
        state.history.insert(
            metadata.submission_name.clone(),
            metadata.declaration_source.clone(),
        );
    }
    state.visible.extend(metadata.declarations.clone());
    let mut globals = runtime.globals.to_vec();
    globals.extend(appended.into_iter().map(AtomicValueSlot::new));
    runtime.globals = globals.into_boxed_slice();
    let mut retained_objects = runtime.objects.to_vec();
    retained_objects.extend(appended_objects);
    retained_objects.extend(extra_owned);
    runtime.objects = retained_objects.into_boxed_slice();
    runtime.object_names.extend(object_name_updates);
    runtime.global_names.extend(cached_import_names);
    runtime.global_names.extend(global_name_updates);
    // Every declaration submission is generative: it creates fresh declarations
    // owned by the Session package. Overwriting a visible name updates only the
    // newest lookup; values and functions from older submissions keep pointing
    // at the declarations they were built against.
    runtime.type_values.extend(new_type_values);
    runtime.diagnostics.clone_from(&artifact.diagnostics);
    // The maps above now hold fresh young pointers inside a package object that
    // may itself have been promoted long ago. Without dirtying its card, a minor
    // collection would never rescan it and the new declarations would be
    // collected out from under the session.
    vm.tlab.heap().conservative_write_barrier(package_ptr);
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
            if baml_builtins2::reserved_package_names().contains(&alias.as_str()) {
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
            exported_names: Vec::new(),
            classes: IndexMap::new(),
            enums: IndexMap::new(),
            interfaces: IndexMap::new(),
            impl_rules: IndexMap::new(),
            functions: IndexMap::new(),
            type_aliases: IndexMap::new(),
            interface_blob: Vec::new(),
            test_init: None,
            mounted_types: IndexMap::new(),
            kind: PackageKind::Session {
                runtime: Box::new(RuntimePackage {
                    objects: Box::new([]),
                    object_names: IndexMap::new(),
                    globals: Box::new([]),
                    global_names: IndexMap::new(),
                    type_values: IndexMap::new(),
                    diagnostics: Vec::new(),
                    dependencies: dependencies.values().copied().collect(),
                    dependency_names: dependencies,
                    init: None,
                    // Session cells intentionally stay mutable between evals.
                    initialized: false,
                }),
                state: Box::new(SessionState {
                    history: IndexMap::new(),
                    visible: IndexMap::new(),
                    busy: Arc::new(AtomicBool::new(false)),
                    submission_counter: 0,
                }),
            },
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
        let mut artifact = match take_compile_artifact(
            vm,
            *artifact,
            "Session._finish received an invalid artifact",
            "Session artifact has already been consumed",
        ) {
            Ok(artifact) => artifact,
            Err(error) => return error.into(),
        };
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
            Object::Package(package) => package
                .runtime()
                .map(|runtime| runtime.diagnostics.clone())
                .unwrap_or_default(),
            _ => Vec::new(),
        };
        diagnostics
            .iter()
            .map(|diagnostic| diagnostic_value(vm, diagnostic))
            .collect()
    }
}

fn ty_never() -> RealizedTy {
    RealizedTy::Never {
        attr: TyAttr::default(),
    }
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
    RealizedTy::Class(head, vec![], TyAttr::default())
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
        attr: TyAttr::default(),
    }
}

/// A value's reconstructed type, `unknown` when it has none (a bound method, an
/// opaque handle). A future reconstructs to the `Future<T, E>` it was spawned at.
fn value_realized_ty(vm: &BexVm, value: Value) -> RealizedTy {
    vm.value_concrete_ty(value)
        .map_or_else(RealizedTy::unknown, RealizedTy::from)
}

/// Whether `value` fits the parameter type `expected`, by the canonical
/// algebra over the runtime context. Fails OPEN when the value's type cannot
/// be reconstructed — an opaque native handle (see `value_concrete_ty`) has no
/// BAML type to compare against, and refusing what we cannot check would
/// reject working calls; the callee remains dynamically safe either way
/// (values stay tagged).
fn value_fits(vm: &BexVm, value: Value, expected: &RealizedTy) -> bool {
    let Some(actual) = vm.value_concrete_ty(value) else {
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
            RealizedTy::Float { .. } => true,
            RealizedTy::Union(members, _) => {
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
        if matches!(self.expected, RealizedTy::Unknown { .. }) {
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
