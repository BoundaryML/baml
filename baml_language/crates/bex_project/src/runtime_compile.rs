//! Concrete runtime compiler assembled above the engine/compiler dependency
//! boundary.

use std::{
    collections::{BTreeMap, HashSet},
    fmt::Write as _,
    path::{Path, PathBuf},
    sync::Arc,
};

use baml_base::Name;
use baml_compiler_diagnostics::{DiagnosticId, Severity};
use baml_compiler_lexer::{TokenKind, lex_lossless};
use baml_compiler_syntax::{BlockElement, BlockExpr, SyntaxKind, SyntaxNode};
use baml_compiler2_emit::emit_units_with_stdlib;
use baml_compiler2_hir::{
    body::{BodyOwnerId, LetBody, let_body},
    contributions::Definition,
    package::PackageId,
};
use baml_compiler2_hir_ty::package_interface::package_interface;
use baml_db::{ProjectDatabase, SourceRootSpec, collect_diagnostics};
use bex_engine::RuntimeCompiler;
use bex_vm_types::{
    InitTail, RuntimeCompileArtifact, RuntimeCompileDiagnostic, RuntimeCompileMode,
    RuntimeCompileRequest, RuntimeDiagnosticSeverity, RuntimePackageMount,
    RuntimeSessionCompileArtifact, RuntimeSessionCompileRequest, RuntimeSessionInitializer,
    RuntimeSessionStep, RuntimeSessionStepKind, RuntimeSourceSpan, SessionVisibleKind,
    SessionVisibleSymbol,
    bytecode::Instruction,
    relink::{IndexOperand, visit_object_operands},
};
use indexmap::IndexMap;
use rowan::ast::AstNode;

type RuntimeLinkStub = (Vec<Name>, Name, String);
type EnrichedRuntimeMount = (Vec<u8>, Vec<RuntimeLinkStub>);

#[derive(Default)]
struct MountedDeclarationDocs {
    declaration: Option<String>,
    members: IndexMap<Name, Option<String>>,
}

const RUNTIME_VIRTUAL_ROOT: &str = "<runtime>";
const BUILTIN_VIRTUAL_ROOT: &str = "<builtin>";

/// Construct a compiler virtual path without consulting the host OS separator.
///
/// Runtime file names are identifiers in the compiler's slash-oriented path
/// domain, not native filesystem paths. Normalizing backslashes also keeps
/// requests produced on Windows stable when they cross a process boundary.
fn runtime_source_virtual_path(path: &str) -> PathBuf {
    let path = path.replace('\\', "/");
    PathBuf::from(format!(
        "{RUNTIME_VIRTUAL_ROOT}/{}",
        path.trim_start_matches('/')
    ))
}

/// Construct the virtual source path for a link stub in a mounted package.
fn runtime_mount_virtual_path(
    alias: &str,
    namespace: &[Name],
    mount_index: usize,
    stub_index: usize,
) -> PathBuf {
    let mut path = format!("{BUILTIN_VIRTUAL_ROOT}/{alias}");
    for segment in namespace {
        write!(&mut path, "/ns_{}", segment.as_str()).expect("writing to String is infallible");
    }
    write!(&mut path, "/runtime_mount_{mount_index}_{stub_index}.baml")
        .expect("writing to String is infallible");
    PathBuf::from(path)
}

/// Hide the synthetic runtime root in diagnostics using virtual-path rules.
fn runtime_relative_virtual_path(path: &Path) -> String {
    let normalized = path.to_string_lossy().replace('\\', "/");
    normalized
        .strip_prefix(RUNTIME_VIRTUAL_ROOT)
        .map(|path| path.strip_prefix('/').unwrap_or(path))
        .unwrap_or(&normalized)
        .to_string()
}

/// The packages a synthesized link-stub file may spell types from: the
/// stdlib set, the consumer's own (`user`) package, and the mount aliases of
/// this compile request.
///
/// A nominal reference to any other package came from a different compile
/// world — a mount alias of the dependency's own compile, say — and its
/// spelling means nothing here. Rendering it into a stub would produce
/// diagnostics in a phantom `runtime_mount_*` file, so such types widen to
/// `unknown` and mounted inference owns the real type.
struct StubViewpoint<'a> {
    aliases: &'a [Name],
}

impl StubViewpoint<'_> {
    fn spellable_package(&self, name: &baml_type::QualifiedTypeName) -> bool {
        name.is_local()
            || baml_builtins2::stdlib_package_names().contains(&name.package().as_str())
            || self.aliases.contains(name.package())
    }

    fn hides_interface(&self, interface: &baml_type::Interface) -> bool {
        !self.spellable_package(&interface.name)
            || interface.generics.iter().any(|ty| self.hides_type(ty))
            || interface
                .associated_types
                .iter()
                .any(|(_, ty)| self.hides_type(ty))
    }

    /// Whether rendering `ty` as source would spell a package this compile
    /// world cannot resolve. Unspellable references can occur below otherwise
    /// source-spellable containers, function types, or interface constraints,
    /// so this inspects the complete type rather than only its outer nominal
    /// reference.
    fn hides_type(&self, ty: &baml_type::Ty) -> bool {
        use baml_type::Ty;

        match ty {
            Ty::Class(name, generics, _) => {
                !self.spellable_package(name) || generics.iter().any(|ty| self.hides_type(ty))
            }
            Ty::Interface(name, generics, associated_types, _) => {
                !self.spellable_package(name)
                    || generics.iter().any(|ty| self.hides_type(ty))
                    || associated_types.iter().any(|(_, ty)| self.hides_type(ty))
            }
            Ty::Enum(name, _) | Ty::EnumVariant(name, ..) | Ty::TypeAlias(name, _) => {
                !self.spellable_package(name)
            }
            Ty::List(inner, _) => self.hides_type(inner),
            Ty::Map { key, value, .. } => self.hides_type(key) || self.hides_type(value),
            Ty::Union(members, _) => members.iter().any(|ty| self.hides_type(ty)),
            Ty::Function {
                params,
                ret,
                throws,
                ..
            } => {
                params.iter().any(|param| self.hides_type(&param.ty))
                    || self.hides_type(ret)
                    || self.hides_type(throws)
            }
            Ty::Future(value, throws, _) => self.hides_type(value) || self.hides_type(throws),
            Ty::AssociatedTypeProjection {
                base, interface, ..
            } => self.hides_type(base) || self.hides_interface(interface),
            Ty::Int { .. }
            | Ty::Bigint { .. }
            | Ty::Float { .. }
            | Ty::String { .. }
            | Ty::Bool { .. }
            | Ty::Null { .. }
            | Ty::Uint8Array { .. }
            | Ty::Media(..)
            | Ty::Literal(..)
            | Ty::RustType { .. }
            | Ty::Type { .. }
            | Ty::Resource { .. }
            | Ty::PromptAst { .. }
            | Ty::Void { .. }
            | Ty::TypeVar(..)
            | Ty::Unknown { .. }
            | Ty::Never { .. }
            | Ty::Error { .. } => false,
        }
    }
}

fn enrich_runtime_mount(
    alias: &str,
    aliases: &[Name],
    mut package: RuntimePackageMount,
) -> Result<EnrichedRuntimeMount, RuntimeCompileDiagnostic> {
    use baml_compiler2_hir_ty::{
        callable::ExternalCallTarget,
        package_interface::{
            ExportedFieldAttrs, ExportedFunction, ExportedImpl, ExportedImplOrigin, ExportedType,
            PackageInterface,
        },
    };

    fn source_identifier(name: &Name) -> bool {
        let mut chars = name.as_str().chars();
        chars
            .next()
            .is_some_and(|ch| ch.is_ascii_alphabetic() || ch == '_')
            && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
    }

    fn relocated_name(
        name: &baml_type::QualifiedTypeName,
        alias: &Name,
    ) -> baml_type::QualifiedTypeName {
        if name.is_local() {
            baml_type::QualifiedTypeName::new(
                alias.clone(),
                name.namespace().clone(),
                name.name().clone(),
            )
        } else {
            name.clone()
        }
    }

    fn relocate_ty(ty: &mut baml_type::Ty, alias: &Name) {
        *ty = ty.map_heads(&mut |name| relocated_name(name, alias));
    }

    fn relocate_interface(interface: &mut baml_type::Interface, alias: &Name) {
        *interface = interface.map_heads(&mut |name| relocated_name(name, alias));
    }

    fn relocate_bounds(bounds: &mut [Vec<baml_type::Interface>], alias: &Name) {
        for interface in bounds.iter_mut().flatten() {
            relocate_interface(interface, alias);
        }
    }

    fn write_docstring(source: &mut String, docstring: Option<&str>, indent: &str) {
        let Some(docstring) = docstring.map(str::trim).filter(|docs| !docs.is_empty()) else {
            return;
        };
        for line in docstring.lines() {
            writeln!(source, "{indent}/// {line}").expect("writing to String is infallible");
        }
    }

    fn relocate_function(function: &mut ExportedFunction, alias: &Name) {
        for param in &mut function.params {
            *param = param.map_heads(&mut |name| relocated_name(name, alias));
        }
        relocate_ty(&mut function.return_type, alias);
        relocate_ty(&mut function.callable_throws, alias);
        relocate_bounds(&mut function.generic_param_bounds, alias);

        match &mut function.target {
            ExternalCallTarget::Free { package, .. }
            | ExternalCallTarget::Method { package, .. } => {
                *package = alias.clone();
            }
            ExternalCallTarget::Interface { interface, .. } => {
                *interface = baml_type::QualifiedTypeName::new(
                    alias.clone(),
                    interface.namespace().clone(),
                    interface.name().clone(),
                );
            }
        }
    }

    /// Whether `function` is an authored, linkable declaration a source stub
    /// may spell. Compiler-generated init/test helpers and callable
    /// companions are not authored declarations: the mounted interface
    /// already exports their exact callable identities; emitting either
    /// spelling as a source stub is invalid (`$init`) or would redeclare the
    /// authored function (`Extract@spec` is postfix syntax, not a declaration
    /// identifier).
    fn stubbable(function: &ExportedFunction) -> bool {
        use baml_compiler2_hir_ty::callable::ExternalLinkability;

        matches!(function.linkability, ExternalLinkability::Linkable)
            && source_identifier(&function.name)
    }

    fn stub_type(ty: &baml_type::Ty, viewpoint: &StubViewpoint<'_>) -> String {
        // Hide a type only when its source spelling would name a package this
        // compile world cannot resolve and so produce diagnostics in a
        // phantom `runtime_mount_*` file.
        if viewpoint.hides_type(ty) {
            "unknown".to_string()
        } else {
            ty.to_string()
        }
    }

    /// `<T extends A & B, U>` for the function's own generic parameters, or
    /// the empty string. Bounds are spelled only when `spell_bounds` (a bound
    /// this world cannot name is dropped rather than widened).
    fn stub_generics(
        function: &ExportedFunction,
        viewpoint: &StubViewpoint<'_>,
        spell_bounds: bool,
    ) -> String {
        let generics = function
            .generic_params
            .iter()
            .enumerate()
            .filter(|(_, param)| !baml_type::is_synthetic_effect_param(param.name()))
            .map(|(index, param)| {
                let bounds = if spell_bounds {
                    function
                        .generic_param_bounds
                        .get(index)
                        .map(|bounds| {
                            bounds
                                .iter()
                                .filter(|bound| !viewpoint.hides_interface(bound))
                                .map(|bound| {
                                    baml_type::Ty::Interface(
                                        bound.name.clone(),
                                        bound.generics.clone(),
                                        bound.associated_types.clone(),
                                        baml_type::TyAttr::default(),
                                    )
                                    .to_string()
                                })
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default()
                } else {
                    Vec::new()
                };
                if bounds.is_empty() {
                    param.to_string()
                } else {
                    format!("{param} extends {}", bounds.join(" & "))
                }
            })
            .collect::<Vec<_>>();
        if generics.is_empty() {
            String::new()
        } else {
            format!("<{}>", generics.join(", "))
        }
    }

    /// The link stub for a free function (or, when its owner has no source
    /// stub, a method), spelled as a free `$rust_function` under the
    /// namespace its call target names. The stub preserves only the callable
    /// identity the emitter slots and the may-throw ABI bit: parameters are
    /// `unknown`, and named error types retain the dependency package's
    /// nominal identity in the mounted interface without necessarily being
    /// source-spellable from this synthetic alias package.
    fn free_function_stub(
        function: &ExportedFunction,
        viewpoint: &StubViewpoint<'_>,
    ) -> Option<(Vec<Name>, Name, String)> {
        if !stubbable(function) {
            return None;
        }
        let name = function.name.clone();
        let namespace = match &function.target {
            ExternalCallTarget::Free { namespace, .. } => namespace.clone(),
            ExternalCallTarget::Method {
                namespace, class, ..
            } => {
                let mut namespace = namespace.clone();
                namespace.push(class.clone());
                namespace
            }
            ExternalCallTarget::Interface { interface, .. } => {
                let mut namespace = interface.namespace().clone();
                namespace.push(interface.name().clone());
                namespace
            }
        };
        let generics = stub_generics(function, viewpoint, false);
        let params = function
            .params
            .iter()
            .enumerate()
            .map(|(index, param)| {
                let param_name = param
                    .name
                    .as_ref()
                    .filter(|name| name.as_str() != "self")
                    .map_or_else(|| format!("arg{index}"), ToString::to_string);
                let default = if param.is_optional() { " = null" } else { "" };
                format!("{param_name}: unknown{default}")
            })
            .collect::<Vec<_>>()
            .join(", ");
        let throws = if matches!(
            function.callable_throws,
            baml_type::Ty::Never { .. } | baml_type::Ty::Void { .. }
        ) {
            String::new()
        } else {
            " throws unknown".to_string()
        };
        // Mounted inference owns the real return type.
        let return_type = stub_type(&function.return_type, viewpoint);
        let source = format!(
            "function {name}{generics}({params}) -> {return_type}{throws} {{ $rust_function }}\n"
        );
        Some((namespace, name, source))
    }

    /// How a method stub is spelled inside its owner's stub body.
    #[derive(Clone, Copy)]
    enum MethodStubKind {
        /// A class-inherent method: a `$rust_function` body the emitter slots
        /// under the class-qualified name the runtime linker resolves.
        ClassMethod,
        /// An interface required method: signature only.
        InterfaceRequired,
        /// An interface default method: a `$rust_function` body, so a
        /// consumer implementor that does not override it links to the
        /// dependency's body.
        InterfaceDefault,
    }

    /// The in-body stub line for a method of a mounted class or interface.
    ///
    /// Unlike a free link stub this spells the real signature (parameters,
    /// bounds, declared throws) wherever this world can name it: source-backed
    /// lookup wins before the mounted interface in HIR, so this stub is the
    /// method the type checker sees — a consumer `implement` block is checked
    /// for conformance against it, and a call on a mounted value is typed by
    /// it.
    fn method_stub(
        function: &ExportedFunction,
        viewpoint: &StubViewpoint<'_>,
        kind: MethodStubKind,
    ) -> Option<String> {
        if !stubbable(function) {
            return None;
        }
        let name = &function.name;
        let generics = stub_generics(function, viewpoint, true);
        let params = function
            .params
            .iter()
            .enumerate()
            .map(|(index, param)| {
                if index == 0
                    && param
                        .name
                        .as_ref()
                        .is_some_and(|name| name.as_str() == "self")
                {
                    return "self".to_string();
                }
                let param_name = param
                    .name
                    .as_ref()
                    .filter(|name| source_identifier(name))
                    .map_or_else(|| format!("arg{index}"), ToString::to_string);
                let ty = stub_type(&param.ty, viewpoint);
                let default = if param.is_optional() { " = null" } else { "" };
                format!("{param_name}: {ty}{default}")
            })
            .collect::<Vec<_>>()
            .join(", ");
        let return_type = stub_type(&function.return_type, viewpoint);
        // `callable_throws` is the one effective contract (declared when
        // written, inferred otherwise) - the stub spells it exactly, ALWAYS:
        // a required interface method's signature and a `$rust_function`
        // body both demand an explicit clause, and `throws never` is that
        // clause when nothing throws. A type this world cannot name
        // degrades per-slot through `stub_type` rather than widening the
        // whole clause to `unknown`.
        let throws = format!(
            " throws {}",
            stub_type(&function.callable_throws, viewpoint)
        );
        let body = match kind {
            MethodStubKind::InterfaceRequired => "",
            MethodStubKind::ClassMethod | MethodStubKind::InterfaceDefault => " { $rust_function }",
        };
        Some(format!(
            "  function {name}{generics}({params}) -> {return_type}{throws}{body}\n"
        ))
    }

    let mut interface = baml_artifact::decode::<PackageInterface>(
        baml_artifact::ArtifactKind::PackageInterface,
        &package.interface_blob,
    )
    .map_err(|error| RuntimeCompileDiagnostic {
        code: "E_RUNTIME_INTERFACE".to_string(),
        message: error.to_string(),
        severity: RuntimeDiagnosticSeverity::Error,
        span: None,
    })?;
    // A package object may be mounted under any source-visible alias. Its
    // exported call targets retain the package's original identity in the
    // persisted interface, so relocate those symbolic link names to the alias.
    // Runtime linking resolves the alias back to the live dependency object.
    let alias = Name::new(alias);
    let viewpoint = StubViewpoint { aliases };
    let mut stubs = Vec::new();
    for throw_set in interface
        .throw_sets
        .direct
        .values_mut()
        .chain(interface.throw_sets.transitive.values_mut())
    {
        *throw_set = std::mem::take(throw_set)
            .into_iter()
            .map(|mut ty| {
                relocate_ty(&mut ty, &alias);
                ty
            })
            .collect();
    }
    for function in interface
        .functions
        .values_mut()
        .flat_map(|namespace| namespace.values_mut())
    {
        relocate_function(function, &alias);
        stubs.extend(free_function_stub(function, &viewpoint));
    }
    for (export_namespace, exported_types) in &mut interface.types {
        for (export_name, exported) in exported_types {
            match exported {
                ExportedType::Class {
                    qtn,
                    fields,
                    methods,
                    generic_params,
                    generic_param_bounds,
                } => {
                    *qtn = relocated_name(qtn, &alias);
                    for (_, ty, _) in fields.iter_mut() {
                        relocate_ty(ty, &alias);
                    }
                    relocate_bounds(generic_param_bounds, &alias);
                    for function in methods.iter_mut() {
                        relocate_function(function, &alias);
                    }
                    // The emitter needs a concrete class object in the mounted
                    // package's discarded source units so a consumer literal
                    // decomposes to an external object reference. At runtime
                    // that reference is resolved to the dependency's actual
                    // class object, preserving a mounted runtime mint instead
                    // of allocating a structurally-similar local class.
                    let class_stub = source_identifier(export_name)
                        && export_namespace.iter().all(source_identifier)
                        && fields.iter().all(|(name, ..)| source_identifier(name));
                    if class_stub {
                        let generics = generic_params
                            .iter()
                            .filter(|param| !baml_type::is_synthetic_effect_param(param.name()))
                            .map(ToString::to_string)
                            .collect::<Vec<_>>();
                        let generic_suffix = if generics.is_empty() {
                            String::new()
                        } else {
                            format!("<{}>", generics.join(", "))
                        };
                        let mut source = format!("class {export_name}{generic_suffix} {{\n");
                        for (field, ty, attrs) in fields.iter() {
                            // Source-backed lookup wins before the mounted
                            // interface in HIR. Preserve every spellable field
                            // type here so nested projections see the same ABI;
                            // only genuinely hidden package names degrade to
                            // `unknown` in the link-only source.
                            let ty = if viewpoint.hides_type(ty) {
                                "unknown".to_string()
                            } else {
                                ty.to_string()
                            };
                            write_docstring(&mut source, attrs.docstring.as_deref(), "  ");
                            writeln!(&mut source, "  {field} {ty}")
                                .expect("writing to String is infallible");
                        }
                        // Inherent methods live in the class body: the stub is
                        // the class the type checker sees, so this is where a
                        // consumer's `value.method()` finds them, and the
                        // emitter slots each under the same class-qualified
                        // name a free stub in a `ns_<Class>/` namespace would
                        // take — without that namespace shadowing the class.
                        for method in methods.iter() {
                            if let ExternalCallTarget::Method { .. } = method.target
                                && let Some(stub) =
                                    method_stub(method, &viewpoint, MethodStubKind::ClassMethod)
                            {
                                source.push_str(&stub);
                            }
                        }
                        source.push_str("}\n");
                        stubs.push((export_namespace.clone(), export_name.clone(), source));
                    } else {
                        for method in methods.iter() {
                            if let ExternalCallTarget::Method { .. } = method.target {
                                stubs.extend(free_function_stub(method, &viewpoint));
                            }
                        }
                    }
                }
                ExportedType::Interface {
                    qtn,
                    generic_params,
                    param_bounds,
                    requires,
                    associated_types,
                    fields,
                    required_methods,
                    default_methods,
                    ..
                } => {
                    relocate_bounds(param_bounds, &alias);
                    for interface in requires {
                        relocate_interface(interface, &alias);
                    }
                    for associated in associated_types.iter_mut() {
                        if let Some(bound) = &mut associated.bound {
                            relocate_interface(bound, &alias);
                        }
                        if let Some(default) = &mut associated.default {
                            relocate_ty(default, &alias);
                        }
                    }
                    for (_, ty, _) in fields.iter_mut() {
                        relocate_ty(ty, &alias);
                    }
                    for function in required_methods
                        .iter_mut()
                        .chain(default_methods.iter_mut())
                    {
                        relocate_function(function, &alias);
                    }
                    let namespace = qtn.namespace().clone();
                    let name = qtn.name().clone();
                    *qtn = baml_type::QualifiedTypeName::new(
                        alias.clone(),
                        namespace.clone(),
                        name.clone(),
                    );
                    let generics = generic_params
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>();
                    let generic_suffix = if generics.is_empty() {
                        String::new()
                    } else {
                        format!("<{}>", generics.join(", "))
                    };
                    let mut source = format!("interface {name}{generic_suffix} {{\n");
                    for associated in associated_types {
                        writeln!(&mut source, "  type {}", associated.name)
                            .expect("writing to String is infallible");
                    }
                    for (field, ty, attrs) in fields {
                        // Keep ordinary source-spellable ABI intact, but avoid
                        // spelling a package this world cannot resolve from any
                        // nested type.
                        let ty = if viewpoint.hides_type(ty) {
                            "unknown".to_string()
                        } else {
                            ty.to_string()
                        };
                        write_docstring(&mut source, attrs.docstring.as_deref(), "  ");
                        writeln!(&mut source, "  {field}: {ty}")
                            .expect("writing to String is infallible");
                    }
                    // Methods live in the interface body: a consumer
                    // `implement` block is checked against this stub, so it
                    // must declare every required method, and a default body
                    // is slotted under the interface-qualified name the
                    // runtime linker resolves to the dependency's body.
                    for method in required_methods.iter() {
                        if let Some(stub) =
                            method_stub(method, &viewpoint, MethodStubKind::InterfaceRequired)
                        {
                            source.push_str(&stub);
                        }
                    }
                    for method in default_methods.iter() {
                        if let Some(stub) =
                            method_stub(method, &viewpoint, MethodStubKind::InterfaceDefault)
                        {
                            source.push_str(&stub);
                        }
                    }
                    source.push_str("}\n");
                    stubs.push((namespace, name, source));
                }
                ExportedType::Enum { qtn, variants } => {
                    *qtn = relocated_name(qtn, &alias);
                    if source_identifier(export_name)
                        && export_namespace.iter().all(source_identifier)
                        && variants.iter().all(source_identifier)
                    {
                        let mut source = format!("enum {export_name} {{\n");
                        for variant in variants {
                            writeln!(&mut source, "  {variant}")
                                .expect("writing to String is infallible");
                        }
                        source.push_str("}\n");
                        stubs.push((export_namespace.clone(), export_name.clone(), source));
                    }
                }
                ExportedType::TypeAlias { qtn, resolved } => {
                    *qtn = relocated_name(qtn, &alias);
                    relocate_ty(resolved, &alias);
                }
            }
        }
    }
    for implementation in &mut interface.impls {
        relocate_interface(&mut implementation.interface, &alias);
        relocate_ty(&mut implementation.for_ty_pattern, &alias);
        relocate_bounds(&mut implementation.param_bounds, &alias);
        for (_, ty) in &mut implementation.associated_types {
            relocate_ty(ty, &alias);
        }
        if let ExportedImplOrigin::InBodyClass { class_qtn } = &mut implementation.origin {
            *class_qtn = relocated_name(class_qtn, &alias);
        }
        // Implementation methods are reached by interface dispatch, which the
        // consumer lowers as a virtual call on the receiver's runtime class:
        // no source stub is needed, and a free stub under `ns_<Interface>/`
        // would shadow the interface's own stub.
        for function in &mut implementation.methods {
            relocate_function(function, &alias);
        }
    }
    // Mounted runtime declarations are spelled `alias.<item name>` in this
    // compile world: the mount surface is the only channel that names them
    // here. Each reached declaration gets one row under its item name, and the
    // row's qtn — the nominal identity every reference lowers to — is that
    // spelling. Two mounts reaching the same declaration agree on the row; two
    // distinct declarations sharing an item name are a fail-closed error (the
    // live tag, carried for exactly this check, tells them apart).
    let mut minted_tags: IndexMap<Name, baml_type::typetag::TypeTag> = IndexMap::new();
    let mut minted_rows: IndexMap<Name, ExportedType> = IndexMap::new();
    let mut minted_docs: IndexMap<Name, MountedDeclarationDocs> = IndexMap::new();
    let duplicate_minted = |name: &Name| RuntimeCompileDiagnostic {
        code: "E0011".to_string(),
        message: format!(
            "two distinct mounted declarations under `{alias}` share the name `{name}`"
        ),
        severity: RuntimeDiagnosticSeverity::Error,
        span: None,
    };
    for mount in &package.types {
        for class in &mount.classes {
            if let Some(&seen) = minted_tags.get(&class.name) {
                if seen != class.tag {
                    return Err(duplicate_minted(&class.name));
                }
                continue;
            }
            minted_tags.insert(class.name.clone(), class.tag);
            minted_docs.insert(
                class.name.clone(),
                MountedDeclarationDocs {
                    declaration: class.docstring.clone(),
                    members: IndexMap::new(),
                },
            );
            minted_rows.insert(
                class.name.clone(),
                ExportedType::Class {
                    qtn: baml_type::QualifiedTypeName::new(
                        alias.clone(),
                        Vec::new(),
                        class.name.clone(),
                    ),
                    fields: class
                        .fields
                        .iter()
                        .map(|(name, ty, attrs)| {
                            (
                                name.clone(),
                                ty.clone(),
                                ExportedFieldAttrs {
                                    alias: attrs.alias.clone(),
                                    description: attrs.description.clone(),
                                    docstring: attrs.docstring.clone(),
                                },
                            )
                        })
                        .collect(),
                    methods: Vec::new(),
                    generic_params: Vec::new(),
                    generic_param_bounds: Vec::new(),
                },
            );
        }
        for enm in &mount.enums {
            if let Some(&seen) = minted_tags.get(&enm.name) {
                if seen != enm.tag {
                    return Err(duplicate_minted(&enm.name));
                }
                continue;
            }
            minted_tags.insert(enm.name.clone(), enm.tag);
            minted_docs.insert(
                enm.name.clone(),
                MountedDeclarationDocs {
                    declaration: enm.docstring.clone(),
                    members: enm
                        .variants
                        .iter()
                        .map(|(name, attrs)| (name.clone(), attrs.docstring.clone()))
                        .collect(),
                },
            );
            minted_rows.insert(
                enm.name.clone(),
                ExportedType::Enum {
                    qtn: baml_type::QualifiedTypeName::new(
                        alias.clone(),
                        Vec::new(),
                        enm.name.clone(),
                    ),
                    variants: enm.variants.iter().map(|(name, _)| name.clone()).collect(),
                },
            );
        }
    }
    // Item rows join the package's root export namespace, so a mounted name
    // colliding with a declared export is exactly as fatal as two mounts
    // colliding with each other.
    if !minted_rows.is_empty() {
        interface.namespaces.insert(Vec::new());
        let root_types = interface.types.entry(Vec::new()).or_default();
        for (name, row) in &minted_rows {
            if root_types.contains_key(name) {
                return Err(duplicate_minted(name));
            }
            root_types.insert(name.clone(), row.clone());
        }
    }
    // Every mounted row gets a source link stub so a consumer literal can
    // decompose to an external object reference; runtime linking resolves it
    // to the live declaration. Source-backed lookup wins before the mounted
    // interface in HIR, so preserve spellable field types here as well.
    for (name, row) in &minted_rows {
        match row {
            ExportedType::Class { fields, .. }
                if source_identifier(name)
                    && fields.iter().all(|(field, ..)| source_identifier(field)) =>
            {
                let mut source = String::new();
                write_docstring(
                    &mut source,
                    minted_docs
                        .get(name)
                        .and_then(|docs| docs.declaration.as_deref()),
                    "",
                );
                writeln!(&mut source, "class {name} {{").expect("writing to String is infallible");
                for (field, ty, attrs) in fields {
                    let ty = if viewpoint.hides_type(ty) {
                        "unknown".to_string()
                    } else {
                        ty.to_string()
                    };
                    write_docstring(&mut source, attrs.docstring.as_deref(), "  ");
                    writeln!(&mut source, "  {field} {ty}")
                        .expect("writing to String is infallible");
                }
                source.push_str("}\n");
                stubs.push((Vec::new(), name.clone(), source));
            }
            ExportedType::Enum { variants, .. }
                if source_identifier(name) && variants.iter().all(source_identifier) =>
            {
                let mut source = String::new();
                let docs = minted_docs.get(name);
                write_docstring(
                    &mut source,
                    docs.and_then(|docs| docs.declaration.as_deref()),
                    "",
                );
                writeln!(&mut source, "enum {name} {{").expect("writing to String is infallible");
                for variant in variants {
                    write_docstring(
                        &mut source,
                        docs.and_then(|docs| docs.members.get(variant))
                            .and_then(|docs| docs.as_deref()),
                        "  ",
                    );
                    writeln!(&mut source, "  {variant}").expect("writing to String is infallible");
                }
                source.push_str("}\n");
                stubs.push((Vec::new(), name.clone(), source));
            }
            ExportedType::Class { .. }
            | ExportedType::Enum { .. }
            | ExportedType::Interface { .. }
            | ExportedType::TypeAlias { .. } => {}
        }
    }
    for mount in package.types.drain(..) {
        let root_ty = baml_type::Ty::from(&mount.ty);
        // The export-name row: an additional spelling of the root declaration
        // (or, for a structural type, an alias row of its own). It carries the
        // same qtn as the item row, so both spellings lower to one identity.
        let exported = match &mount.ty {
            baml_type::RealizedTy::Class(qtn, _, _) | baml_type::RealizedTy::Enum(qtn, _) => {
                minted_rows.get(qtn.name()).cloned()
            }
            _ => Some(ExportedType::TypeAlias {
                qtn: baml_type::QualifiedTypeName::new(
                    alias.clone(),
                    Vec::new(),
                    mount.export_name.clone(),
                ),
                resolved: root_ty.clone(),
            }),
        }
        .ok_or_else(|| RuntimeCompileDiagnostic {
            code: "E_RUNTIME_INTERFACE".to_string(),
            message: format!(
                "runtime type `{}` has no structural definition",
                mount.export_name
            ),
            severity: RuntimeDiagnosticSeverity::Error,
            span: None,
        })?;
        // When the export name is the root declaration's own item name, the
        // item row already spells it with the same identity.
        let export_is_root_item = matches!(
            &mount.ty,
            baml_type::RealizedTy::Class(qtn, _, _) | baml_type::RealizedTy::Enum(qtn, _)
                if qtn.name() == &mount.export_name
        );
        let root_types = interface.types.entry(Vec::new()).or_default();
        match root_types.get(&mount.export_name) {
            Some(_) if export_is_root_item => {}
            Some(_) => {
                return Err(RuntimeCompileDiagnostic {
                    code: "E0011".to_string(),
                    message: format!("duplicate exported type name `{}`", mount.export_name),
                    severity: RuntimeDiagnosticSeverity::Error,
                    span: None,
                });
            }
            None => {
                match &exported {
                    ExportedType::Class { qtn, fields, .. }
                        if source_identifier(&mount.export_name)
                            && fields.iter().all(|(name, ..)| source_identifier(name)) =>
                    {
                        let mut source = String::new();
                        write_docstring(
                            &mut source,
                            minted_docs
                                .get(qtn.name())
                                .and_then(|docs| docs.declaration.as_deref()),
                            "",
                        );
                        writeln!(&mut source, "class {} {{", mount.export_name)
                            .expect("writing to String is infallible");
                        for (field, ty, attrs) in fields {
                            write_docstring(&mut source, attrs.docstring.as_deref(), "  ");
                            let ty = if viewpoint.hides_type(ty) {
                                "unknown".to_string()
                            } else {
                                ty.to_string()
                            };
                            writeln!(&mut source, "  {field} {ty}")
                                .expect("writing to String is infallible");
                        }
                        source.push_str("}\n");
                        stubs.push((Vec::new(), mount.export_name.clone(), source));
                    }
                    ExportedType::Enum { qtn, variants }
                        if source_identifier(&mount.export_name)
                            && variants.iter().all(source_identifier) =>
                    {
                        let mut source = String::new();
                        let docs = minted_docs.get(qtn.name());
                        write_docstring(
                            &mut source,
                            docs.and_then(|docs| docs.declaration.as_deref()),
                            "",
                        );
                        writeln!(&mut source, "enum {} {{", mount.export_name)
                            .expect("writing to String is infallible");
                        for variant in variants {
                            write_docstring(
                                &mut source,
                                docs.and_then(|docs| docs.members.get(variant))
                                    .and_then(|docs| docs.as_deref()),
                                "  ",
                            );
                            writeln!(&mut source, "  {variant}")
                                .expect("writing to String is infallible");
                        }
                        source.push_str("}\n");
                        stubs.push((Vec::new(), mount.export_name.clone(), source));
                    }
                    ExportedType::Class { .. }
                    | ExportedType::Enum { .. }
                    | ExportedType::Interface { .. }
                    | ExportedType::TypeAlias { .. } => {}
                }
                interface.namespaces.insert(Vec::new());
                root_types.insert(mount.export_name.clone(), exported);
            }
        }

        for (witness, field_links) in mount.witnesses {
            interface.impls.push(ExportedImpl {
                interface: witness.clone(),
                for_ty_pattern: root_ty.clone(),
                generic_params: Vec::new(),
                param_bounds: Vec::new(),
                associated_types: witness.associated_types.to_vec(),
                field_links,
                origin: ExportedImplOrigin::OutOfBody,
                methods: Vec::new(),
            });
        }
    }
    stubs.sort();
    stubs.dedup();
    baml_artifact::encode(baml_artifact::ArtifactKind::PackageInterface, &interface)
        .map(|blob| (blob, stubs))
        .map_err(|error| RuntimeCompileDiagnostic {
            code: "E_RUNTIME_INTERFACE".to_string(),
            message: error.to_string(),
            severity: RuntimeDiagnosticSeverity::Error,
            span: None,
        })
}

/// Stateless compiler provider. A fresh database is allocated inside every
/// [`RuntimeCompiler::compile`] call and dropped before the call returns.
#[derive(Debug, Default)]
struct ProjectRuntimeCompiler;

pub(crate) fn runtime_compiler() -> Arc<dyn RuntimeCompiler> {
    Arc::new(ProjectRuntimeCompiler)
}

fn owned_diagnostic(
    db: &ProjectDatabase,
    diagnostic: &baml_compiler_diagnostics::Diagnostic,
) -> RuntimeCompileDiagnostic {
    let span = diagnostic.primary_span().and_then(|span| {
        db.file_id_to_path(span.file_id)
            .map(|path| RuntimeSourceSpan {
                file: runtime_relative_virtual_path(path),
                start: usize::from(span.range.start()),
                end: usize::from(span.range.end()),
            })
    });
    RuntimeCompileDiagnostic {
        code: diagnostic.code().to_string(),
        message: diagnostic.message_with_primary_label().into_owned(),
        severity: match diagnostic.severity {
            Severity::Error => RuntimeDiagnosticSeverity::Error,
            Severity::Warning => RuntimeDiagnosticSeverity::Warning,
            Severity::Info => RuntimeDiagnosticSeverity::Info,
        },
        span,
    }
}

fn runtime_diagnostic(
    code: DiagnosticId,
    file: &str,
    start: usize,
    end: usize,
    message: impl Into<String>,
) -> RuntimeCompileDiagnostic {
    RuntimeCompileDiagnostic {
        code: code.code().to_string(),
        message: message.into(),
        severity: RuntimeDiagnosticSeverity::Error,
        span: Some(RuntimeSourceSpan {
            file: file.to_string(),
            start,
            end,
        }),
    }
}

fn byte_range(node: &SyntaxNode) -> std::ops::Range<usize> {
    let range = node.text_range();
    usize::from(range.start())..usize::from(range.end())
}

fn declaration_kind(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::FUNCTION_DEF
            | SyntaxKind::CLASS_DEF
            | SyntaxKind::ENUM_DEF
            | SyntaxKind::INTERFACE_DEF
            | SyntaxKind::CLIENT_VALUE_DEF
            | SyntaxKind::CLIENT_DEF
            | SyntaxKind::GENERATOR_DEF
            | SyntaxKind::RETRY_POLICY_DEF
            | SyntaxKind::TEMPLATE_STRING_DEF
            | SyntaxKind::TYPE_ALIAS_DEF
            | SyntaxKind::IMPLEMENTS_FOR
    )
}

fn declaration_name(node: &SyntaxNode) -> Option<(String, std::ops::Range<usize>)> {
    if node.kind() == SyntaxKind::CLIENT_DEF {
        return node
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .filter(|token| token.kind() == SyntaxKind::WORD)
            .last()
            .map(|token| {
                let range = token.text_range();
                (
                    token.text().to_string(),
                    usize::from(range.start())..usize::from(range.end()),
                )
            });
    }
    let mut saw_head = false;
    for token in node
        .children_with_tokens()
        .filter_map(rowan::NodeOrToken::into_token)
    {
        if matches!(
            token.kind(),
            SyntaxKind::KW_FUNCTION
                | SyntaxKind::KW_CLASS
                | SyntaxKind::KW_ENUM
                | SyntaxKind::KW_INTERFACE
                | SyntaxKind::KW_CLIENT
                | SyntaxKind::KW_GENERATOR
                | SyntaxKind::KW_RETRY_POLICY
                | SyntaxKind::KW_TEMPLATE_STRING
                | SyntaxKind::KW_TYPE
        ) {
            saw_head = true;
            continue;
        }
        if saw_head && token.kind() == SyntaxKind::WORD {
            let range = token.text_range();
            return Some((
                token.text().to_string(),
                usize::from(range.start())..usize::from(range.end()),
            ));
        }
    }
    None
}

fn first_pattern_name(node: &SyntaxNode) -> Option<(String, std::ops::Range<usize>)> {
    let pattern = node
        .children()
        .find(|child| child.kind() == SyntaxKind::PATTERN)?;
    let token = pattern
        .descendants_with_tokens()
        .filter_map(rowan::NodeOrToken::into_token)
        .find(|token| token.kind() == SyntaxKind::WORD)?;
    let range = token.text_range();
    Some((
        token.text().to_string(),
        usize::from(range.start())..usize::from(range.end()),
    ))
}

fn expression_is_assignment(node: &SyntaxNode) -> bool {
    node.children_with_tokens()
        .filter_map(rowan::NodeOrToken::into_token)
        .any(|token| {
            matches!(
                token.kind(),
                SyntaxKind::EQUALS
                    | SyntaxKind::PLUS_EQUALS
                    | SyntaxKind::MINUS_EQUALS
                    | SyntaxKind::STAR_EQUALS
                    | SyntaxKind::SLASH_EQUALS
                    | SyntaxKind::PERCENT_EQUALS
                    | SyntaxKind::AND_EQUALS
                    | SyntaxKind::PIPE_EQUALS
                    | SyntaxKind::CARET_EQUALS
                    | SyntaxKind::LESS_LESS_EQUALS
                    | SyntaxKind::GREATER_GREATER_EQUALS
            )
        })
}

fn assignment_parts(source: &str) -> Option<(&str, &'static str, &str)> {
    let tokens = lex_lossless(source, baml_base::FileId::new(0));
    let (token, operator) = tokens.iter().find_map(|token| {
        let operator = match token.kind {
            TokenKind::Equals => "=",
            TokenKind::PlusEquals => "+",
            TokenKind::MinusEquals => "-",
            TokenKind::StarEquals => "*",
            TokenKind::SlashEquals => "/",
            TokenKind::PercentEquals => "%",
            TokenKind::AndEquals => "&",
            TokenKind::PipeEquals => "|",
            TokenKind::CaretEquals => "^",
            TokenKind::LessLessEquals => "<<",
            TokenKind::GreaterGreaterEquals => ">>",
            _ => return None,
        };
        Some((token, operator))
    })?;
    let start = usize::from(token.span.range.start());
    let end = usize::from(token.span.range.end());
    Some((source[..start].trim(), operator, source[end..].trim()))
}

/// Recognize the statement-only `type T = unreflect(expr)` form and return
/// the source-visible name plus the operand text. The parser has already
/// validated the delimiters for block statements; this small lexical helper
/// is also used to distinguish the same token shape from a top-level alias
/// during Session's initial declaration-hoisting pass.
fn runtime_type_binding_parts(source: &str) -> Option<(String, &str)> {
    let tokens = lex_lossless(source, baml_base::FileId::new(0));
    let words = tokens
        .iter()
        .enumerate()
        .filter(|(_, token)| token.kind == TokenKind::Word)
        .collect::<Vec<_>>();
    let [(_, type_word), (_, name), (unreflect_index, unreflect), ..] = words.as_slice() else {
        return None;
    };
    if type_word.text.as_str() != "type" || unreflect.text.as_str() != "unreflect" {
        return None;
    }
    let open = tokens
        .iter()
        .skip(*unreflect_index + 1)
        .find(|token| token.kind == TokenKind::LParen)?;
    let close = tokens
        .iter()
        .rev()
        .find(|token| token.kind == TokenKind::RParen)?;
    let start = usize::from(open.span.range.end());
    let end = usize::from(close.span.range.start());
    (start <= end).then(|| (name.text.clone(), source[start..end].trim()))
}

fn runtime_type_binding_prelude(bindings: &IndexMap<String, SessionVisibleSymbol>) -> String {
    let mut prelude = String::new();
    for symbol in bindings.values() {
        if let SessionVisibleKind::TypeBinding { type_value } = &symbol.kind {
            let _ = writeln!(
                prelude,
                "type {} = unreflect({type_value});",
                symbol.internal
            );
        }
    }
    prelude
}

fn locally_bound_names(
    node: &SyntaxNode,
    outer_let: Option<&std::ops::Range<usize>>,
) -> HashSet<String> {
    let mut names = HashSet::new();
    for local in node
        .descendants()
        .filter(|child| matches!(child.kind(), SyntaxKind::PARAMETER | SyntaxKind::LET_STMT))
    {
        if local.kind() == SyntaxKind::LET_STMT {
            if let Some((name, range)) = first_pattern_name(&local)
                && outer_let != Some(&range)
            {
                names.insert(name);
            }
        } else if let Some(token) = local
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .find(|token| token.kind() == SyntaxKind::WORD)
        {
            names.insert(token.text().to_string());
        }
    }
    names
}

fn structural_name_ranges(node: &SyntaxNode) -> HashSet<std::ops::Range<usize>> {
    node.descendants()
        .filter(|child| matches!(child.kind(), SyntaxKind::FIELD | SyntaxKind::ENUM_VARIANT))
        .filter_map(|child| {
            child
                .children_with_tokens()
                .filter_map(rowan::NodeOrToken::into_token)
                .find(|token| token.kind() == SyntaxKind::WORD)
                .map(|token| {
                    let range = token.text_range();
                    usize::from(range.start())..usize::from(range.end())
                })
        })
        .collect()
}

/// Lossless, lexer-driven renaming of flat Session globals. Member/field keys
/// and lexically local names are intentionally excluded; every other bare word
/// resolves through the newest visible Session symbol.
fn rewrite_identifiers(
    source: &str,
    mapping: &indexmap::IndexMap<String, String>,
    forced: &indexmap::IndexMap<std::ops::Range<usize>, String>,
    skipped: &HashSet<std::ops::Range<usize>>,
    local_names: &HashSet<String>,
) -> String {
    // The lexer is intentionally context-free and exposes words inside quoted
    // strings/comments. Mark those byte ranges before considering identifier
    // tokens. Backtick strings stay live because `${...}` interpolation must
    // still resolve Session names (literal words there are harmless unless
    // they exactly equal a visible identifier).
    let bytes = source.as_bytes();
    let mut quoted_or_comment = vec![false; bytes.len()];
    let mut cursor = 0;
    while cursor < bytes.len() {
        let start = cursor;
        let end = if bytes[cursor..].starts_with(b"//") {
            cursor += 2;
            while cursor < bytes.len() && bytes[cursor] != b'\n' {
                cursor += 1;
            }
            cursor
        } else if bytes[cursor..].starts_with(b"/*") {
            cursor += 2;
            while cursor + 1 < bytes.len() && !bytes[cursor..].starts_with(b"*/") {
                cursor += 1;
            }
            (cursor + 2).min(bytes.len())
        } else if bytes[cursor] == b'#' {
            let hashes = bytes[cursor..]
                .iter()
                .take_while(|byte| **byte == b'#')
                .count();
            if bytes.get(cursor + hashes) == Some(&b'"') {
                cursor += hashes + 1;
                loop {
                    let Some(relative) = bytes[cursor..].iter().position(|byte| *byte == b'"')
                    else {
                        cursor = bytes.len();
                        break;
                    };
                    cursor += relative + 1;
                    if bytes
                        .get(cursor..cursor.saturating_add(hashes))
                        .is_some_and(|suffix| suffix.iter().all(|byte| *byte == b'#'))
                    {
                        cursor += hashes;
                        break;
                    }
                }
                cursor
            } else {
                cursor += 1;
                continue;
            }
        } else if bytes[cursor] == b'"' {
            cursor += 1;
            let mut escaped = false;
            while cursor < bytes.len() {
                let byte = bytes[cursor];
                cursor += 1;
                if byte == b'"' && !escaped {
                    break;
                }
                escaped = byte == b'\\' && !escaped;
                if byte != b'\\' {
                    escaped = false;
                }
            }
            cursor
        } else {
            cursor += 1;
            continue;
        };
        for byte in &mut quoted_or_comment[start..end] {
            *byte = true;
        }
        cursor = end;
    }
    let tokens = lex_lossless(source, baml_base::FileId::new(0));
    let significant = tokens
        .iter()
        .enumerate()
        .filter(|(_, token)| {
            let start = usize::from(token.span.range.start());
            if quoted_or_comment.get(start).copied().unwrap_or(false) {
                return false;
            }
            !matches!(token.kind, TokenKind::Whitespace | TokenKind::Newline)
        })
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    let significant_pos = significant
        .iter()
        .enumerate()
        .map(|(position, index)| (*index, position))
        .collect::<std::collections::HashMap<_, _>>();
    let mut output = String::with_capacity(source.len());
    for (index, token) in tokens.iter().enumerate() {
        let range = usize::from(token.span.range.start())..usize::from(token.span.range.end());
        if let Some(replacement) = forced.get(&range) {
            output.push_str(replacement);
            continue;
        }
        let replacement = if token.kind == TokenKind::Word
            && !quoted_or_comment.get(range.start).copied().unwrap_or(false)
            && !skipped.contains(&range)
            && !local_names.contains(token.text.as_str())
        {
            significant_pos.get(&index).and_then(|position| {
                let prev = position
                    .checked_sub(1)
                    .and_then(|p| significant.get(p))
                    .map(|&i| tokens[i].kind);
                let next = significant.get(position + 1).map(|&i| tokens[i].kind);
                let reserved_package_root = next == Some(TokenKind::Dot)
                    && (token.text.as_str() == "json"
                        || baml_builtins2::stdlib_package_names().contains(&token.text.as_str()));
                if prev == Some(TokenKind::Dot)
                    || next == Some(TokenKind::Colon)
                    || reserved_package_root
                {
                    None
                } else {
                    mapping.get(token.text.as_str())
                }
            })
        } else {
            None
        };
        output.push_str(replacement.map_or(token.text.as_str(), String::as_str));
    }
    output
}

struct LoweredSession {
    source: String,
    artifact: RuntimeSessionCompileArtifact,
    result_global: String,
}

/// Everything a compile carries *because* it is a session.
///
/// One value rather than four parallel `Option`s that must agree: a package
/// compile has none of this, a session compile has all of it, and there is no
/// state in between for a reader to invent an answer for.
struct SessionCompile {
    artifact: RuntimeSessionCompileArtifact,
    /// The global holding the submission's result, checked against `expected`.
    result_global: String,
    /// The contract from `eval<T>`; unknown when the eval is uncontracted.
    expected: bex_vm_types::SessionContract,
    lease: bex_vm_types::SessionEvalLease,
}

fn let_initializer_type(db: &ProjectDatabase, name: &str) -> Option<baml_type::Ty> {
    let package_id = PackageId::new(db, Name::new("user"));
    let package_items = baml_compiler2_hir::package::package_items(db, package_id);
    let Definition::Let(let_loc) = package_items.lookup_value(&[], &Name::new(name))? else {
        return None;
    };
    let inference = baml_compiler2_hir_ty::infer::infer_body(db, BodyOwnerId::Let(let_loc));
    let body = let_body(db, let_loc);
    let LetBody::Expr(body) = body.as_ref() else {
        return None;
    };
    body.root_expr
        .and_then(|root| inference.type_of_expr.get(&root).cloned())
}

fn lower_session_submission(
    request: &RuntimeSessionCompileRequest,
) -> Result<LoweredSession, Vec<RuntimeCompileDiagnostic>> {
    // P-6 is deliberately unavailable in a Session: its lexical package would
    // be ambiguous between the submitting package and the transient unit.
    if let Some(start) = request.source.find("reflect.Package.current") {
        return Err(vec![runtime_diagnostic(
            DiagnosticId::InvalidSyntax,
            &request.submission_name,
            start,
            start + "reflect.Package.current".len(),
            "`reflect.Package.current()` is not available inside a Session submission",
        )]);
    }

    let sequence = request
        .submission_name
        .chars()
        .filter(char::is_ascii_digit)
        .collect::<String>();
    let sequence = if sequence.is_empty() { "0" } else { &sequence };
    let internal = |name: &str| format!("__baml_session_{sequence}_{name}");

    let tokens = lex_lossless(&request.source, baml_base::FileId::new(0));
    let (green, _) = baml_compiler_parser::parse_file(&tokens);
    let root = SyntaxNode::new_root(green);
    let declarations_nodes = root
        .children()
        .filter(|node| {
            declaration_kind(node.kind())
                && (node.kind() == SyntaxKind::IMPLEMENTS_FOR || declaration_name(node).is_some())
                && !(node.kind() == SyntaxKind::TYPE_ALIAS_DEF
                    && runtime_type_binding_parts(&node.text().to_string()).is_some())
        })
        .collect::<Vec<_>>();

    let mut declarations = IndexMap::new();
    let mut declaration_name_ranges = IndexMap::new();
    for node in &declarations_nodes {
        if let Some((name, range)) = declaration_name(node) {
            let symbol = SessionVisibleSymbol {
                internal: internal(&name),
                kind: SessionVisibleKind::Declaration,
            };
            declaration_name_ranges.insert(range, symbol.internal.clone());
            declarations.insert(name, symbol);
        }
    }

    let mut declaration_mapping = request
        .visible
        .iter()
        .filter(|(_, symbol)| !matches!(symbol.kind, SessionVisibleKind::Let))
        .map(|(name, symbol)| (name.clone(), symbol.internal.clone()))
        .collect::<IndexMap<_, _>>();
    declaration_mapping.extend(
        declarations
            .iter()
            .map(|(name, symbol)| (name.clone(), symbol.internal.clone())),
    );

    let mut declaration_source = String::new();
    let mut masked = request.source.as_bytes().to_vec();
    for node in &declarations_nodes {
        let range = byte_range(node);
        let fragment = &request.source[range.clone()];
        let forced = declaration_name_ranges
            .iter()
            .filter(|(name_range, _)| {
                name_range.start >= range.start && name_range.end <= range.end
            })
            .map(|(name_range, replacement)| {
                (
                    name_range.start - range.start..name_range.end - range.start,
                    replacement.clone(),
                )
            })
            .collect::<IndexMap<_, _>>();
        let skipped = structural_name_ranges(node)
            .into_iter()
            .map(|name_range| name_range.start - range.start..name_range.end - range.start)
            .collect();
        let locals = locally_bound_names(node, None);
        declaration_source.push_str(&rewrite_identifiers(
            fragment,
            &declaration_mapping,
            &forced,
            &skipped,
            &locals,
        ));
        declaration_source.push('\n');
        for byte in &mut masked[range] {
            if *byte != b'\n' && *byte != b'\r' {
                *byte = b' ';
            }
        }
    }
    let masked = String::from_utf8(masked).expect("masking source preserves UTF-8");
    let prefix = "function __baml_session_parse__() -> unknown {\n";
    let wrapped = format!("{prefix}{masked}\n}}\n");
    let wrapped_tokens = lex_lossless(&wrapped, baml_base::FileId::new(0));
    let (wrapped_green, parse_errors) = baml_compiler_parser::parse_file(&wrapped_tokens);
    if let Some(error) = parse_errors.first() {
        let (span, message) = match error {
            baml_compiler_diagnostics::ParseError::UnexpectedToken {
                expected,
                found,
                span,
            } => (
                *span,
                format!("unexpected token: expected {expected}, found {found}"),
            ),
            baml_compiler_diagnostics::ParseError::UnexpectedEof { expected, span } => (
                *span,
                format!("unexpected end of file: expected {expected}"),
            ),
            baml_compiler_diagnostics::ParseError::InvalidSyntax { message, span }
            | baml_compiler_diagnostics::ParseError::RemovedFeature { message, span } => {
                (*span, message.clone())
            }
        };
        let range = span.range;
        let start = usize::from(range.start()).saturating_sub(prefix.len());
        let end = usize::from(range.end()).saturating_sub(prefix.len());
        return Err(vec![runtime_diagnostic(
            DiagnosticId::InvalidSyntax,
            &request.submission_name,
            start,
            end,
            message,
        )]);
    }
    let wrapped_root = SyntaxNode::new_root(wrapped_green);
    let block_node = wrapped_root
        .descendants()
        .find(|node| node.kind() == SyntaxKind::BLOCK_EXPR)
        .expect("synthetic function has a block");
    let block = BlockExpr::cast(block_node).expect("BLOCK_EXPR cast");
    let elements = block
        .elements()
        .filter(|element| !matches!(element, BlockElement::HeaderComment(_)))
        .collect::<Vec<_>>();

    let mut visible_mapping = request
        .visible
        .iter()
        .map(|(name, symbol)| (name.clone(), symbol.internal.clone()))
        .collect::<IndexMap<_, _>>();
    visible_mapping.extend(
        declarations
            .iter()
            .map(|(name, symbol)| (name.clone(), symbol.internal.clone())),
    );
    let mut active_type_bindings = request
        .visible
        .iter()
        .filter(|(_, symbol)| matches!(symbol.kind, SessionVisibleKind::TypeBinding { .. }))
        .map(|(name, symbol)| (name.clone(), symbol.clone()))
        .collect::<IndexMap<_, _>>();
    let mut generated = declaration_source.clone();
    let mut steps = Vec::new();
    let mut result_step = None;

    for (index, element) in elements.iter().enumerate() {
        let (node, wrapped_range, has_semicolon, is_statement) = match element {
            BlockElement::Stmt(node) => (
                Some(node),
                byte_range(node),
                element.has_trailing_semicolon(),
                true,
            ),
            BlockElement::ExprNode(node) => (
                Some(node),
                byte_range(node),
                element.has_trailing_semicolon(),
                expression_is_assignment(node),
            ),
            BlockElement::ExprToken(token) => {
                let range = token.text_range();
                (
                    None,
                    usize::from(range.start())..usize::from(range.end()),
                    element.has_trailing_semicolon(),
                    false,
                )
            }
            BlockElement::HeaderComment(_) => continue,
        };
        let mut source_range = wrapped_range.start.saturating_sub(prefix.len())
            ..wrapped_range.end.saturating_sub(prefix.len());
        if has_semicolon {
            let bytes = request.source.as_bytes();
            let mut end = source_range.end;
            while end < bytes.len() && bytes[end].is_ascii_whitespace() {
                end += 1;
            }
            if bytes.get(end) == Some(&b';') {
                source_range.end = end + 1;
            }
        }
        let raw = &request.source[source_range.clone()];
        let is_type_binding = node.is_some_and(|node| node.kind() == SyntaxKind::TYPE_BINDING_STMT);
        let is_outer_let = node.is_some_and(|node| node.kind() == SyntaxKind::LET_STMT);
        let outer_binding = (!is_type_binding)
            .then(|| node.and_then(first_pattern_name))
            .flatten();
        let local_names = node.map_or_else(HashSet::new, |node| {
            locally_bound_names(node, outer_binding.as_ref().map(|(_, range)| range))
        });
        let prelude = runtime_type_binding_prelude(&active_type_bindings);
        let (generated_name, step_source, commit_global, binding) = if is_type_binding {
            let Some((name, operand)) = runtime_type_binding_parts(raw) else {
                return Err(vec![runtime_diagnostic(
                    DiagnosticId::InvalidSyntax,
                    &request.submission_name,
                    source_range.start,
                    source_range.end,
                    "invalid runtime type binding",
                )]);
            };
            let type_name = internal(&name);
            let backing_name = format!("__baml_session_{sequence}_type_value_{name}");
            let operand = rewrite_identifiers(
                operand,
                &visible_mapping,
                &IndexMap::new(),
                &HashSet::new(),
                &local_names,
            );
            let source = format!("let {backing_name} = {{\n{prelude}({operand})\n}}\n");
            let symbol = SessionVisibleSymbol {
                internal: type_name,
                kind: SessionVisibleKind::TypeBinding {
                    type_value: backing_name.clone(),
                },
            };
            (backing_name, source, None, Some((name, symbol)))
        } else {
            let mut forced = IndexMap::new();
            let mut binding = None;
            let generated_name = if let Some((name, wrapped_name_range)) = outer_binding {
                let internal_name = internal(&name);
                let local_range = wrapped_name_range.start.saturating_sub(prefix.len())
                    - source_range.start
                    ..wrapped_name_range.end.saturating_sub(prefix.len()) - source_range.start;
                forced.insert(local_range, internal_name.clone());
                let symbol = SessionVisibleSymbol {
                    internal: internal_name.clone(),
                    kind: SessionVisibleKind::Let,
                };
                binding = Some((name, symbol));
                internal_name
            } else {
                // Synthetic step globals must live outside `internal()`'s
                // namespace so a user binding such as `stmt_1` cannot mint
                // the same name.
                format!("__baml_stmt_{sequence}_{index}")
            };
            let assignment = is_statement
                .then(|| assignment_parts(raw))
                .flatten()
                .and_then(|(target, operator, rhs)| {
                    let symbol = request.visible.get(target)?;
                    matches!(symbol.kind, SessionVisibleKind::Let)
                        .then_some((symbol, operator, rhs))
                });
            let (source, commit_global) = if let Some((target, operator, rhs)) = assignment {
                let rhs = rewrite_identifiers(
                    rhs,
                    &visible_mapping,
                    &IndexMap::new(),
                    &HashSet::new(),
                    &local_names,
                );
                // An assignment to a visible binding IS an ordinary
                // assignment, so it is written as one. The binding lives in a
                // global and a global cannot be assigned in place, which is
                // why this road exists at all — but binding a local to the
                // global first and assigning THAT gives the ordinary
                // assignment road everything it needs: the local carries the
                // binding's type, so the value checks against it and a
                // mismatch is the same diagnostic ordinary BAML gives. The
                // local's final value is what `commit_global` writes back, and
                // a compound operator dispatches through the same road it does
                // anywhere else.
                //
                // The value is spliced in exactly as it was written, with no
                // wrapping parentheses: parenthesizing it would change the
                // verdict (a fresh literal loses its freshness inside
                // parentheses, so `n += 1.5` on an `int` binding would be
                // refused here while ordinary code accepts it), and the whole
                // point is that the two roads agree.
                // The local's name must be one `internal()` can never mint: a
                // user binding called `target_1` in submission N would mint
                // `__baml_session_N_target_1`, and a block-local of that name
                // shadows the global the rewritten value reads — the
                // assignment would silently read itself. A different root
                // prefix is outside `internal()`'s range entirely.
                let target_local = format!("__baml_assign_{sequence}_{index}");
                let assign = if operator == "=" {
                    format!("{target_local} = {rhs}")
                } else {
                    format!("{target_local} {operator}= {rhs}")
                };
                let source = format!(
                    "let {generated_name} = {{\n{prelude}let {target_local} = {}\n{assign}\n{target_local}\n}}\n",
                    target.internal
                );
                (source, Some(format!("user.{}", target.internal)))
            } else {
                let rewritten = rewrite_identifiers(
                    raw,
                    &visible_mapping,
                    &forced,
                    &HashSet::new(),
                    &local_names,
                );
                let source = if is_outer_let && prelude.is_empty() {
                    format!("{rewritten}\n")
                } else if is_outer_let {
                    format!(
                        "let {generated_name} = {{\n{prelude}{rewritten}\n{generated_name}\n}}\n"
                    )
                } else if !is_statement && !has_semicolon && prelude.is_empty() {
                    format!("let {generated_name} = ({rewritten})\n")
                } else if !is_statement && !has_semicolon {
                    format!("let {generated_name} = {{\n{prelude}{rewritten}\n}}\n")
                } else {
                    format!("let {generated_name} = {{\n{prelude}{rewritten}\nnull\n}}\n")
                };
                (source, None)
            };
            (generated_name, source, commit_global, binding)
        };
        generated.push_str(&step_source);
        let returns_value =
            index + 1 == elements.len() && !is_outer_let && !is_statement && !has_semicolon;
        if returns_value {
            result_step = Some(steps.len());
        }
        let kind = binding
            .clone()
            .map_or(RuntimeSessionStepKind::Expression, |(name, symbol)| {
                RuntimeSessionStepKind::Binding {
                    name,
                    symbol,
                    replay_source: step_source.clone(),
                }
            });
        steps.push(RuntimeSessionStep {
            global: format!("user.{generated_name}"),
            commit_global,
            kind,
        });
        if let Some((name, symbol)) = binding {
            visible_mapping.insert(name.clone(), symbol.internal.clone());
            if matches!(symbol.kind, SessionVisibleKind::TypeBinding { .. }) {
                active_type_bindings.insert(name, symbol);
            }
        }
    }

    if result_step.is_none() {
        // This fallback must likewise be outside `internal()`'s namespace:
        // otherwise a user binding called `result` collides with it.
        let generated_name = format!("__baml_result_{sequence}");
        let step_source = format!("let {generated_name} = null\n");
        generated.push_str(&step_source);
        result_step = Some(steps.len());
        steps.push(RuntimeSessionStep {
            global: format!("user.{generated_name}"),
            commit_global: None,
            kind: RuntimeSessionStepKind::Expression,
        });
    }
    let result_step = result_step.expect("session lowering always has a result");
    let result_global = steps[result_step].global.clone();
    Ok(LoweredSession {
        source: generated,
        artifact: RuntimeSessionCompileArtifact {
            submission_name: request.submission_name.clone(),
            declaration_source,
            declarations,
            steps,
            result_step: Some(result_step),
            initializers: Vec::new(),
        },
        result_global,
    })
}

/// Retain only initializer helpers owned by the new submission. A fresh
/// compile correctly checks all replayed source, but its group-wide init tail
/// contains every historical helper; retaining that tail would make Session
/// runtime growth quadratic and would rerun old initializers.
fn prune_session_init_tail(
    tail: &InitTail,
    submission_name: &str,
) -> Result<(InitTail, Vec<RuntimeSessionInitializer>), String> {
    let old_object_count = tail.objects.len();
    let old_slot_count = tail.slot_objects.len();
    let named_objects = tail
        .named
        .iter()
        .map(|(_, index)| *index as usize)
        .collect::<HashSet<_>>();
    let helper_slots = tail
        .slot_objects
        .iter()
        .enumerate()
        .filter(|(_, object)| !named_objects.contains(&(**object as usize)))
        .map(|(slot, object)| (slot, *object as usize))
        .collect::<Vec<_>>();
    let selected = helper_slots
        .iter()
        .filter(|(_, object)| {
            matches!(
                tail.objects.get(*object),
                Some(bex_vm_types::Object::Function(function))
                    if function.source_file == submission_name
            )
        })
        .copied()
        .collect::<Vec<_>>();
    let selected_slot_map = selected
        .iter()
        .enumerate()
        .map(|(new, (old, _))| (*old, new))
        .collect::<std::collections::HashMap<_, _>>();

    let init_object = tail
        .package_init_order
        .iter()
        .find_map(|init_name| {
            tail.named
                .iter()
                .find(|(name, _)| name == init_name)
                .map(|(_, index)| *index as usize)
        })
        .and_then(|index| tail.objects.get(index));
    let mut initializers = Vec::new();
    if let Some(bex_vm_types::Object::Function(init)) = init_object {
        let mut pending = None;
        for instruction in &init.bytecode.instructions {
            match instruction {
                Instruction::Call { callee, .. }
                | Instruction::CallWithRuntimeId { callee, .. } => {
                    pending = selected_slot_map.get(&callee.raw()).copied();
                }
                Instruction::StoreGlobal(target) => {
                    if let Some(helper_slot) = pending.take() {
                        let raw = target.raw();
                        if raw < old_slot_count {
                            return Err(format!(
                                "Session init helper stored into unexpected tail-local slot {raw}"
                            ));
                        }
                        let symbol =
                            tail.global_imports
                                .get(raw - old_slot_count)
                                .ok_or_else(|| {
                                    format!("Session init global import {raw} is out of bounds")
                                })?;
                        initializers.push(RuntimeSessionInitializer {
                            helper_slot: u32::try_from(helper_slot)
                                .map_err(|_| "too many Session initializer helpers".to_string())?,
                            target_global: symbol.fq_name.clone(),
                        });
                    }
                }
                _ => {}
            }
        }
    }
    if initializers.len() != selected.len() {
        return Err(format!(
            "Session init tail retained {} helpers but found {} stores",
            selected.len(),
            initializers.len()
        ));
    }

    // Each helper's literals/lambdas immediately precede its function object.
    // The preceding slot owner therefore marks the start of this helper group.
    let mut selected_old_objects = Vec::new();
    for (old_slot, helper_object) in &selected {
        let start = old_slot
            .checked_sub(1)
            .and_then(|slot| tail.slot_objects.get(slot))
            .map_or(0, |object| *object as usize + 1);
        selected_old_objects.extend(start..=*helper_object);
    }
    let object_map = selected_old_objects
        .iter()
        .enumerate()
        .map(|(new, old)| (*old, new))
        .collect::<std::collections::HashMap<_, _>>();
    let new_object_count = selected_old_objects.len();
    let new_slot_count = selected.len();
    let mut used_object_imports = std::collections::BTreeSet::new();
    let mut used_global_imports = std::collections::BTreeSet::new();
    for old in &selected_old_objects {
        let mut object = tail.objects[*old].clone();
        visit_object_operands(&mut object, |operand| match operand {
            IndexOperand::Object(index) if index.raw() >= old_object_count => {
                used_object_imports.insert(index.raw() - old_object_count);
            }
            IndexOperand::Global(index) if index.raw() >= old_slot_count => {
                used_global_imports.insert(index.raw() - old_slot_count);
            }
            _ => {}
        });
    }
    let object_import_map = used_object_imports
        .iter()
        .enumerate()
        .map(|(new, old)| (*old, new))
        .collect::<std::collections::HashMap<_, _>>();
    let global_import_map = used_global_imports
        .iter()
        .enumerate()
        .map(|(new, old)| (*old, new))
        .collect::<std::collections::HashMap<_, _>>();
    let mut objects = Vec::with_capacity(new_object_count);
    for old in &selected_old_objects {
        let mut object = tail.objects[*old].clone();
        let mut relocation_error = None;
        visit_object_operands(&mut object, |operand| match operand {
            IndexOperand::Object(index) => {
                let raw = index.raw();
                let relocated = if raw < old_object_count {
                    object_map.get(&raw).copied().ok_or_else(|| {
                        format!("Session helper references omitted tail object {raw}")
                    })
                } else {
                    object_import_map
                        .get(&(raw - old_object_count))
                        .map(|import| new_object_count + *import)
                        .ok_or_else(|| format!("Session helper object import {raw} was omitted"))
                };
                match relocated {
                    Ok(raw) => *index = bex_vm_types::ObjectIndex::from_raw(raw),
                    Err(error) => relocation_error = Some(error),
                }
            }
            IndexOperand::Global(index) => {
                let raw = index.raw();
                let relocated = if raw < old_slot_count {
                    selected_slot_map
                        .get(&raw)
                        .copied()
                        .ok_or_else(|| format!("Session helper references omitted tail slot {raw}"))
                } else {
                    global_import_map
                        .get(&(raw - old_slot_count))
                        .map(|import| new_slot_count + *import)
                        .ok_or_else(|| format!("Session helper global import {raw} was omitted"))
                };
                match relocated {
                    Ok(raw) => *index = bex_vm_types::GlobalIndex::from_raw(raw),
                    Err(error) => relocation_error = Some(error),
                }
            }
        });
        if let Some(error) = relocation_error {
            return Err(error);
        }
        objects.push(object);
    }
    let slot_objects = selected
        .iter()
        .map(|(_, old_object)| {
            object_map
                .get(old_object)
                .copied()
                .and_then(|index| u32::try_from(index).ok())
                .ok_or_else(|| "Session helper object relocation failed".to_string())
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok((
        InitTail {
            objects,
            object_imports: used_object_imports
                .into_iter()
                .map(|index| tail.object_imports[index].clone())
                .collect(),
            global_imports: used_global_imports
                .into_iter()
                .map(|index| tail.global_imports[index].clone())
                .collect(),
            slot_objects,
            named: Vec::new(),
            package_init_order: Vec::new(),
        },
        initializers,
    ))
}

impl RuntimeCompiler for ProjectRuntimeCompiler {
    fn compile(
        &self,
        request: RuntimeCompileRequest,
    ) -> Result<RuntimeCompileArtifact, Vec<RuntimeCompileDiagnostic>> {
        let RuntimeCompileRequest {
            files,
            packages,
            mode,
        } = request;
        let (files, mut session) = match mode {
            RuntimeCompileMode::Package => (files, None),
            RuntimeCompileMode::Session(session) => {
                let session = *session;
                let lowered = lower_session_submission(&session)?;
                let mut files = session.history.clone();
                files.insert(session.submission_name.clone(), lowered.source);
                (
                    files,
                    Some(SessionCompile {
                        artifact: lowered.artifact,
                        result_global: lowered.result_global,
                        expected: session.expected,
                        lease: session.lease,
                    }),
                )
            }
        };
        let stdlib = crate::precompiled_stdlib::load().map_err(|message| {
            vec![RuntimeCompileDiagnostic {
                code: "E_RUNTIME_STDLIB".to_string(),
                message,
                severity: RuntimeDiagnosticSeverity::Error,
                span: None,
            }]
        })?;
        // This local is the transience guarantee: no handle to `db` occurs in
        // either return type, and all retained values below are deep-owned.
        // The stdlib arrives as precompiled interface blobs below (never as
        // source), so no `Stdlib` roots are materialized.
        let mut db = ProjectDatabase::new();
        let workspace = db
            .add_source_root(SourceRootSpec {
                path: PathBuf::from(RUNTIME_VIRTUAL_ROOT),
                package: Name::new(baml_type::RESERVED_USER_PACKAGE),
                kind: baml_base::SourceRootKind::Workspace,
            })
            .unwrap_or_else(|e| unreachable!("fresh database accepts one workspace root: {e}"));
        let aliases: Vec<Name> = packages
            .keys()
            .map(|name| Name::new(name.as_str()))
            .collect();
        let enriched = packages
            .into_iter()
            .map(|(name, package)| {
                enrich_runtime_mount(&name, &aliases, package)
                    .map(|(blob, stubs)| (name, blob, stubs))
            })
            .collect::<Result<Vec<_>, _>>()
            .map_err(|diagnostic| vec![diagnostic])?;
        let mounted = enriched
            .iter()
            .map(|(name, blob, _)| (name.clone(), blob.clone()))
            .collect::<BTreeMap<_, _>>();
        let precompiled_stdlib_names = stdlib.interfaces.keys().cloned().collect::<Vec<_>>();
        db.set_mounted_packages(mounted).map_err(|message| {
            vec![RuntimeCompileDiagnostic {
                code: "E_RUNTIME_INTERFACE".to_string(),
                message,
                severity: RuntimeDiagnosticSeverity::Error,
                span: None,
            }]
        })?;
        db.set_precompiled_stdlib_packages(stdlib.interfaces);
        debug_assert!(
            precompiled_stdlib_names.iter().all(|name| {
                baml_compiler2_hir::package::is_precompiled_package(&db, &Name::new(name))
            }),
            "set_mounted_packages must run before set_precompiled_stdlib_packages"
        );
        // Emit needs concrete pool/global slots while producing the consumer's
        // relocatable units. Materialize link-only native stubs in the mounted
        // package; the mounted interface remains the semantic authority, and
        // the stub units are discarded below so the final artifact keeps the
        // dependency references unresolved for the runtime linker.
        for (mount_index, (alias, _, stubs)) in enriched.iter().enumerate() {
            if baml_compiler2_hir::package::is_reserved_package_name(alias) || stubs.is_empty() {
                continue;
            }
            // Stub units live in a `Dynamic` root for the mount's package
            // (its virtual `<builtin>/<alias>` prefix): runtime-loaded, so it
            // sorts after every statically compiled root.
            let stub_root = db
                .add_source_root(SourceRootSpec {
                    path: PathBuf::from(format!("{BUILTIN_VIRTUAL_ROOT}/{alias}")),
                    package: Name::new(alias),
                    kind: baml_base::SourceRootKind::Dynamic,
                })
                .map_err(|error| {
                    vec![RuntimeCompileDiagnostic {
                        code: "E_RUNTIME_MOUNT".to_string(),
                        message: format!("cannot mount package `{alias}`: {error}"),
                        severity: RuntimeDiagnosticSeverity::Error,
                        span: None,
                    }]
                })?;
            let stub_files: Vec<(PathBuf, &str)> = stubs
                .iter()
                .enumerate()
                .map(|(stub_index, (namespace, _name, source))| {
                    (
                        runtime_mount_virtual_path(alias, namespace, mount_index, stub_index),
                        source.as_str(),
                    )
                })
                .collect();
            db.add_or_update_files_in(
                stub_root,
                stub_files
                    .iter()
                    .map(|(path, source)| (path.as_path(), *source)),
            );
        }
        for (path, source) in files {
            // Runtime input names are package-relative. Mounting them beneath
            // the synthetic root makes `ns_foo/` namespace derivation behave
            // exactly like an ordinary project without exposing the synthetic
            // prefix in diagnostics.
            let path = runtime_source_virtual_path(&path);
            if session.is_some() {
                db.add_session_file(path, &source);
            } else {
                db.add_or_update_file_in(workspace, &path, &source);
            }
        }

        let diagnostics: Vec<_> = collect_diagnostics(&db)
            .iter()
            .map(|diagnostic| owned_diagnostic(&db, diagnostic))
            .collect();
        if diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == RuntimeDiagnosticSeverity::Error)
        {
            return Err(diagnostics);
        }

        if let Some(session) = &session
            && !matches!(
                session.expected,
                bex_vm_types::SessionContract::Checkable(baml_type::RuntimeTy::Unknown { .. })
            )
        {
            let file = session.artifact.submission_name.as_str();
            let result_name = session
                .result_global
                .strip_prefix("user.")
                .unwrap_or(session.result_global.as_str());
            let Some(actual) = let_initializer_type(&db, result_name) else {
                return Err(vec![RuntimeCompileDiagnostic {
                    code: "E_RUNTIME_SESSION".to_string(),
                    message: format!(
                        "internal compiler error: Session result binding `{result_name}` has no initializer type"
                    ),
                    severity: RuntimeDiagnosticSeverity::Error,
                    span: Some(RuntimeSourceSpan {
                        file: file.to_string(),
                        start: 0,
                        end: 0,
                    }),
                }]);
            };
            // The check runs in the compiler's context, which names declarations
            // rather than pointing at them. `eval<T>` can be handed a
            // runtime-created type, which has no name to recover — the engine
            // classified that under the heap permit (nothing heap-shaped may
            // reach this task; see `SessionContract`), so report the
            // unstateable contract it recorded rather than comparing against a
            // stand-in and reporting whatever mismatch the stand-in produces.
            let bex_vm_types::SessionContract::Checkable(expected) = session.expected.clone()
            else {
                return Err(vec![runtime_diagnostic(
                    DiagnosticId::TypeMismatch,
                    file,
                    0,
                    0,
                    "`eval` contract names a runtime-created declaration, which the \
                     compiler cannot check a submission against"
                        .to_string(),
                )]);
            };
            let expected = baml_type::Ty::from(expected);
            let context = baml_compiler2_hir_ty::facts::Facts::new(&db);
            if !baml_type::normalize::is_subtype(&actual, &expected, &context) {
                return Err(vec![runtime_diagnostic(
                    DiagnosticId::TypeMismatch,
                    file,
                    0,
                    0,
                    format!(
                        "submission result has type `{actual}`, which is not a subtype of requested contract `{expected}`"
                    ),
                )]);
            }
        }

        let interface = package_interface(&db, PackageId::new(&db, Name::new("user")));
        let interface_blob =
            baml_artifact::encode(baml_artifact::ArtifactKind::PackageInterface, interface)
                .map_err(|error| {
                    vec![RuntimeCompileDiagnostic {
                        code: "E_RUNTIME_INTERFACE".to_string(),
                        message: error.to_string(),
                        severity: RuntimeDiagnosticSeverity::Error,
                        span: None,
                    }]
                })?;
        let emitted = emit_units_with_stdlib(
            &db,
            crate::precompiled_stdlib_config::OPT_LEVEL,
            &stdlib.program,
        )
        .map_err(|error| {
            vec![RuntimeCompileDiagnostic {
                code: "E_RUNTIME_EMIT".to_string(),
                message: error.to_string(),
                severity: RuntimeDiagnosticSeverity::Error,
                span: None,
            }]
        })?;
        let mut units: Vec<_> = emitted
            .into_iter()
            .filter(|unit| {
                unit.package.as_str() == "user"
                    && session
                        .as_ref()
                        .is_none_or(|s| unit.source_file == s.artifact.submission_name)
            })
            .collect();
        if let Some(session) = session.as_mut() {
            let mut initializers = Vec::new();
            for unit in &mut units {
                if let Some(tail) = unit.init_tail.take() {
                    let (tail, retained) =
                        prune_session_init_tail(&tail, &session.artifact.submission_name).map_err(
                            |message| {
                                vec![RuntimeCompileDiagnostic {
                                    code: "E_RUNTIME_SESSION_INIT".to_string(),
                                    message,
                                    severity: RuntimeDiagnosticSeverity::Error,
                                    span: None,
                                }]
                            },
                        )?;
                    unit.init_tail = Some(tail);
                    initializers.extend(retained);
                }
            }
            session.artifact.initializers = initializers;
        }
        let kind = session.map_or(bex_vm_types::ArtifactKind::Package, |session| {
            bex_vm_types::ArtifactKind::Session {
                meta: session.artifact,
                lease: session.lease,
            }
        });
        Ok(RuntimeCompileArtifact {
            units,
            interface_blob,
            diagnostics,
            kind,
        })
    }
}

#[cfg(test)]
mod tests {
    use baml_compiler2_hir::file_package::file_package;
    use baml_compiler2_hir_ty::package_interface::{FunctionThrowSets, PackageInterface};

    use super::*;

    #[test]
    fn unspellable_package_detection_is_recursive() {
        let aliases = vec![Name::new("app")];
        let viewpoint = StubViewpoint { aliases: &aliases };
        let class_list = |qtn: baml_type::QualifiedTypeName| {
            baml_type::Ty::List(
                Box::new(baml_type::Ty::Class(
                    qtn,
                    Box::new([]),
                    baml_type::TyAttr::default(),
                )),
                baml_type::TyAttr::default(),
            )
        };
        // Local, stdlib, and mount-alias packages are spellable — even nested.
        assert!(
            !viewpoint.hides_type(&class_list(baml_type::QualifiedTypeName::local(Name::new(
                "SourceClass"
            ))))
        );
        assert!(!viewpoint.hides_type(&class_list(
            baml_type::QualifiedTypeName::from_dotted_path("app.Mounted")
        )));
        // A package from some other compile world is not, wherever it nests.
        assert!(
            viewpoint.hides_type(&class_list(baml_type::QualifiedTypeName::from_dotted_path(
                "elsewhere.Foreign"
            )))
        );
    }

    #[test]
    fn runtime_virtual_paths_are_slash_oriented() {
        let source = runtime_source_virtual_path(r"ns_tools\ns_nested\main.baml");
        assert_eq!(
            source.to_string_lossy(),
            "<runtime>/ns_tools/ns_nested/main.baml"
        );

        let mount =
            runtime_mount_virtual_path("app", &[Name::new("tools"), Name::new("nested")], 7, 9);
        assert_eq!(
            mount.to_string_lossy(),
            "<builtin>/app/ns_tools/ns_nested/runtime_mount_7_9.baml"
        );
        assert!(!source.to_string_lossy().contains('\\'));
        assert!(!mount.to_string_lossy().contains('\\'));
    }

    #[test]
    fn runtime_virtual_paths_derive_packages_and_namespaces() {
        let mut db = ProjectDatabase::new();
        let workspace = db
            .add_source_root(SourceRootSpec {
                path: PathBuf::from(RUNTIME_VIRTUAL_ROOT),
                package: Name::new(baml_type::RESERVED_USER_PACKAGE),
                kind: baml_base::SourceRootKind::Workspace,
            })
            .unwrap();

        let source = db.add_or_update_file_in(
            workspace,
            &runtime_source_virtual_path(r"ns_tools\ns_nested\main.baml"),
            "",
        );
        let source_package = file_package(&db, source);
        assert_eq!(source_package.package.as_str(), "user");
        assert_eq!(
            source_package
                .namespace_path
                .iter()
                .map(Name::as_str)
                .collect::<Vec<_>>(),
            ["tools", "nested"]
        );

        let mount_root = db
            .add_source_root(SourceRootSpec {
                path: PathBuf::from(format!("{BUILTIN_VIRTUAL_ROOT}/app")),
                package: Name::new("app"),
                kind: baml_base::SourceRootKind::Dynamic,
            })
            .unwrap();
        let mount = db.add_or_update_file_in(
            mount_root,
            &runtime_mount_virtual_path("app", &[Name::new("tools"), Name::new("nested")], 0, 0),
            "",
        );
        let mount_package = file_package(&db, mount);
        assert_eq!(mount_package.package.as_str(), "app");
        assert_eq!(
            mount_package
                .namespace_path
                .iter()
                .map(Name::as_str)
                .collect::<Vec<_>>(),
            ["tools", "nested"]
        );
    }

    #[test]
    fn runtime_diagnostic_paths_tolerate_backslashes() {
        assert_eq!(
            runtime_relative_virtual_path(Path::new(r"<runtime>\ns_tools\main.baml")),
            "ns_tools/main.baml"
        );
        assert_eq!(
            runtime_relative_virtual_path(Path::new("<runtime>/main.baml")),
            "main.baml"
        );
        assert_eq!(
            runtime_relative_virtual_path(Path::new(RUNTIME_VIRTUAL_ROOT)),
            ""
        );
    }

    /// The link stubs a mount emits for a package exporting a class `Host`
    /// with the given fields and the inherent methods `greet(self, punct?)`
    /// and `make(name)`, plus an interface `Describable` with the required
    /// method `describe(self)` and the default method `shout(self)`.
    fn host_and_describable_stubs(
        fields: Vec<(
            Name,
            baml_type::Ty,
            baml_compiler2_hir_ty::package_interface::ExportedFieldAttrs,
        )>,
    ) -> Vec<(Vec<Name>, Name, String)> {
        use baml_compiler2_hir_ty::{
            callable::{ExternalCallTarget, ExternalLinkability},
            package_interface::{ExportedFunction, ExportedType},
        };
        use baml_type::{FunctionParamTy, ParamTy, Ty, TyAttr};

        let app = Name::new("app");
        let class_qtn =
            baml_type::QualifiedTypeName::new(app.clone(), Vec::new(), Name::new("Host"));
        let iface_qtn =
            baml_type::QualifiedTypeName::new(app.clone(), Vec::new(), Name::new("Describable"));
        let self_param = ParamTy::new(0, Name::new("Self"));
        let self_ty = Ty::TypeVar(self_param.clone(), TyAttr::default());
        let function = |name: &str, params: Vec<FunctionParamTy>, target: ExternalCallTarget| {
            ExportedFunction {
                name: Name::new(name),
                params,
                return_type: Ty::string(),
                callable_throws: Ty::Never {
                    attr: TyAttr::default(),
                },
                generic_params: Vec::new(),
                generic_param_bounds: Vec::new(),
                builtin_kind: None,
                target,
                linkability: ExternalLinkability::Linkable,
            }
        };
        let class_self = FunctionParamTy::required(
            Some(Name::new("self")),
            Ty::Class(class_qtn.clone(), Box::new([]), TyAttr::default()),
        );
        let interface_self = FunctionParamTy::required(Some(Name::new("self")), self_ty);
        let mut types = IndexMap::new();
        let mut root = IndexMap::new();
        root.insert(
            Name::new("Host"),
            ExportedType::Class {
                qtn: class_qtn,
                fields,
                methods: vec![
                    function(
                        "greet",
                        vec![
                            class_self,
                            FunctionParamTy::optional(Some(Name::new("punct")), Ty::string()),
                        ],
                        ExternalCallTarget::Method {
                            package: app.clone(),
                            namespace: Vec::new(),
                            class: Name::new("Host"),
                            name: Name::new("greet"),
                        },
                    ),
                    function(
                        "make",
                        vec![FunctionParamTy::required(
                            Some(Name::new("name")),
                            Ty::string(),
                        )],
                        ExternalCallTarget::Method {
                            package: app,
                            namespace: Vec::new(),
                            class: Name::new("Host"),
                            name: Name::new("make"),
                        },
                    ),
                ],
                generic_params: Vec::new(),
                generic_param_bounds: Vec::new(),
            },
        );
        root.insert(
            Name::new("Describable"),
            ExportedType::Interface {
                qtn: iface_qtn.clone(),
                self_param,
                generic_params: Vec::new(),
                param_bounds: Vec::new(),
                requires: Vec::new(),
                associated_types: Vec::new(),
                fields: Vec::new(),
                required_methods: vec![function(
                    "describe",
                    vec![interface_self.clone()],
                    ExternalCallTarget::Interface {
                        interface: iface_qtn.clone(),
                        method: Name::new("describe"),
                    },
                )],
                default_methods: vec![function(
                    "shout",
                    vec![interface_self],
                    ExternalCallTarget::Interface {
                        interface: iface_qtn,
                        method: Name::new("shout"),
                    },
                )],
            },
        );
        types.insert(Vec::new(), root);
        let interface = PackageInterface {
            types,
            functions: IndexMap::new(),
            throw_sets: FunctionThrowSets::default(),
            namespaces: std::collections::BTreeSet::default(),
            impls: Vec::new(),
        };
        let interface_blob =
            baml_artifact::encode(baml_artifact::ArtifactKind::PackageInterface, &interface)
                .expect("package interface encodes");
        let package = RuntimePackageMount {
            interface_blob,
            types: Vec::new(),
        };
        let (_, stubs) = enrich_runtime_mount("app", &[Name::new("app")], package)
            .expect("runtime mount enriches");
        stubs
    }

    /// Methods of a mounted class or interface are stubbed inside their
    /// owner's body — never as free functions under a `ns_<Owner>/` namespace,
    /// which would shadow the owner's own stub (E0099) and leave the source
    /// stub the type checker sees without the methods the mounted interface
    /// exports.
    #[test]
    fn runtime_mount_stubs_spell_methods_inside_their_owner() {
        use baml_compiler2_hir_ty::package_interface::ExportedFieldAttrs;

        let stubs = host_and_describable_stubs(vec![(
            Name::new("name"),
            baml_type::Ty::string(),
            ExportedFieldAttrs::default(),
        )]);

        assert!(
            stubs.iter().all(|(namespace, ..)| namespace.is_empty()),
            "no stub may open a namespace under the mount: {stubs:?}"
        );
        let source_of = |name: &str| {
            &stubs
                .iter()
                .find(|(_, stub, _)| stub.as_str() == name)
                .unwrap_or_else(|| panic!("missing stub for {name}: {stubs:?}"))
                .2
        };
        assert_eq!(
            source_of("Host"),
            "class Host {\n  name string\n  function greet(self, punct: string = null) -> string \
             throws never { $rust_function }\n  function make(name: string) -> string throws \
             never { $rust_function }\n}\n"
        );
        assert_eq!(
            source_of("Describable"),
            "interface Describable {\n  function describe(self) -> string throws never\n  \
             function shout(self) -> string throws never { $rust_function }\n}\n"
        );
    }

    /// A class whose stub cannot be spelled (here: a field name that is not a
    /// source identifier) still keeps its inherent methods slottable for the
    /// emitter — as free link stubs under the class-named namespace, the
    /// pre-existing form, which shadows nothing because there is no class
    /// stub to shadow.
    #[test]
    fn runtime_mount_falls_back_to_free_method_stubs_without_a_class_stub() {
        use baml_compiler2_hir_ty::package_interface::ExportedFieldAttrs;

        let stubs = host_and_describable_stubs(vec![(
            Name::new("0"),
            baml_type::Ty::string(),
            ExportedFieldAttrs::default(),
        )]);

        assert!(
            stubs.iter().all(|(_, name, _)| name.as_str() != "Host"),
            "an unspellable class must not get a class stub: {stubs:?}"
        );
        let host = vec![Name::new("Host")];
        let free_stub = |name: &str| {
            &stubs
                .iter()
                .find(|(namespace, stub, _)| *namespace == host && stub.as_str() == name)
                .unwrap_or_else(|| panic!("missing free stub for Host.{name}: {stubs:?}"))
                .2
        };
        assert_eq!(
            free_stub("greet"),
            "function greet(arg0: unknown, punct: unknown = null) -> string { $rust_function }\n"
        );
        assert_eq!(
            free_stub("make"),
            "function make(name: unknown) -> string { $rust_function }\n"
        );
    }

    #[test]
    fn runtime_mount_stubs_preserve_declaration_and_variant_docstrings() {
        let interface = PackageInterface {
            types: IndexMap::new(),
            functions: IndexMap::new(),
            throw_sets: FunctionThrowSets::default(),
            namespaces: std::collections::BTreeSet::default(),
            impls: Vec::new(),
        };
        let interface_blob =
            baml_artifact::encode(baml_artifact::ArtifactKind::PackageInterface, &interface)
                .expect("empty package interface encodes");
        let class_name = Name::new("RuntimeClass");
        let enum_name = Name::new("RuntimeState");
        let class_qtn =
            baml_type::QualifiedTypeName::new(Name::new("app"), Vec::new(), class_name.clone());
        let enum_qtn =
            baml_type::QualifiedTypeName::new(Name::new("app"), Vec::new(), enum_name.clone());
        let package = RuntimePackageMount {
            interface_blob,
            types: vec![
                bex_vm_types::RuntimeTypeMount {
                    export_name: Name::new("ClassAlias"),
                    ty: baml_type::RealizedTy::Class(
                        class_qtn,
                        Box::new([]),
                        baml_type::TyAttr::default(),
                    ),
                    classes: vec![bex_vm_types::RuntimeMountedClass {
                        name: class_name,
                        tag: baml_type::typetag::TypeTag::of_head("runtime.RuntimeClass"),
                        docstring: Some("Runtime class docs".to_string()),
                        fields: vec![(
                            Name::new("value"),
                            baml_type::Ty::string(),
                            bex_vm_types::RuntimeMountedFieldAttrs {
                                docstring: Some("Runtime field docs".to_string()),
                                ..Default::default()
                            },
                        )],
                    }],
                    enums: Vec::new(),
                    witnesses: Vec::new(),
                },
                bex_vm_types::RuntimeTypeMount {
                    export_name: Name::new("StateAlias"),
                    ty: baml_type::RealizedTy::Enum(enum_qtn, baml_type::TyAttr::default()),
                    classes: Vec::new(),
                    enums: vec![bex_vm_types::RuntimeMountedEnum {
                        name: enum_name,
                        tag: baml_type::typetag::TypeTag::of_head("runtime.RuntimeState"),
                        docstring: Some("Runtime enum docs".to_string()),
                        variants: vec![(
                            Name::new("READY"),
                            bex_vm_types::RuntimeMountedVariantAttrs {
                                docstring: Some("Runtime variant docs".to_string()),
                            },
                        )],
                    }],
                    witnesses: Vec::new(),
                },
            ],
        };

        let (_, stubs) = enrich_runtime_mount("app", &[Name::new("app")], package)
            .expect("runtime mount enriches");
        let sources = stubs
            .into_iter()
            .map(|(_, _, source)| source)
            .collect::<Vec<_>>();

        assert!(sources.iter().any(|source| {
            source.starts_with("/// Runtime class docs\nclass RuntimeClass {")
                && source.contains("  /// Runtime field docs\n  value string")
        }));
        assert!(sources.iter().any(|source| {
            source.starts_with("/// Runtime class docs\nclass ClassAlias {")
                && source.contains("  /// Runtime field docs\n  value string")
        }));
        assert!(sources.iter().any(|source| {
            source.starts_with("/// Runtime enum docs\nenum RuntimeState {")
                && source.contains("  /// Runtime variant docs\n  READY")
        }));
        assert!(sources.iter().any(|source| {
            source.starts_with("/// Runtime enum docs\nenum StateAlias {")
                && source.contains("  /// Runtime variant docs\n  READY")
        }));
    }
}
