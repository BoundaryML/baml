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
use baml_compiler2_emit::{CompileOptions, emit_units_with_stdlib};
use baml_compiler2_hir::{
    body::{BodyOwnerId, LetBody, let_body},
    contributions::Definition,
    package::PackageId,
};
use baml_compiler2_hir_ty::package_interface::package_interface;
use baml_project::{ProjectDatabase, collect_diagnostics};
use bex_engine::RuntimeCompiler;
use bex_vm_types::{
    InitTail, RuntimeCompileArtifact, RuntimeCompileDiagnostic, RuntimeCompileMode,
    RuntimeCompileRequest, RuntimeDiagnosticSeverity, RuntimePackageMount,
    RuntimeSessionCompileArtifact, RuntimeSessionCompileRequest, RuntimeSessionInitializer,
    RuntimeSessionStep, RuntimeSourceSpan, SessionVisibleKind, SessionVisibleSymbol,
    bytecode::Instruction,
    relink::{IndexOperand, visit_object_operands},
};
use indexmap::IndexMap;
use rowan::ast::AstNode;

type RuntimeLinkStub = (Vec<Name>, Name, String);
type EnrichedRuntimeMount = (Vec<u8>, Vec<RuntimeLinkStub>);

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

fn interface_contains_runtime_minted_name(interface: &baml_type::Interface) -> bool {
    interface.name.is_runtime_minted()
        || interface
            .generics
            .iter()
            .any(type_contains_runtime_minted_name)
        || interface
            .associated_types
            .iter()
            .any(|(_, ty)| type_contains_runtime_minted_name(ty))
}

/// Whether rendering `ty` as source would expose a hidden runtime-mint name.
///
/// Runtime-minted names can occur below otherwise source-spellable containers,
/// function types, or interface constraints, so this must inspect the complete
/// type rather than only its outer nominal reference.
fn type_contains_runtime_minted_name(ty: &baml_type::Ty) -> bool {
    use baml_type::Ty;

    match ty {
        Ty::Class(name, generics, _) => {
            name.is_runtime_minted() || generics.iter().any(type_contains_runtime_minted_name)
        }
        Ty::Interface(name, generics, associated_types, _) => {
            name.is_runtime_minted()
                || generics.iter().any(type_contains_runtime_minted_name)
                || associated_types
                    .iter()
                    .any(|(_, ty)| type_contains_runtime_minted_name(ty))
        }
        Ty::Enum(name, _) | Ty::EnumVariant(name, ..) | Ty::TypeAlias(name, _) => {
            name.is_runtime_minted()
        }
        Ty::List(inner, _) | Ty::EvolvingList(inner, _) => type_contains_runtime_minted_name(inner),
        Ty::Map { key, value, .. } | Ty::EvolvingMap(key, value, _) => {
            type_contains_runtime_minted_name(key) || type_contains_runtime_minted_name(value)
        }
        Ty::Union(members, _) => members.iter().any(type_contains_runtime_minted_name),
        Ty::Function {
            params,
            ret,
            throws,
            ..
        } => {
            params
                .iter()
                .any(|param| type_contains_runtime_minted_name(&param.ty))
                || type_contains_runtime_minted_name(ret)
                || type_contains_runtime_minted_name(throws)
        }
        Ty::Future(value, throws, _) => {
            type_contains_runtime_minted_name(value) || type_contains_runtime_minted_name(throws)
        }
        Ty::AssociatedTypeProjection {
            base, interface, ..
        } => {
            type_contains_runtime_minted_name(base)
                || interface_contains_runtime_minted_name(interface)
        }
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
        | Ty::BuiltinUnknown { .. }
        | Ty::Never { .. }
        | Ty::Unknown { .. }
        | Ty::Error { .. }
        | Ty::Infer { .. } => false,
    }
}

fn enrich_runtime_mount(
    alias: &str,
    mut package: RuntimePackageMount,
) -> Result<EnrichedRuntimeMount, RuntimeCompileDiagnostic> {
    use baml_compiler2_hir_ty::package_interface::{
        ExportedFieldAttrs, ExportedFunction, ExportedImpl, ExportedImplOrigin, ExportedType,
        PackageInterface,
    };

    fn source_identifier(name: &Name) -> bool {
        let mut chars = name.as_str().chars();
        chars
            .next()
            .is_some_and(|ch| ch.is_ascii_alphabetic() || ch == '_')
            && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
    }

    fn relocate_function(
        function: &mut ExportedFunction,
        alias: &Name,
        stubs: &mut Vec<(Vec<Name>, Name, String)>,
    ) {
        use baml_compiler2_hir_ty::callable::{ExternalCallTarget, ExternalLinkability};
        if !matches!(function.linkability, ExternalLinkability::Linkable) {
            return;
        }
        let (namespace, name) = match &mut function.target {
            ExternalCallTarget::Free {
                package,
                namespace,
                name,
            } => {
                *package = alias.clone();
                (namespace.clone(), name.clone())
            }
            ExternalCallTarget::Method {
                package,
                namespace,
                class,
                name,
            } => {
                *package = alias.clone();
                let mut target_namespace = namespace.clone();
                target_namespace.push(class.clone());
                (target_namespace, name.clone())
            }
            ExternalCallTarget::Interface { interface, method } => {
                let mut target_namespace = interface.namespace().clone();
                target_namespace.push(interface.name().clone());
                *interface = baml_type::QualifiedTypeName::new(
                    alias.clone(),
                    interface.namespace().clone(),
                    interface.name().clone(),
                );
                (target_namespace, method.clone())
            }
        };
        // Compiler-generated init/test helpers are not part of the mounted
        // callable ABI. They can carry internal-only signature forms that are
        // intentionally not source-spellable, and no consumer can name them.
        if name.as_str().starts_with('$') {
            return;
        }
        let generic_params = function
            .generic_params
            .iter()
            .filter(|param| !baml_type::is_synthetic_effect_param(param.name()))
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        let generic_suffix = if generic_params.is_empty() {
            String::new()
        } else {
            format!("<{}>", generic_params.join(", "))
        };
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
            // The link stub preserves only the may-throw ABI bit. Named error
            // types retain the dependency package's nominal identity in the
            // mounted interface and are not necessarily source-spellable from
            // this synthetic alias package.
            " throws unknown".to_string()
        };
        // Mounted inference owns the real return type. Hide it only when its
        // source spelling would expose a hidden runtime-minted name and produce
        // diagnostics in a phantom `runtime_mount_*` file.
        let return_type = if type_contains_runtime_minted_name(&function.return_type) {
            "unknown".to_string()
        } else {
            function.return_type.to_string()
        };
        let source = format!(
            "function {name}{generic_suffix}({params}) -> {return_type}{throws} {{ $rust_function }}\n"
        );
        stubs.push((namespace, name, source));
    }

    let mut interface =
        borsh::from_slice::<PackageInterface>(&package.interface_blob).map_err(|error| {
            RuntimeCompileDiagnostic {
                code: "E_RUNTIME_INTERFACE".to_string(),
                message: error.to_string(),
                severity: RuntimeDiagnosticSeverity::Error,
                span: None,
            }
        })?;
    // A package object may be mounted under any source-visible alias. Its
    // exported call targets retain the package's original identity in the
    // persisted interface, so relocate those symbolic link names to the alias.
    // Runtime linking resolves the alias back to the live dependency object.
    let alias = Name::new(alias);
    let mut stubs = Vec::new();
    for function in interface
        .functions
        .values_mut()
        .flat_map(|namespace| namespace.values_mut())
    {
        relocate_function(function, &alias, &mut stubs);
    }
    for (export_namespace, exported_types) in &mut interface.types {
        for (export_name, exported) in exported_types {
            match exported {
                ExportedType::Class {
                    fields,
                    methods,
                    generic_params,
                    ..
                } => {
                    // The emitter needs a concrete class object in the mounted
                    // package's discarded source units so a consumer literal
                    // decomposes to an external object reference. At runtime
                    // that reference is resolved to the dependency's actual
                    // class object, preserving a mounted runtime mint instead
                    // of allocating a structurally-similar local class.
                    if source_identifier(export_name)
                        && export_namespace.iter().all(source_identifier)
                        && fields.iter().all(|(name, ..)| source_identifier(name))
                    {
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
                        for (field, ..) in fields.iter() {
                            // Mounted inference owns the exact field type. The
                            // source stub exists only to allocate/link the
                            // positional class object, so `unknown` avoids
                            // re-spelling hidden runtime-qualified names.
                            writeln!(&mut source, "  {field} unknown")
                                .expect("writing to String is infallible");
                        }
                        source.push_str("}\n");
                        stubs.push((export_namespace.clone(), export_name.clone(), source));
                    }
                    for function in methods {
                        relocate_function(function, &alias, &mut stubs);
                    }
                }
                ExportedType::Interface {
                    qtn,
                    generic_params,
                    associated_types,
                    fields,
                    required_methods,
                    default_methods,
                    ..
                } => {
                    for function in required_methods.iter_mut().chain(default_methods) {
                        relocate_function(function, &alias, &mut stubs);
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
                    for (field, ty, _) in fields {
                        // Keep ordinary source-spellable ABI intact, but avoid
                        // exposing a runtime-minted name from any nested type.
                        let ty = if type_contains_runtime_minted_name(ty) {
                            "unknown".to_string()
                        } else {
                            ty.to_string()
                        };
                        writeln!(&mut source, "  {field}: {ty}")
                            .expect("writing to String is infallible");
                    }
                    source.push_str("}\n");
                    stubs.push((namespace, name, source));
                }
                ExportedType::Enum { variants, .. } => {
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
                ExportedType::TypeAlias { .. } => {}
            }
        }
    }
    for implementation in &mut interface.impls {
        for function in &mut implementation.methods {
            relocate_function(function, &alias, &mut stubs);
        }
    }
    for mount in package.types.drain(..) {
        let root_types = interface.types.entry(Vec::new()).or_default();
        if root_types.contains_key(&mount.export_name) {
            return Err(RuntimeCompileDiagnostic {
                code: "E0011".to_string(),
                message: format!("duplicate exported type name `{}`", mount.export_name),
                severity: RuntimeDiagnosticSeverity::Error,
                span: None,
            });
        }

        let class_rows = mount
            .classes
            .iter()
            .map(|class| {
                (
                    class.qtn.clone(),
                    ExportedType::Class {
                        qtn: class.qtn.clone(),
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
                                    },
                                )
                            })
                            .collect(),
                        methods: Vec::new(),
                        generic_params: Vec::new(),
                        generic_param_bounds: Vec::new(),
                    },
                )
            })
            .collect::<Vec<_>>();
        let enum_rows = mount
            .enums
            .iter()
            .map(|enm| {
                (
                    enm.qtn.clone(),
                    ExportedType::Enum {
                        qtn: enm.qtn.clone(),
                        variants: enm.variants.clone(),
                    },
                )
            })
            .collect::<Vec<_>>();

        // Hidden runtime names remain the lowering identity. They are also
        // indexed structurally so field/member lookup can follow recursive or
        // mutually-referential minted definitions after `app.Export` lowers to
        // its internal qtn.
        for (qtn, row) in class_rows.iter().chain(&enum_rows) {
            interface.namespaces.insert(qtn.namespace().clone());
            interface
                .types
                .entry(qtn.namespace().clone())
                .or_default()
                .entry(qtn.name().clone())
                .or_insert_with(|| row.clone());
        }

        let root_ty = baml_type::Ty::from(&mount.ty);
        let exported = match &mount.ty {
            baml_type::RealizedTy::Class(qtn, _, _) => class_rows
                .iter()
                .find(|(candidate, _)| candidate == qtn)
                .map(|(_, row)| row.clone()),
            baml_type::RealizedTy::Enum(qtn, _) => enum_rows
                .iter()
                .find(|(candidate, _)| candidate == qtn)
                .map(|(_, row)| row.clone()),
            _ => Some(ExportedType::TypeAlias {
                qtn: mount.identity_name.clone(),
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
        match &exported {
            ExportedType::Class {
                fields,
                generic_params,
                ..
            } if source_identifier(&mount.export_name)
                && fields.iter().all(|(name, ..)| source_identifier(name)) =>
            {
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
                let mut source = format!("class {}{generic_suffix} {{\n", mount.export_name);
                for (field, ..) in fields {
                    writeln!(&mut source, "  {field} unknown")
                        .expect("writing to String is infallible");
                }
                source.push_str("}\n");
                stubs.push((Vec::new(), mount.export_name.clone(), source));
            }
            ExportedType::Enum { variants, .. }
                if source_identifier(&mount.export_name)
                    && variants.iter().all(source_identifier) =>
            {
                let mut source = format!("enum {} {{\n", mount.export_name);
                for variant in variants {
                    writeln!(&mut source, "  {variant}").expect("writing to String is infallible");
                }
                source.push_str("}\n");
                stubs.push((Vec::new(), mount.export_name.clone(), source));
            }
            ExportedType::Class { .. }
            | ExportedType::Enum { .. }
            | ExportedType::Interface { .. }
            | ExportedType::TypeAlias { .. } => {}
        }
        interface
            .types
            .entry(Vec::new())
            .or_default()
            .insert(mount.export_name.clone(), exported);

        for (witness, field_links) in mount.witnesses {
            interface.impls.push(ExportedImpl {
                interface: witness.clone(),
                for_ty_pattern: root_ty.clone(),
                generic_params: Vec::new(),
                param_bounds: Vec::new(),
                associated_types: witness.associated_types.clone(),
                field_links,
                origin: ExportedImplOrigin::OutOfBody,
                methods: Vec::new(),
            });
        }
    }
    stubs.sort();
    stubs.dedup();
    borsh::to_vec(&interface)
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
        if symbol.kind == SessionVisibleKind::TypeBinding
            && let Some(type_value) = &symbol.type_value
        {
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
                    && matches!(token.text.as_str(), "json" | "reflect" | "type");
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
    body.root_expr.and_then(|root| {
        inference
            .type_of_expr
            .get(&root)
            .map(baml_type::interned::Ty::to_plain)
    })
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
                type_value: None,
            };
            declaration_name_ranges.insert(range, symbol.internal.clone());
            declarations.insert(name, symbol);
        }
    }

    let mut declaration_mapping = request
        .visible
        .iter()
        .filter(|(_, symbol)| symbol.kind != SessionVisibleKind::Let)
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
        .filter(|(_, symbol)| symbol.kind == SessionVisibleKind::TypeBinding)
        .map(|(name, symbol)| (name.clone(), symbol.clone()))
        .collect::<IndexMap<_, _>>();
    let mut generated = declaration_source.clone();
    let mut steps = Vec::new();

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
                kind: SessionVisibleKind::TypeBinding,
                type_value: Some(backing_name.clone()),
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
                    type_value: None,
                };
                binding = Some((name, symbol));
                internal_name
            } else {
                format!("__baml_session_{sequence}_stmt_{index}")
            };
            let assignment = is_statement
                .then(|| assignment_parts(raw))
                .flatten()
                .and_then(|(target, operator, rhs)| {
                    let symbol = request.visible.get(target)?;
                    (symbol.kind == SessionVisibleKind::Let).then_some((symbol, operator, rhs))
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
                let target_local = format!("__baml_session_{sequence}_target_{index}");
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
        steps.push(RuntimeSessionStep {
            global: format!("user.{generated_name}"),
            commit_global,
            binding: binding.clone(),
            replay_source: binding.as_ref().map(|_| step_source),
            returns_value,
        });
        if let Some((name, symbol)) = binding {
            visible_mapping.insert(name.clone(), symbol.internal.clone());
            if symbol.kind == SessionVisibleKind::TypeBinding {
                active_type_bindings.insert(name, symbol);
            }
        }
    }

    if !steps.iter().any(|step| step.returns_value) {
        let generated_name = format!("__baml_session_{sequence}_result");
        let step_source = format!("let {generated_name} = null\n");
        generated.push_str(&step_source);
        steps.push(RuntimeSessionStep {
            global: format!("user.{generated_name}"),
            commit_global: None,
            binding: None,
            replay_source: None,
            returns_value: true,
        });
    }
    let result_global = steps
        .iter()
        .find(|step| step.returns_value)
        .expect("session lowering always has a result")
        .global
        .clone();
    Ok(LoweredSession {
        source: generated,
        artifact: RuntimeSessionCompileArtifact {
            submission_name: request.submission_name.clone(),
            declaration_source,
            declarations,
            steps,
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
        let (files, mut session_artifact, result_contract, session_lease, is_session) = match mode {
            RuntimeCompileMode::Package => (files, None, None, None, false),
            RuntimeCompileMode::Session(session) => {
                let session = *session;
                let lowered = lower_session_submission(&session)?;
                let mut files = session.history.clone();
                files.insert(session.submission_name.clone(), lowered.source);
                (
                    files,
                    Some(lowered.artifact),
                    Some((lowered.result_global, session.expected.clone())),
                    Some(session.lease),
                    true,
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
        let mut db = ProjectDatabase::new();
        db.set_project_root_with_precompiled_stdlib(Path::new(RUNTIME_VIRTUAL_ROOT));
        let enriched = packages
            .into_iter()
            .map(|(name, package)| {
                enrich_runtime_mount(&name, package).map(|(blob, stubs)| (name, blob, stubs))
            })
            .collect::<Result<Vec<_>, _>>()
            .map_err(|diagnostic| vec![diagnostic])?;
        let mounted = enriched
            .iter()
            .map(|(name, blob, _)| (name.clone(), blob.clone()))
            .collect::<BTreeMap<_, _>>();
        let precompiled_stdlib_names = stdlib.interfaces.keys().cloned().collect::<Vec<_>>();
        db.set_mounted_packages(mounted);
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
            if baml_compiler2_hir::package::is_reserved_package_name(alias) {
                continue;
            }
            for (stub_index, (namespace, _name, source)) in stubs.iter().enumerate() {
                let path = runtime_mount_virtual_path(alias, namespace, mount_index, stub_index);
                db.add_compiler2_virtual_file(path, source);
            }
        }
        for (path, source) in files {
            // Runtime input names are package-relative. Mounting them beneath
            // the synthetic root makes `ns_foo/` namespace derivation behave
            // exactly like an ordinary project without exposing the synthetic
            // prefix in diagnostics.
            let path = runtime_source_virtual_path(&path);
            if is_session {
                db.add_session_file(path, &source);
            } else {
                db.add_file(path, &source);
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

        if let Some((result_global, expected)) = &result_contract
            && !matches!(expected, baml_type::RuntimeTy::BuiltinUnknown { .. })
        {
            let result_name = result_global
                .strip_prefix("user.")
                .unwrap_or(result_global.as_str());
            let actual =
                let_initializer_type(&db, result_name).unwrap_or_else(baml_type::Ty::unknown);
            let expected = baml_type::Ty::from(expected.clone());
            let context = baml_compiler2_hir_ty::facts::Facts::new(&db);
            if !baml_type::normalize::is_subtype(&actual, &expected, &context) {
                let file = session_artifact
                    .as_ref()
                    .map_or("$submission.baml", |artifact| {
                        artifact.submission_name.as_str()
                    });
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
        let interface_blob = borsh::to_vec(interface).map_err(|error| {
            vec![RuntimeCompileDiagnostic {
                code: "E_RUNTIME_INTERFACE".to_string(),
                message: error.to_string(),
                severity: RuntimeDiagnosticSeverity::Error,
                span: None,
            }]
        })?;
        let options = CompileOptions {
            emit_test_cases: crate::precompiled_stdlib_config::EMIT_TEST_CASES,
        };
        let emitted = emit_units_with_stdlib(
            &db,
            &options,
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
                    && session_artifact
                        .as_ref()
                        .is_none_or(|session| unit.source_file == session.submission_name)
            })
            .collect();
        if let Some(session) = session_artifact.as_mut() {
            let mut initializers = Vec::new();
            for unit in &mut units {
                if let Some(tail) = unit.init_tail.take() {
                    let (tail, retained) = prune_session_init_tail(&tail, &session.submission_name)
                        .map_err(|message| {
                            vec![RuntimeCompileDiagnostic {
                                code: "E_RUNTIME_SESSION_INIT".to_string(),
                                message,
                                severity: RuntimeDiagnosticSeverity::Error,
                                span: None,
                            }]
                        })?;
                    unit.init_tail = Some(tail);
                    initializers.extend(retained);
                }
            }
            session.initializers = initializers;
        }
        Ok(RuntimeCompileArtifact {
            units,
            interface_blob,
            diagnostics,
            session: session_artifact,
            session_lease,
        })
    }
}

#[cfg(test)]
mod tests {
    use baml_compiler2_hir::file_package::file_package;

    use super::*;

    #[test]
    fn runtime_minted_name_detection_is_recursive() {
        let ordinary = baml_type::Ty::List(
            Box::new(baml_type::Ty::Class(
                baml_type::QualifiedTypeName::local(Name::new("SourceClass")),
                Vec::new(),
                baml_type::TyAttr::default(),
            )),
            baml_type::TyAttr::default(),
        );
        assert!(!type_contains_runtime_minted_name(&ordinary));

        let nested_mint = baml_type::Ty::List(
            Box::new(baml_type::Ty::Class(
                baml_type::QualifiedTypeName::runtime_local(Name::new("RuntimeMinted"), 7),
                Vec::new(),
                baml_type::TyAttr::default(),
            )),
            baml_type::TyAttr::default(),
        );
        assert!(type_contains_runtime_minted_name(&nested_mint));
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
        db.set_project_root(Path::new(RUNTIME_VIRTUAL_ROOT));

        let source = db.add_file(
            runtime_source_virtual_path(r"ns_tools\ns_nested\main.baml"),
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

        let mount = db.add_compiler2_virtual_file(
            runtime_mount_virtual_path("app", &[Name::new("tools"), Name::new("nested")], 0, 0),
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
}
