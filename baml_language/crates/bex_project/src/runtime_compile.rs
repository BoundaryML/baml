//! Concrete runtime compiler assembled above the engine/compiler dependency
//! boundary.

use std::{
    collections::HashSet,
    fmt::Write as _,
    path::{Path, PathBuf},
    sync::Arc,
};

use baml_base::Name;
use baml_compiler_diagnostics::{
    DiagnosticId, DiagnosticIdentifierKind, DiagnosticMessageHighlight, DiagnosticMessageKind,
    DiagnosticPhase, ParseError, Severity,
};
use baml_compiler_lexer::{TokenKind, lex_lossless};
use baml_compiler_syntax::{BlockElement, BlockExpr, SyntaxKind, SyntaxNode};
use baml_compiler2_emit::{OptLevel, emit_package, emit_session_submission};
use baml_compiler2_hir::{
    body::{BodyOwnerId, LetBody, let_body},
    contributions::Definition,
};
use baml_compiler2_hir_ty::package_interface::{
    PackageInterface, WireInterface, export_interface, interface_digest,
};
use baml_db::{Dependency, ProjectDatabase, SourceRootSpec, collect_diagnostics};
use baml_type::{
    TypeName, WireName,
    wire::{DepSlot, Digest, Locator},
};
use bex_engine::RuntimeCompiler;
use bex_vm::{
    ArtifactKind, RuntimeCompileArtifact, RuntimeSessionCompileArtifact, RuntimeSessionStep,
    RuntimeSessionStepKind,
};
use bex_vm_types::{
    RuntimeCompileDiagnostic, RuntimeCompileMode, RuntimeCompileRequest,
    RuntimeDiagnosticAnnotation, RuntimeDiagnosticDetails, RuntimeDiagnosticHighlight,
    RuntimeDiagnosticHighlightKind, RuntimeDiagnosticPhase, RuntimeDiagnosticRelatedInfo,
    RuntimeDiagnosticSeverity, RuntimePackageMount, RuntimeSessionCompileRequest,
    RuntimeSourceSpan, SessionVisibleKind, SessionVisibleSymbol, types::LocalName,
};
use indexmap::IndexMap;
use rowan::ast::AstNode;

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

/// Hide the synthetic runtime root in diagnostics using virtual-path rules.
fn runtime_relative_virtual_path(path: &Path) -> String {
    let normalized = path.to_string_lossy().replace('\\', "/");
    normalized
        .strip_prefix(RUNTIME_VIRTUAL_ROOT)
        .map(|path| path.strip_prefix('/').unwrap_or(path))
        .unwrap_or(&normalized)
        .to_string()
}

/// A mount-boundary failure as a diagnostic.
fn mount_diagnostic(code: &str, message: String) -> RuntimeCompileDiagnostic {
    RuntimeCompileDiagnostic {
        code: code.to_string(),
        message,
        severity: RuntimeDiagnosticSeverity::Error,
        span: None,
        details: None,
    }
}

/// Mount creation order: every package after the packages its edges reach
/// (Kahn's algorithm over the request's identity edges, request order among
/// the ready). `Err` names the offending edge: one reaching a package the
/// request omits, or the edges of a package inside a cycle.
fn mount_order(
    packages: &IndexMap<bex_vm_types::RuntimePackageIdentity, RuntimePackageMount>,
) -> Result<Vec<bex_vm_types::RuntimePackageIdentity>, String> {
    let references: Vec<Vec<usize>> = packages
        .iter()
        .enumerate()
        .map(|(index, (_, mount))| {
            mount
                .edges
                .iter()
                .map(|(name, edge)| {
                    packages.get_index_of(&edge.target).ok_or_else(|| {
                        format!("package {index}'s edge `{name}` reaches one the request omits")
                    })
                })
                .collect::<Result<Vec<_>, _>>()
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut placed = vec![false; packages.len()];
    let mut order = Vec::with_capacity(packages.len());
    while order.len() < packages.len() {
        let next = (0..packages.len())
            .find(|&i| !placed[i] && references[i].iter().all(|&dep| placed[dep]));
        match next {
            Some(i) => {
                placed[i] = true;
                order.push(
                    *packages
                        .get_index(i)
                        .map(|(id, _)| id)
                        .unwrap_or_else(|| unreachable!("the index came from the map")),
                );
            }
            None => {
                let stuck = (0..packages.len())
                    .filter(|&i| !placed[i])
                    .map(|i| {
                        let edges = packages
                            .get_index(i)
                            .map(|(_, mount)| mount.edges.keys().cloned().collect::<Vec<_>>())
                            .unwrap_or_default()
                            .join(", ");
                        format!("package {i} (edges: {edges})")
                    })
                    .collect::<Vec<_>>()
                    .join("; ");
                return Err(format!("packages reach each other in a cycle: {stuck}"));
            }
        }
    }
    Ok(order)
}

/// The wire interface of a package that had no compile, from its projected
/// surface (`bex_vm_types::RuntimeProjectedSurface`): its own class, enum
/// and alias rows, its impl rows, the declarations it re-exports by name,
/// and — for every re-exported edge — every export of the target, by the
/// target's path (`built` holds the targets' rows; the order the roots are
/// created in guarantees it). A name exported twice is E0011.
///
/// The surface spells every foreign head through one of the mount's own
/// edges (or a language package's fixed name), so the blob's dependency
/// table is exactly those edges: every non-prelude edge a direct entry
/// carrying its target's digest (`digest_of`), a language package a prelude
/// entry — the same table an export of a compiled package builds.
fn projected_interface(
    surface: &bex_vm_types::RuntimeProjectedSurface,
    edges: &IndexMap<Name, bex_vm_types::RuntimeMountEdge>,
    built: &IndexMap<bex_vm_types::RuntimePackageIdentity, PackageInterface<WireName>>,
    digest_of: &dyn Fn(bex_vm_types::RuntimePackageIdentity) -> Digest,
) -> Result<WireInterface, RuntimeCompileDiagnostic> {
    use baml_compiler2_hir_ty::package_interface::{
        ExportedFieldAttrs, ExportedImpl, ExportedImplOrigin, ExportedType, ReExport,
    };

    let mut interface = PackageInterface::<TypeName> {
        types: IndexMap::new(),
        functions: IndexMap::new(),
        throw_sets: baml_compiler2_hir_ty::package_interface::FunctionThrowSets::default(),
        namespaces: std::collections::BTreeSet::default(),
        impls: Vec::new(),
        reexports: IndexMap::new(),
    };
    let duplicate = |name: &LocalName| {
        mount_diagnostic("E0011", format!("duplicate exported type name `{name}`"))
    };
    let exported = |interface: &PackageInterface<TypeName>, name: &LocalName| {
        interface.lookup_type(&name.namespace, &name.name).is_some()
            || interface
                .lookup_function(&name.namespace, &name.name)
                .is_some()
            || interface
                .lookup_reexport(&name.namespace, &name.name)
                .is_some()
    };
    let add_row = |interface: &mut PackageInterface<TypeName>,
                   name: &LocalName,
                   row: ExportedType<TypeName>|
     -> Result<(), RuntimeCompileDiagnostic> {
        if exported(interface, name) {
            return Err(duplicate(name));
        }
        interface.namespaces.insert(name.namespace.clone());
        interface
            .types
            .entry(name.namespace.clone())
            .or_default()
            .insert(name.name.clone(), row);
        Ok(())
    };
    for class in &surface.classes {
        let name = LocalName {
            namespace: Vec::new(),
            name: class.name.clone(),
        };
        add_row(
            &mut interface,
            &name,
            ExportedType::Class {
                qtn: baml_type::QualifiedTypeName::local(class.name.clone()),
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
        )?;
    }
    for enm in &surface.enums {
        let name = LocalName {
            namespace: Vec::new(),
            name: enm.name.clone(),
        };
        add_row(
            &mut interface,
            &name,
            ExportedType::Enum {
                qtn: baml_type::QualifiedTypeName::local(enm.name.clone()),
                variants: enm.variants.iter().map(|(name, _)| name.clone()).collect(),
            },
        )?;
    }
    for alias in &surface.aliases {
        add_row(
            &mut interface,
            &alias.name,
            ExportedType::TypeAlias {
                qtn: baml_type::QualifiedTypeName::qualified(
                    baml_type::Package::Local,
                    alias.name.namespace.clone(),
                    alias.name.name.clone(),
                ),
                resolved: alias.ty.clone(),
            },
        )?;
    }
    let add_reexport = |interface: &mut PackageInterface<TypeName>,
                        name: &LocalName,
                        reexport: ReExport<TypeName>|
     -> Result<(), RuntimeCompileDiagnostic> {
        if exported(interface, name) {
            return Err(duplicate(name));
        }
        interface.namespaces.insert(name.namespace.clone());
        interface
            .reexports
            .entry(name.namespace.clone())
            .or_default()
            .insert(name.name.clone(), reexport);
        Ok(())
    };
    for reexport in &surface.reexports {
        let of = reexport.of.clone();
        let entry = match reexport.kind {
            bex_vm_types::RuntimeReExportKind::Class
            | bex_vm_types::RuntimeReExportKind::Enum
            | bex_vm_types::RuntimeReExportKind::Interface
            | bex_vm_types::RuntimeReExportKind::TypeAlias => ReExport::Type(of),
        };
        add_reexport(&mut interface, &reexport.name, entry)?;
    }
    // Every export of a re-exported edge's target, by the target's own path
    // — its rows and its re-exports alike; a re-export of a re-export is
    // followed at resolution.
    for (edge, mount_edge) in edges {
        if mount_edge.kind != bex_vm_types::types::EdgeKind::ReExported {
            continue;
        }
        let target = built.get(&mount_edge.target).ok_or_else(|| {
            mount_diagnostic(
                "E_RUNTIME_MOUNT",
                "a re-exported package is mounted after the package re-exporting it".to_string(),
            )
        })?;
        let spell = |namespace: &[Name], name: &Name| {
            TypeName::new(edge.clone(), namespace.to_vec(), name.clone())
        };
        for (namespace, items) in &target.types {
            for name in items.keys() {
                add_reexport(
                    &mut interface,
                    &LocalName {
                        namespace: namespace.clone(),
                        name: name.clone(),
                    },
                    ReExport::Type(spell(namespace, name)),
                )?;
            }
        }
        for (namespace, items) in &target.functions {
            for name in items.keys() {
                add_reexport(
                    &mut interface,
                    &LocalName {
                        namespace: namespace.clone(),
                        name: name.clone(),
                    },
                    ReExport::Function(spell(namespace, name)),
                )?;
            }
        }
        for (namespace, items) in &target.reexports {
            for (name, reexport) in items {
                let entry = match reexport {
                    ReExport::Type(_) => ReExport::Type(spell(namespace, name)),
                    ReExport::Function(_) => ReExport::Function(spell(namespace, name)),
                };
                add_reexport(
                    &mut interface,
                    &LocalName {
                        namespace: namespace.clone(),
                        name: name.clone(),
                    },
                    entry,
                )?;
            }
        }
    }
    for rule in &surface.impls {
        interface.impls.push(ExportedImpl {
            // The bindings are the row's `associated_types`; the header
            // names the interface and its arguments only, exactly as a
            // source impl's does.
            associated_types: rule.interface.associated_types.to_vec(),
            interface: baml_type::Interface::new(
                rule.interface.name.clone(),
                rule.interface.generics.clone(),
                Box::new([]),
            ),
            for_ty_pattern: rule.for_ty.clone(),
            generic_params: Vec::new(),
            param_bounds: Vec::new(),
            field_links: rule.field_links.clone(),
            origin: ExportedImplOrigin::OutOfBody,
            methods: Vec::new(),
        });
    }
    // Every edge of the mount is a slot, in edge order; a language package
    // the surface names by its fixed name — the engine spells the prelude by
    // its declared names, never through an edge — joins the table on demand.
    let mut table: Vec<(Name, Locator)> = edges
        .iter()
        .map(|(edge, mount_edge)| {
            let locator = match mount_edge.kind {
                bex_vm_types::types::EdgeKind::Prelude => Locator::Prelude { edge: edge.clone() },
                bex_vm_types::types::EdgeKind::Declared
                | bex_vm_types::types::EdgeKind::ReExported
                | bex_vm_types::types::EdgeKind::Anonymous => Locator::Direct {
                    edge: edge.clone(),
                    digest: digest_of(mount_edge.target),
                },
            };
            (edge.clone(), locator)
        })
        .collect();
    let rows = interface.try_map_heads(&mut |name: &TypeName| {
        if name.is_local() {
            return Ok(WireName::own(name.namespace().clone(), name.name().clone()));
        }
        let edge = name.package();
        let index = match table.iter().position(|(slotted, _)| slotted == edge) {
            Some(index) => index,
            None if baml_builtins2::stdlib_package_names().contains(&edge.as_str()) => {
                table.push((edge.clone(), Locator::Prelude { edge: edge.clone() }));
                table.len() - 1
            }
            None => {
                return Err(mount_diagnostic(
                    "E_RUNTIME_MOUNT",
                    format!("the projected surface names `{name}` through no edge of the mount"),
                ));
            }
        };
        Ok(WireName::qualified(
            DepSlot::of_dependency_index(index),
            name.namespace().clone(),
            name.name().clone(),
        ))
    })?;
    Ok(WireInterface {
        dependencies: table.into_iter().map(|(_, locator)| locator).collect(),
        rows,
    })
}

/// The names the roots of a request take, made distinct: an aliased package
/// is named by its first alias; a package no alias reaches — one a mount's
/// edges reach — by the first edge name reaching it; either suffixed with
/// `$k` when the name is taken or is one of the package's OWN edge names (a
/// root's name resolves to the root itself, so it must not shadow an edge
/// its blob spells). A root's name is what the program spells its
/// declarations by where no edge reaches it; every declaration a consumer
/// can write is spelled through an edge or a re-export instead.
fn root_names(
    packages: &IndexMap<bex_vm_types::RuntimePackageIdentity, RuntimePackageMount>,
    aliases: &IndexMap<String, bex_vm_types::RuntimePackageIdentity>,
) -> IndexMap<bex_vm_types::RuntimePackageIdentity, Name> {
    let mut taken: HashSet<Name> = baml_builtins2::reserved_edge_names()
        .iter()
        .map(Name::new)
        .collect();
    let mut names: IndexMap<bex_vm_types::RuntimePackageIdentity, Name> = IndexMap::new();
    let mut claim = |taken: &mut HashSet<Name>,
                     identity: bex_vm_types::RuntimePackageIdentity,
                     preferred: &Name| {
        if names.contains_key(&identity) {
            return;
        }
        let own_edges = packages
            .get(&identity)
            .map(|mount| &mount.edges)
            .into_iter()
            .flat_map(|edges| edges.keys())
            .cloned()
            .collect::<HashSet<Name>>();
        let mut candidate = preferred.clone();
        let mut suffix = 1;
        while taken.contains(&candidate) || own_edges.contains(&candidate) {
            candidate = Name::new(format!("{preferred}${suffix}"));
            suffix += 1;
        }
        taken.insert(candidate.clone());
        names.insert(identity, candidate);
    };
    for (alias, identity) in aliases {
        claim(&mut taken, *identity, &Name::new(alias.as_str()));
    }
    for mount in packages.values() {
        for (edge, mount_edge) in &mount.edges {
            claim(&mut taken, mount_edge.target, edge);
        }
    }
    names
}

/// Stateless compiler provider. A fresh database is allocated inside every
/// [`RuntimeCompiler::compile`] call and dropped before the call returns.
#[derive(Debug, Default)]
struct ProjectRuntimeCompiler;

pub(crate) fn runtime_compiler() -> Arc<dyn RuntimeCompiler> {
    Arc::new(ProjectRuntimeCompiler)
}

/// `session` marks a submission compile, whose generated names are the only
/// ones worth demangling: a package compile has no generator prefixes, and
/// rewriting its diagnostics could only corrupt a user's own identifier that
/// happens to look like one.
fn owned_diagnostic(
    db: &ProjectDatabase,
    diagnostic: &baml_compiler_diagnostics::Diagnostic,
    session: bool,
) -> RuntimeCompileDiagnostic {
    fn owned_span(db: &ProjectDatabase, span: baml_base::Span) -> Option<RuntimeSourceSpan> {
        db.file_id_to_path(span.file_id)
            .map(|path| RuntimeSourceSpan {
                file: runtime_relative_virtual_path(path),
                start: usize::from(span.range.start()),
                end: usize::from(span.range.end()),
            })
    }

    fn highlight_kind(kind: DiagnosticMessageKind) -> RuntimeDiagnosticHighlightKind {
        match kind {
            DiagnosticMessageKind::Identifier(DiagnosticIdentifierKind::Type) => {
                RuntimeDiagnosticHighlightKind::IdentifierType
            }
            DiagnosticMessageKind::Identifier(DiagnosticIdentifierKind::Function) => {
                RuntimeDiagnosticHighlightKind::IdentifierFunction
            }
            DiagnosticMessageKind::Identifier(DiagnosticIdentifierKind::Field) => {
                RuntimeDiagnosticHighlightKind::IdentifierField
            }
            DiagnosticMessageKind::Identifier(DiagnosticIdentifierKind::Variable) => {
                RuntimeDiagnosticHighlightKind::IdentifierVariable
            }
            DiagnosticMessageKind::Identifier(DiagnosticIdentifierKind::EnumVariant) => {
                RuntimeDiagnosticHighlightKind::IdentifierEnumVariant
            }
            DiagnosticMessageKind::Identifier(DiagnosticIdentifierKind::Attribute) => {
                RuntimeDiagnosticHighlightKind::IdentifierAttribute
            }
            DiagnosticMessageKind::TypeExpression => RuntimeDiagnosticHighlightKind::TypeExpression,
            DiagnosticMessageKind::Code => RuntimeDiagnosticHighlightKind::Code,
        }
    }

    fn owned_text(
        text: &str,
        highlights: &[DiagnosticMessageHighlight],
        session: bool,
    ) -> (String, Vec<RuntimeDiagnosticHighlight>) {
        let rendered = if session {
            demangle_session_names(text)
        } else {
            text.to_string()
        };
        let highlights = highlights
            .iter()
            .filter_map(|highlight| {
                let start = usize::try_from(highlight.start).ok()?;
                let end = usize::try_from(highlight.end).ok()?;
                if start > end
                    || end > text.len()
                    || !text.is_char_boundary(start)
                    || !text.is_char_boundary(end)
                {
                    return None;
                }
                let (start, end) = if session {
                    (
                        demangle_session_names(&text[..start]).len(),
                        demangle_session_names(&text[..end]).len(),
                    )
                } else {
                    (start, end)
                };
                Some(RuntimeDiagnosticHighlight {
                    start: u32::try_from(start).ok()?,
                    end: u32::try_from(end).ok()?,
                    kind: highlight_kind(highlight.kind),
                })
            })
            .collect();
        (rendered, highlights)
    }

    let span = diagnostic
        .primary_span()
        .and_then(|span| owned_span(db, span));
    let (headline, message_highlights) =
        owned_text(&diagnostic.message, &diagnostic.message_highlights, session);
    let primary_label = diagnostic
        .annotations
        .iter()
        .find(|annotation| annotation.is_primary)
        .and_then(|annotation| annotation.message.as_deref())
        .map(|label| owned_text(label, &[], session).0);
    let annotations = diagnostic
        .annotations
        .iter()
        .filter_map(|annotation| {
            let span = owned_span(db, annotation.span)?;
            let (message, message_highlights) = annotation.message.as_deref().map_or_else(
                || (None, Vec::new()),
                |message| {
                    let (message, highlights) =
                        owned_text(message, &annotation.message_highlights, session);
                    (Some(message), highlights)
                },
            );
            Some(RuntimeDiagnosticAnnotation {
                span,
                message,
                message_highlights,
                is_primary: annotation.is_primary,
            })
        })
        .collect();
    let related_info = diagnostic
        .related_info
        .iter()
        .filter_map(|related| {
            let span = owned_span(db, related.span).or_else(|| {
                related.file_path.as_ref().map(|file| RuntimeSourceSpan {
                    file: file.clone(),
                    start: usize::from(related.span.range.start()),
                    end: usize::from(related.span.range.end()),
                })
            })?;
            let (message, message_highlights) =
                owned_text(&related.message, &related.message_highlights, session);
            Some(RuntimeDiagnosticRelatedInfo {
                span,
                message,
                message_highlights,
                file_path: related.file_path.clone(),
            })
        })
        .collect();
    RuntimeCompileDiagnostic {
        code: diagnostic.code().to_string(),
        message: if session {
            demangle_session_names(&diagnostic.message_with_primary_label())
        } else {
            diagnostic.message_with_primary_label().into_owned()
        },
        severity: match diagnostic.severity {
            Severity::Error => RuntimeDiagnosticSeverity::Error,
            Severity::Warning => RuntimeDiagnosticSeverity::Warning,
            Severity::Info => RuntimeDiagnosticSeverity::Info,
        },
        span,
        details: Some(Box::new(RuntimeDiagnosticDetails {
            headline,
            primary_label,
            phase: match diagnostic.phase {
                DiagnosticPhase::Parse => RuntimeDiagnosticPhase::Parse,
                DiagnosticPhase::Hir => RuntimeDiagnosticPhase::Hir,
                DiagnosticPhase::Validation => RuntimeDiagnosticPhase::Validation,
                DiagnosticPhase::Type => RuntimeDiagnosticPhase::Type,
            },
            message_highlights,
            annotations,
            related_info,
        })),
    }
}

/// Strip the session generator's internal prefixes from a message, so a
/// diagnostic quotes the name the author wrote (`T`) rather than the
/// per-submission global it was lowered to (`__baml_session_3_T`). A backing
/// type-value global has no user spelling at all; it reads as its binding.
fn demangle_session_names(message: &str) -> String {
    const PREFIX: &str = "__baml_session_";
    let mut out = String::with_capacity(message.len());
    let mut rest = message;
    while let Some(at) = rest.find(PREFIX) {
        out.push_str(&rest[..at]);
        let after = &rest[at + PREFIX.len()..];
        let digits = after.bytes().take_while(u8::is_ascii_digit).count();
        match after[digits..].strip_prefix('_') {
            Some(tail) if digits > 0 => {
                rest = tail.strip_prefix("type_value_").unwrap_or(tail);
            }
            _ => {
                out.push_str(PREFIX);
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
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
        details: None,
    }
}

fn session_parse_error_diagnostic(
    error: &ParseError,
    submission_name: &str,
    prefix_len: usize,
) -> RuntimeCompileDiagnostic {
    let (code, span, message) = match error {
        ParseError::UnexpectedToken {
            expected,
            found,
            span,
        } => (
            DiagnosticId::InvalidSyntax,
            *span,
            format!("unexpected token: expected {expected}, found {found}"),
        ),
        ParseError::UnexpectedEof { expected, span } => (
            DiagnosticId::InvalidSyntax,
            *span,
            format!("unexpected end of file: expected {expected}"),
        ),
        ParseError::InvalidSyntax { message, span }
        | ParseError::RemovedFeature { message, span } => {
            (DiagnosticId::InvalidSyntax, *span, message.clone())
        }
        ParseError::AmbiguousUnion { span, groupings } => (
            DiagnosticId::AmbiguousUnion,
            *span,
            format!(
                "{}: {}",
                ParseError::AMBIGUOUS_UNION_MESSAGE,
                ParseError::ambiguous_union_label(groupings)
            ),
        ),
    };
    let range = span.range;
    let start = usize::from(range.start()).saturating_sub(prefix_len);
    let end = usize::from(range.end()).saturating_sub(prefix_len);
    runtime_diagnostic(code, submission_name, start, end, message)
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
    /// The binding holding the submission's result (the artifact's result
    /// step commits it to a global under the same name).
    result_name: String,
    /// Each step's extent within `source`, parallel to `artifact.steps`.
    step_ranges: Vec<std::ops::Range<usize>>,
}

/// Everything a compile carries *because* it is a session.
///
/// One value rather than four parallel `Option`s that must agree: a package
/// compile has none of this, a session compile has all of it, and there is no
/// state in between for a reader to invent an answer for.
struct SessionCompile {
    artifact: RuntimeSessionCompileArtifact,
    /// The binding holding the submission's result, checked against
    /// `expected`: how the compiler, which names the workspace's items
    /// unqualified, looks the result up.
    result_name: String,
    /// The contract from `eval<T>`; unknown when the eval is uncontracted.
    expected: bex_vm_types::SessionContract,
    lease: bex_vm_types::SessionEvalLease,
}

/// The compile-time head an `eval<T>` contract's name denotes from the
/// submission's package: that package for a local name, the package one of
/// its edges names, else the package the compile world spells that way
/// within its dependency closure (a contract may name a type the submission
/// only reaches through a mount). `None` when the submission cannot reach it.
// BUG: the contract crosses the compile seam by NAME (the engine spells it
// with `TypeHead::to_name`), so it is read back by spelling. A session that
// mounts a package under another name than the one its declarations display,
// or a closure holding two packages that display alike, reads the contract
// as a different declaration than the one `eval<T>` was given. Every mount
// edge crosses this seam by identity (`RuntimeMountEdge`); the contract's
// heads should name their packages the same way.
#[deprecated = "the `eval<T>` contract should carry its heads' packages by identity \
                (`RuntimePackageIdentity`), like every mount edge; this reads them back by \
                name until it does"]
fn session_contract_head(
    db: &dyn baml_compiler2_hir::Db,
    workspace: baml_base::SourceRoot,
    name: &TypeName,
) -> Option<baml_type::DeclName> {
    use baml_compiler2_hir::package::{accessible_package, package_dependency_closure, spelling};

    let root = if name.is_local() {
        workspace
    } else if let Some(root) = accessible_package(db, workspace, name.package()) {
        root
    } else {
        let root = spelling(db).root(name.package())?;
        package_dependency_closure(db, workspace)
            .contains(&root)
            .then_some(root)?
    };
    Some(baml_type::DeclName::in_root(
        root,
        name.namespace().clone(),
        name.name().clone(),
    ))
}

fn let_initializer_type(
    db: &ProjectDatabase,
    package: baml_base::SourceRoot,
    name: &str,
) -> Option<baml_type::Ty> {
    let package_items = baml_compiler2_hir::package::package_items(db, package);
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

/// The level a runtime compile emits at.
const OPT_LEVEL: OptLevel = OptLevel::One;

/// The path of a submission's generated `let`: submissions sit at the
/// session package's root, so an item path is the bare generated name.
fn local_item(name: &str) -> LocalName {
    LocalName {
        namespace: Vec::new(),
        name: Name::new(name),
    }
}

/// `erase_steps` names steps (by index into the lowered `steps`) whose value
/// is typed by a session `type T = …` binding, as a first compile reported at
/// their block tails (E0172). Such a step publishes through the erasing helper
/// so its global carries `unknown` — the only type a binding-typed value may
/// leave its block as — while every other step keeps its precise type.
fn lower_session_submission(
    request: &RuntimeSessionCompileRequest,
    erase_steps: &HashSet<usize>,
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
        return Err(vec![session_parse_error_diagnostic(
            error,
            &request.submission_name,
            prefix.len(),
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
    // A call gives its argument a ground context: routing an erased step's
    // block through this helper is the ordinary road by which a binding-typed
    // value leaves as `unknown` (the same spelling a user writes with
    // `let v: unknown = { … }`), and needs no ascription on the step's own
    // `let`, which a session binding does not admit. Hoisted with the
    // declarations so a replayed step still finds it.
    let erase_helper = format!("__baml_erase_{sequence}");
    if !erase_steps.is_empty() {
        writeln!(
            declaration_source,
            "function {erase_helper}(v: unknown) -> unknown throws never {{ v }}"
        )
        .expect("writing to String is infallible");
    }
    let mut generated = declaration_source.clone();
    let mut steps = Vec::new();
    let mut step_ranges: Vec<std::ops::Range<usize>> = Vec::new();
    let mut result_step = None;
    let mut result_name: Option<String> = None;

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
        // Only the submission's final expression is its value. An earlier
        // one is a statement, as it would be in a block: its value is
        // dropped, so an `if` with no `else` there owes no unit branch.
        let returns_value =
            index + 1 == elements.len() && !is_outer_let && !is_statement && !has_semicolon;
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
                (source, Some(local_item(&target.internal)))
            } else {
                let rewritten = rewrite_identifiers(
                    raw,
                    &visible_mapping,
                    &forced,
                    &HashSet::new(),
                    &local_names,
                );
                // A step's value leaves its block into a global that later
                // submissions read, and a scoped `type T = …` binding lives
                // only inside that block: a value typed by one cannot leave
                // (E0172) unless the block's context is a ground type. A
                // step the first compile reported at its tail is re-lowered
                // through the erasing helper; see `lower_session_submission`.
                let block_step = |body: String| {
                    if erase_steps.contains(&steps.len()) {
                        format!("let {generated_name} = {erase_helper}({{\n{body}\n}})\n")
                    } else {
                        format!("let {generated_name} = {{\n{body}\n}}\n")
                    }
                };
                let source = if is_outer_let && prelude.is_empty() {
                    format!("{rewritten}\n")
                } else if is_outer_let {
                    block_step(format!("{prelude}{rewritten}\n{generated_name}"))
                } else if returns_value && prelude.is_empty() {
                    format!("let {generated_name} = ({rewritten})\n")
                } else if returns_value {
                    block_step(format!("{prelude}{rewritten}"))
                } else {
                    format!("let {generated_name} = {{\n{prelude}{rewritten}\nnull\n}}\n")
                };
                (source, None)
            };
            (generated_name, source, commit_global, binding)
        };
        let step_start = generated.len();
        generated.push_str(&step_source);
        step_ranges.push(step_start..generated.len());
        if returns_value {
            result_step = Some(steps.len());
            result_name = Some(generated_name.clone());
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
            global: local_item(&generated_name),
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
        let step_start = generated.len();
        generated.push_str(&step_source);
        step_ranges.push(step_start..generated.len());
        result_step = Some(steps.len());
        result_name = Some(generated_name.clone());
        steps.push(RuntimeSessionStep {
            global: local_item(&generated_name),
            commit_global: None,
            kind: RuntimeSessionStepKind::Expression,
        });
    }
    let result_step = result_step.expect("session lowering always has a result");
    let result_name = result_name.expect("session lowering always has a result");
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
        result_name,
        step_ranges,
    })
}

/// The steps a compile of `lowered` reported an E0172 inside: their value is
/// typed by a session binding and must publish as `unknown`. Diagnostics are
/// keyed by the submission's virtual path, in generated-source offsets.
fn steps_publishing_a_binding(
    diagnostics: &[RuntimeCompileDiagnostic],
    submission_name: &str,
    lowered: &LoweredSession,
) -> HashSet<usize> {
    let submission = runtime_relative_virtual_path(&runtime_source_virtual_path(submission_name));
    diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == DiagnosticId::ScopedTypeEscapesBlock.code())
        .filter_map(|diagnostic| diagnostic.span.as_ref())
        .filter(|span| span.file == submission)
        .filter_map(|span| {
            lowered
                .step_ranges
                .iter()
                .position(|range| range.contains(&span.start))
        })
        .collect()
}

impl RuntimeCompiler for ProjectRuntimeCompiler {
    fn compile(
        &self,
        request: RuntimeCompileRequest,
    ) -> Result<RuntimeCompileArtifact, Vec<RuntimeCompileDiagnostic>> {
        let RuntimeCompileRequest {
            files,
            packages,
            aliases,
            mode,
        } = request;
        let mut session_request = None;
        let mut lowered_session = None;
        let (files, mut session) = match mode {
            RuntimeCompileMode::Package => (files, None),
            RuntimeCompileMode::Session(session) => {
                let session = *session;
                let lowered = lower_session_submission(&session, &HashSet::new())?;
                let mut files = session.history.clone();
                files.insert(session.submission_name.clone(), lowered.source.clone());
                let compile = SessionCompile {
                    artifact: lowered.artifact.clone(),
                    result_name: lowered.result_name.clone(),
                    expected: session.expected.clone(),
                    lease: session.lease.clone(),
                };
                lowered_session = Some(lowered);
                session_request = Some(session);
                (files, Some(compile))
            }
        };
        let stdlib = crate::precompiled_stdlib::load().map_err(|message| {
            vec![RuntimeCompileDiagnostic {
                code: "E_RUNTIME_STDLIB".to_string(),
                message,
                severity: RuntimeDiagnosticSeverity::Error,
                span: None,
                details: None,
            }]
        })?;
        // This local is the transience guarantee: no handle to `db` occurs in
        // either return type, and all retained values below are deep-owned.
        // The stdlib arrives as precompiled interface blobs (never as source):
        // its roots are served from those interfaces and hold no files.
        let mut db = ProjectDatabase::new();
        db.ensure_precompiled_stdlib(&stdlib.interfaces);
        let workspace = db
            .add_source_root(SourceRootSpec::new(
                RUNTIME_VIRTUAL_ROOT,
                baml_base::SourceRootKind::Workspace,
            ))
            .unwrap_or_else(|e| unreachable!("fresh database accepts one workspace root: {e}"));
        // One root per package OBJECT, served from its interface (the
        // semantic authority) and holding no files, created after the
        // packages its edges reach so a re-export resolves to a root that
        // exists. A root is `Dynamic` (runtime-loaded), so it sorts after
        // every statically compiled root. The consumer reaches the packages
        // it mounted under its aliases; a package only an edge reaches has a
        // root all the same, since a consumer names a re-exported
        // declaration by the package it is defined in.
        let mount_error = |name: &Name, error: &dyn std::fmt::Display| {
            vec![mount_diagnostic(
                "E_RUNTIME_MOUNT",
                format!("cannot mount package `{name}`: {error}"),
            )]
        };
        let names = root_names(&packages, &aliases);
        let mut mount_roots: IndexMap<bex_vm_types::RuntimePackageIdentity, baml_base::SourceRoot> =
            IndexMap::new();
        let mut built: IndexMap<bex_vm_types::RuntimePackageIdentity, PackageInterface<WireName>> =
            IndexMap::new();
        for identity in mount_order(&packages)
            .map_err(|error| vec![mount_diagnostic("E_RUNTIME_MOUNT", error)])?
        {
            let mount = &packages[&identity];
            let name = names[&identity].clone();
            let (interface, blob) = match &mount.surface {
                bex_vm_types::RuntimeMountSurface::Compiled(blob) => {
                    let interface = baml_artifact::decode::<WireInterface>(
                        baml_artifact::ArtifactKind::PackageInterface,
                        blob,
                    )
                    .map_err(|error| mount_error(&name, &error))?;
                    (interface.rows, blob.clone())
                }
                bex_vm_types::RuntimeMountSurface::Projected(surface) => {
                    // Every edge's target is mounted already (`mount_order`),
                    // so its digest is the digest of the root serving it.
                    let interface =
                        projected_interface(surface, &mount.edges, &built, &|identity| {
                            interface_digest(&db, mount_roots[&identity])
                        })
                        .map_err(|diagnostic| vec![diagnostic])?;
                    let blob = baml_artifact::encode(
                        baml_artifact::ArtifactKind::PackageInterface,
                        &interface,
                    )
                    .map_err(|error| mount_error(&name, &error))?;
                    (interface.rows, blob)
                }
            };
            let edges = mount
                .edges
                .iter()
                .map(|(edge, mount_edge)| Dependency {
                    name: edge.clone(),
                    root: mount_roots[&mount_edge.target],
                })
                .collect();
            let root = db
                .add_source_root(
                    SourceRootSpec::new(
                        format!("{BUILTIN_VIRTUAL_ROOT}/mounts/{}", mount_roots.len()),
                        baml_base::SourceRootKind::Dynamic,
                    )
                    .named(name.clone())
                    .depending_on(edges)
                    .served_from(blob),
                )
                .map_err(|error| mount_error(&name, &error))?;
            mount_roots.insert(identity, root);
            built.insert(identity, interface);
        }
        for (alias, identity) in &aliases {
            let alias = Name::new(alias.as_str());
            db.add_dependency(
                workspace,
                Dependency {
                    name: alias.clone(),
                    root: mount_roots[identity],
                },
            )
            .map_err(|error| mount_error(&alias, &error))?;
        }
        let mut submission_file = None;
        for (path, source) in files {
            // Runtime input names are package-relative. Mounting them beneath
            // the synthetic root makes `ns_foo/` namespace derivation behave
            // exactly like an ordinary project without exposing the synthetic
            // prefix in diagnostics.
            let virtual_path = runtime_source_virtual_path(&path);
            match &session {
                Some(compile) => {
                    let file = db.add_session_file_in(workspace, virtual_path, &source);
                    if path == compile.artifact.submission_name {
                        submission_file = Some(file);
                    }
                }
                None => {
                    db.add_or_update_file_in(workspace, &virtual_path, &source);
                }
            }
        }

        let mut diagnostics: Vec<_> = collect_diagnostics(&db)
            .iter()
            .map(|diagnostic| owned_diagnostic(&db, diagnostic, session_request.is_some()))
            .collect();
        // A step whose value is typed by a session binding cannot leave its
        // block as written; only the compiler knows which steps those are, so
        // the first compile finds them (E0172 at the step) and one re-lower
        // publishes them as `unknown`. Every other diagnostic stands.
        //
        // ONE round is enough because erasing a step only ever REMOVES the
        // report that selected it: the step's block gains a ground `unknown`
        // context, and nothing else about the submission changes. A step
        // selected by an E0172 that erasure cannot fix (a thrown type, or an
        // inference variable decided as the binding) costs one extra compile
        // and then reports exactly as it did - bounded, and it cannot select a
        // step twice because the erased source is what the second compile sees.
        if let (Some(request), Some(lowered), Some(compile)) =
            (&session_request, &lowered_session, session.as_mut())
        {
            let erase = steps_publishing_a_binding(&diagnostics, &request.submission_name, lowered);
            if !erase.is_empty() {
                let relowered = lower_session_submission(request, &erase)?;
                db.add_session_file_in(
                    workspace,
                    runtime_source_virtual_path(&request.submission_name),
                    &relowered.source,
                );
                compile.artifact = relowered.artifact;
                compile.result_name = relowered.result_name;
                diagnostics = collect_diagnostics(&db)
                    .iter()
                    .map(|diagnostic| owned_diagnostic(&db, diagnostic, session_request.is_some()))
                    .collect();
            }
        }
        if diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == RuntimeDiagnosticSeverity::Error)
        {
            return Err(diagnostics);
        }

        if let Some(session) = &session
            && !matches!(
                session.expected,
                bex_vm_types::SessionContract::Checkable(baml_type::RuntimeTy::Unknown)
            )
        {
            let file = session.artifact.submission_name.as_str();
            let result_name = session.result_name.as_str();
            let Some(actual) = let_initializer_type(&db, workspace, result_name) else {
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
                    details: None,
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
            // The contract arrives by name; read it from the submission's
            // package, whose edges decide what it can name.
            #[expect(
                deprecated,
                reason = "the contract crosses the compile seam by name until it carries identities"
            )]
            let expected = match baml_type::Ty::<TypeName>::from(expected).try_map_heads(
                &mut |name| session_contract_head(&db, workspace, name).ok_or_else(|| name.clone()),
            ) {
                Ok(expected) => expected,
                Err(unreachable_name) => {
                    return Err(vec![runtime_diagnostic(
                        DiagnosticId::TypeMismatch,
                        file,
                        0,
                        0,
                        format!(
                            "`eval` contract names `{unreachable_name}`, whose package this submission cannot reach"
                        ),
                    )]);
                }
            };
            let context = baml_compiler2_hir_ty::facts::Facts::new(&db);
            if !baml_type::normalize::is_subtype(&actual, &expected, &context) {
                let viewpoint =
                    baml_compiler2_hir_ty::render::Viewpoint::user_facing(&db, workspace);
                return Err(vec![runtime_diagnostic(
                    DiagnosticId::TypeMismatch,
                    file,
                    0,
                    0,
                    format!(
                        "submission result has type `{}`, which is not a subtype of requested contract `{}`",
                        actual.render_with(&viewpoint),
                        expected.render_with(&viewpoint),
                    ),
                )]);
            }
        }

        let interface = export_interface(&db, workspace);
        let interface_blob =
            baml_artifact::encode(baml_artifact::ArtifactKind::PackageInterface, &interface)
                .map_err(|error| {
                    vec![RuntimeCompileDiagnostic {
                        code: "E_RUNTIME_INTERFACE".to_string(),
                        message: error.to_string(),
                        severity: RuntimeDiagnosticSeverity::Error,
                        span: None,
                        details: None,
                    }]
                })?;
        let emit_error = |error: baml_compiler2_emit::LoweringError| {
            vec![RuntimeCompileDiagnostic {
                code: "E_RUNTIME_EMIT".to_string(),
                message: error.to_string(),
                severity: RuntimeDiagnosticSeverity::Error,
                span: None,
                details: None,
            }]
        };
        let opt = OPT_LEVEL;
        let (emitted, kind) = match session {
            None => (
                emit_package(&db, workspace, opt).map_err(emit_error)?,
                ArtifactKind::Package,
            ),
            Some(mut session) => {
                let file = submission_file.unwrap_or_else(|| {
                    unreachable!("the session's submission is among the files it compiles")
                });
                let submission =
                    emit_session_submission(&db, workspace, file, opt).map_err(emit_error)?;
                session.artifact.initializers = submission.initializers;
                (
                    submission.package,
                    ArtifactKind::Session {
                        meta: session.artifact,
                        lease: session.lease,
                    },
                )
            }
        };
        Ok(RuntimeCompileArtifact {
            emitted,
            interface_blob,
            diagnostics,
            kind,
        })
    }
}

#[cfg(test)]
mod tests {
    use baml_compiler_diagnostics::{Diagnostic, DiagnosticText};
    use baml_compiler2_hir::file_package::file_package;

    use super::*;

    #[test]
    fn session_ambiguity_retains_its_error_code_and_pipe_span() {
        let span = baml_base::Span::new(
            baml_base::FileId::new(0),
            rowan::TextRange::new(20.into(), 21.into()),
        );
        let diagnostic = session_parse_error_diagnostic(
            &ParseError::AmbiguousUnion {
                span,
                groupings: vec![
                    "((A) -> B) | ((C) -> D)".into(),
                    "(A) -> (B | (C) -> D)".into(),
                ],
            },
            "submission.baml",
            10,
        );
        assert_eq!(diagnostic.code, DiagnosticId::AmbiguousUnion.code());
        assert_eq!(diagnostic.span.unwrap().start, 10);
        assert!(
            diagnostic
                .message
                .ends_with("`((A) -> B) | ((C) -> D)` or `(A) -> (B | (C) -> D)`"),
            "{}",
            diagnostic.message
        );
    }

    #[test]
    fn runtime_diagnostic_retains_structured_compiler_metadata() {
        let mut db = ProjectDatabase::new();
        let workspace = db
            .add_source_root(SourceRootSpec::new(
                RUNTIME_VIRTUAL_ROOT,
                baml_base::SourceRootKind::Workspace,
            ))
            .unwrap();
        let primary_file = db.add_or_update_file_in(
            workspace,
            &runtime_source_virtual_path("primary.baml"),
            "0123456789",
        );
        let related_file = db.add_or_update_file_in(
            workspace,
            &runtime_source_virtual_path("related.baml"),
            "abcdefghij",
        );
        let primary = baml_base::Span::new(
            primary_file.file_id(&db),
            rowan::TextRange::new(1.into(), 4.into()),
        );
        let secondary = baml_base::Span::new(
            related_file.file_id(&db),
            rowan::TextRange::new(2.into(), 5.into()),
        );
        let diagnostic = Diagnostic::warning(
            DiagnosticId::TypeMismatch,
            DiagnosticText::new()
                .text("cannot use ")
                .type_expr("string")
                .text(" here"),
        )
        .with_primary(
            primary,
            DiagnosticText::new().text("expected ").type_expr("int"),
        )
        .with_secondary(
            secondary,
            DiagnosticText::new()
                .text("value declared as ")
                .type_expr("string"),
        )
        .with_related(
            secondary,
            DiagnosticText::new().text("declaration of ").code("value"),
        )
        .with_phase(DiagnosticPhase::Type);

        let owned = owned_diagnostic(&db, &diagnostic, false);
        assert_eq!(owned.severity, RuntimeDiagnosticSeverity::Warning);
        assert_eq!(owned.message, "cannot use `string` here: expected `int`");
        assert_eq!(
            owned.span,
            Some(RuntimeSourceSpan {
                file: "primary.baml".to_string(),
                start: 1,
                end: 4,
            })
        );
        let details = owned.details.expect("source diagnostics retain detail");
        assert_eq!(details.phase, RuntimeDiagnosticPhase::Type);
        assert_eq!(details.headline, "cannot use `string` here");
        assert_eq!(details.primary_label.as_deref(), Some("expected `int`"));
        assert_eq!(details.message_highlights.len(), 1);
        assert_eq!(
            details.message_highlights[0].kind,
            RuntimeDiagnosticHighlightKind::TypeExpression
        );
        assert_eq!(details.annotations.len(), 2);
        assert!(details.annotations[0].is_primary);
        assert_eq!(details.annotations[0].message_highlights.len(), 1);
        assert!(!details.annotations[1].is_primary);
        assert_eq!(details.annotations[1].span.file, "related.baml");
        assert_eq!(details.related_info.len(), 1);
        assert_eq!(details.related_info[0].message, "declaration of `value`");
        assert_eq!(details.related_info[0].message_highlights.len(), 1);
        assert_eq!(details.related_info[0].span.file, "related.baml");
    }

    #[test]
    fn runtime_virtual_paths_are_slash_oriented() {
        let source = runtime_source_virtual_path(r"ns_tools\ns_nested\main.baml");
        assert_eq!(
            source.to_string_lossy(),
            "<runtime>/ns_tools/ns_nested/main.baml"
        );
        assert!(!source.to_string_lossy().contains('\\'));
    }

    #[test]
    fn runtime_virtual_paths_derive_packages_and_namespaces() {
        let mut db = ProjectDatabase::new();
        let workspace = db
            .add_source_root(SourceRootSpec::new(
                RUNTIME_VIRTUAL_ROOT,
                baml_base::SourceRootKind::Workspace,
            ))
            .unwrap();

        let source = db.add_or_update_file_in(
            workspace,
            &runtime_source_virtual_path(r"ns_tools\ns_nested\main.baml"),
            "",
        );
        let source_package = file_package(&db, source);
        assert_eq!(source_package.root, workspace);
        assert_eq!(
            source_package
                .namespace_path
                .iter()
                .map(Name::as_str)
                .collect::<Vec<_>>(),
            ["tools", "nested"]
        );
    }

    #[test]
    fn session_internal_names_demangle_to_what_the_author_wrote() {
        assert_eq!(
            demangle_session_names("scoped runtime type `__baml_session_3_T` cannot leave"),
            "scoped runtime type `T` cannot leave"
        );
        assert_eq!(
            demangle_session_names("`__baml_session_12_type_value_Out` and `__baml_session_0_v`"),
            "`Out` and `v`"
        );
        // Not the generator's shape: left alone.
        assert_eq!(
            demangle_session_names("__baml_session_x `__baml_session_`"),
            "__baml_session_x `__baml_session_`"
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
