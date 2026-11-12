//! Concrete runtime compiler assembled above the engine/compiler dependency
//! boundary.

use std::{
    collections::HashSet,
    fmt::Write as _,
    path::{Path, PathBuf},
    sync::Arc,
};

use baml_base::{LangPackage, Name};
use baml_compiler_diagnostics::{
    DiagnosticId, DiagnosticIdentifierKind, DiagnosticMessageHighlight, DiagnosticMessageKind,
    DiagnosticPhase, Severity,
};
use baml_compiler2_emit::OptLevel;
use baml_compiler2_hir::{
    body::{BodyOwnerId, LetBody, let_body},
    contributions::Definition,
};
use baml_compiler2_hir_ty::package_interface::{PackageInterface, WireInterface, interface_digest};
use baml_db::{CheckedWorld, Dependency, ProjectDatabase, ProjectDiagnostics, SourceRootSpec};
use baml_parser::ParseError;
use baml_syntax::{
    index::descendants,
    lexer::lex,
    nodes::{
        AnyFunctionBody, AnyItem, AnyPattern, AnyStmt, AssignStmt, EnumVariant, Field, FileTree,
        LetStmt, Name as NameNode, Param, TypeBindingStmt,
    },
    token::{Ident, TokenKind, TokenSet as _},
    tree::{AnyNode as _, AnyNodeId, Id},
};
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
/// Every head of the surface is a [`MountHead`]: the mount's own by path,
/// another mount's through one of the mount's own edges, a language
/// package's by identity. The blob's dependency table is exactly those edges
/// — every one a direct entry carrying its target's digest (`digest_of`),
/// since a mount edge never reaches a language package — plus a prelude
/// entry for each language package a head names, under the package's fixed
/// name: the same table an export of a compiled package builds.
fn projected_interface(
    surface: &bex_vm_types::RuntimeProjectedSurface,
    edges: &IndexMap<Name, bex_vm_types::RuntimeMountEdge>,
    built: &IndexMap<bex_vm_types::RuntimePackageIdentity, PackageInterface<WireName>>,
    digest_of: &dyn Fn(bex_vm_types::RuntimePackageIdentity) -> Digest,
) -> Result<WireInterface, RuntimeCompileDiagnostic> {
    use baml_compiler2_hir_ty::package_interface::{
        ExportedFieldAttrs, ExportedImpl, ExportedImplOrigin, ExportedType, ReExport,
    };
    use bex_vm_types::MountHead;

    /// The blob's dependency table under construction: a slot per edge of
    /// the mount, in edge order, then a prelude slot per language package the
    /// surface names, in first-use order. An edge is found by its name and a
    /// prelude slot by its package, each in its own table, so neither lookup
    /// can answer with the other's entry.
    struct Slots {
        edges: Vec<(Name, Locator)>,
        /// Each language package's slot index, by its position in
        /// [`LangPackage::ALL`]; `None` until the surface names it.
        prelude: [Option<usize>; LangPackage::ALL.len()],
    }

    impl Slots {
        fn edge(&self, edge: &Name) -> Result<DepSlot, RuntimeCompileDiagnostic> {
            self.edges
                .iter()
                .position(|(slotted, _)| slotted == edge)
                .map(DepSlot::of_dependency_index)
                .ok_or_else(|| {
                    mount_diagnostic(
                        "E_RUNTIME_MOUNT",
                        format!(
                            "the projected surface names a declaration through `{edge}`, which \
                             is no edge of the mount"
                        ),
                    )
                })
        }

        /// The slot of the language package `package`, by identity: the
        /// next index past the edges on first use.
        fn prelude(&mut self, package: LangPackage) -> DepSlot {
            let next = self.edges.len() + self.prelude.iter().flatten().count();
            let index = *self.prelude[package as usize].get_or_insert(next);
            DepSlot::of_dependency_index(index)
        }

        /// The table in slot order: the edges, then the prelude slots in
        /// the order they were handed out.
        fn locators(self) -> Vec<Locator> {
            let mut prelude: Vec<(usize, LangPackage)> = LangPackage::ALL
                .into_iter()
                .filter_map(|package| Some((self.prelude[package as usize]?, package)))
                .collect();
            prelude.sort_unstable_by_key(|(index, _)| *index);
            self.edges
                .into_iter()
                .map(|(_, locator)| locator)
                .chain(
                    prelude
                        .into_iter()
                        .map(|(_, package)| Locator::Prelude(package)),
                )
                .collect()
        }

        fn wire(&mut self, head: &MountHead) -> Result<WireName, RuntimeCompileDiagnostic> {
            match head {
                MountHead::Own(path) => {
                    Ok(WireName::own(path.namespace.clone(), path.name.clone()))
                }
                MountHead::Edge { edge, path } => Ok(WireName::qualified(
                    self.edge(edge)?,
                    path.namespace.clone(),
                    path.name.clone(),
                )),
                MountHead::Prelude { package, path } => Ok(WireName::qualified(
                    self.prelude(*package),
                    path.namespace.clone(),
                    path.name.clone(),
                )),
            }
        }

        fn wire_ty(
            &mut self,
            ty: &baml_type::Ty<MountHead>,
        ) -> Result<baml_type::Ty<WireName>, RuntimeCompileDiagnostic> {
            ty.try_map_heads(&mut |head| self.wire(head))
        }
    }

    let mut slots = Slots {
        prelude: [None; LangPackage::ALL.len()],
        edges: edges
            .iter()
            .map(|(edge, mount_edge)| {
                // A mount edge never reaches a language package (the prelude
                // never crosses the seam), so every edge is a digested entry.
                let locator = match mount_edge.kind {
                    bex_vm_types::types::EdgeKind::Dependency
                    | bex_vm_types::types::EdgeKind::ReExported => Locator::Direct {
                        edge: edge.clone(),
                        digest: digest_of(mount_edge.target),
                    },
                };
                (edge.clone(), locator)
            })
            .collect(),
    };

    let mut interface = PackageInterface::<WireName> {
        types: IndexMap::new(),
        functions: IndexMap::new(),
        values: IndexMap::new(),
        throw_sets: baml_compiler2_hir_ty::package_interface::FunctionThrowSets::default(),
        namespaces: std::collections::BTreeSet::default(),
        impls: Vec::new(),
        reexports: IndexMap::new(),
    };
    let duplicate = |name: &LocalName| {
        mount_diagnostic("E0011", format!("duplicate exported type name `{name}`"))
    };
    let exported = |interface: &PackageInterface<WireName>, name: &LocalName| {
        interface.lookup_type(&name.namespace, &name.name).is_some()
            || interface
                .lookup_function(&name.namespace, &name.name)
                .is_some()
            || interface
                .lookup_reexport(&name.namespace, &name.name)
                .is_some()
    };
    let add_row = |interface: &mut PackageInterface<WireName>,
                   name: &LocalName,
                   row: ExportedType<WireName>|
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
        let fields = class
            .fields
            .iter()
            .map(|(name, ty, attrs)| {
                Ok((
                    name.clone(),
                    slots.wire_ty(ty)?,
                    ExportedFieldAttrs {
                        alias: attrs.alias.clone(),
                        description: attrs.description.clone(),
                        docstring: attrs.docstring.clone(),
                    },
                ))
            })
            .collect::<Result<Vec<_>, RuntimeCompileDiagnostic>>()?;
        add_row(
            &mut interface,
            &name,
            ExportedType::Class {
                qtn: WireName::own(Vec::new(), class.name.clone()),
                fields,
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
                qtn: WireName::own(Vec::new(), enm.name.clone()),
                variants: enm.variants.iter().map(|(name, _)| name.clone()).collect(),
            },
        )?;
    }
    for alias in &surface.aliases {
        let resolved = slots.wire_ty(&alias.ty)?;
        add_row(
            &mut interface,
            &alias.name,
            ExportedType::TypeAlias {
                qtn: WireName::own(alias.name.namespace.clone(), alias.name.name.clone()),
                resolved,
            },
        )?;
    }
    let add_reexport = |interface: &mut PackageInterface<WireName>,
                        name: &LocalName,
                        reexport: ReExport<WireName>|
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
        let of = slots.wire(&reexport.of)?;
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
        let slot = slots.edge(edge)?;
        let spell = |namespace: &[Name], name: &Name| {
            WireName::qualified(slot, namespace.to_vec(), name.clone())
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
        let generics = rule
            .interface
            .generics
            .iter()
            .map(|ty| slots.wire_ty(ty))
            .collect::<Result<Vec<_>, RuntimeCompileDiagnostic>>()?;
        let associated_types = rule
            .interface
            .associated_types
            .iter()
            .map(|(name, ty)| Ok((name.clone(), slots.wire_ty(ty)?)))
            .collect::<Result<Vec<_>, RuntimeCompileDiagnostic>>()?;
        interface.impls.push(ExportedImpl {
            // The bindings are the row's `associated_types`; the header
            // names the interface and its arguments only, exactly as a
            // source impl's does.
            associated_types,
            interface: baml_type::Interface::new(
                slots.wire(&rule.interface.name)?,
                generics.into(),
                Box::new([]),
            ),
            for_ty_pattern: slots.wire_ty(&rule.for_ty)?,
            generic_params: Vec::new(),
            param_bounds: Vec::new(),
            field_links: rule.field_links.clone(),
            origin: ExportedImplOrigin::OutOfBody,
            methods: Vec::new(),
        });
    }
    Ok(WireInterface {
        dependencies: slots.locators(),
        rows: interface,
    })
}

/// The names the roots of a request take, made distinct: an aliased package
/// is named by its first alias; a package no alias reaches — one a mount's
/// edges reach — by the first edge name reaching it; either suffixed with
/// `$k` when the name is taken. A root's name is display metadata and what
/// the program spells its declarations by where no edge reaches it; it
/// resolves nothing — a package reaches itself as `root` only — so it may
/// coincide with one of the package's own edge names. Every declaration a
/// consumer can write is spelled through an edge or a re-export.
fn root_names(
    packages: &IndexMap<bex_vm_types::RuntimePackageIdentity, RuntimePackageMount>,
    aliases: &IndexMap<String, bex_vm_types::RuntimePackageIdentity>,
) -> IndexMap<bex_vm_types::RuntimePackageIdentity, Name> {
    let mut taken: HashSet<Name> = baml_builtins2::RESERVED_EDGE_NAMES
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
        let mut candidate = preferred.clone();
        let mut suffix = 1;
        while taken.contains(&candidate) {
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
    let (code, message) = match error {
        ParseError::UnexpectedToken {
            expected, found, ..
        } => (
            DiagnosticId::InvalidSyntax,
            format!("unexpected token: expected {expected}, found {found}"),
        ),
        ParseError::UnexpectedEof { expected, .. } => (
            DiagnosticId::InvalidSyntax,
            format!("unexpected end of file: expected {expected}"),
        ),
        ParseError::InvalidSyntax { message, .. } | ParseError::RemovedFeature { message, .. } => {
            (DiagnosticId::InvalidSyntax, message.clone())
        }
        ParseError::InvalidStringEscape { issue, .. } => {
            (DiagnosticId::InvalidStringEscape, issue.to_string())
        }
        ParseError::InvalidDirective { message, .. } => {
            (DiagnosticId::MalformedAttribute, message.clone())
        }
        ParseError::AmbiguousUnion { groupings, .. } => (
            DiagnosticId::AmbiguousUnion,
            format!(
                "{}: {}",
                ParseError::AMBIGUOUS_UNION_MESSAGE,
                ParseError::ambiguous_union_label(groupings)
            ),
        ),
    };
    let range = error.range();
    let start = usize::from(range.start()).saturating_sub(prefix_len);
    let end = usize::from(range.end()).saturating_sub(prefix_len);
    runtime_diagnostic(code, submission_name, start, end, message)
}

/// The byte extent of a node in the text its tree was parsed from.
fn byte_range(tree: &FileTree, id: AnyNodeId) -> std::ops::Range<usize> {
    let range = tree.tokens().text_range(tree.arenas().span(id));
    usize::from(range.start())..usize::from(range.end())
}

/// A name token's text and byte extent.
fn name_at(tree: &FileTree, name: Id<NameNode>) -> (String, std::ops::Range<usize>) {
    let idx = tree.get(name).token.idx;
    let range = tree.tokens().range(idx);
    (
        tree.token_text(idx).to_string(),
        usize::from(range.start())..usize::from(range.end()),
    )
}

/// A submission item that declares something the rest of the submission
/// (and later submissions) can name.
struct Declaration {
    node: AnyNodeId,
    /// The declared name and its byte extent; an `implements … for` has
    /// none.
    name: Option<(String, std::ops::Range<usize>)>,
}

/// The declaration `item` is, if it is one. A nameless head of a named kind
/// is not a declaration (its parse error is reported by the wrapped parse).
fn declaration(tree: &FileTree, item: AnyItem) -> Option<Declaration> {
    let node = item
        .any_node_id()
        .unwrap_or_else(|| unreachable!("every alternative of `AnyItem` is a node"));
    let name = match item {
        AnyItem::Function(id) => tree.get(id).name.ok()?,
        AnyItem::Class(id) => tree.get(id).name.ok()?,
        AnyItem::Enum(id) => tree.get(id).name.ok()?,
        AnyItem::Interface(id) => tree.get(id).name.ok()?,
        AnyItem::TypeAlias(id) => tree.get(id).name.ok()?,
        AnyItem::ClientValue(id) => tree.get(id).name,
        AnyItem::Removed(id) => tree.get(id).name?,
        AnyItem::ImplementsFor(_) => return Some(Declaration { node, name: None }),
        AnyItem::Let(_) | AnyItem::TypeBinding(_) | AnyItem::Test(_) | AnyItem::Bogus(_) => {
            return None;
        }
    };
    Some(Declaration {
        node,
        name: Some(name_at(tree, name)),
    })
}

/// The first name under a `let`'s pattern.
// BUG: "first name" reads the type of a destructuring or ascribed pattern
// (`let User { name } = …` binds `User`, `let _: int = …` binds `int`), as
// the rowan walk did; a session `let` should register every binding the
// pattern introduces and nothing else.
fn first_pattern_name(
    tree: &FileTree,
    stmt: Id<LetStmt>,
) -> Option<(String, std::ops::Range<usize>)> {
    let pattern = tree.get(stmt).pattern.ok()?;
    let pattern = pattern
        .any_node_id()
        .unwrap_or_else(|| unreachable!("every alternative of `AnyPattern` is a node"));
    descendants(tree, pattern)
        .find_map(AnyNodeId::cast::<NameNode>)
        .map(|name| name_at(tree, name))
}

/// The target, operator and value of an assignment step, as written. The
/// operator is spelled without its `=`, so a compound one re-forms as
/// `{operator}=`.
fn assignment_parts<'a>(
    tree: &FileTree,
    stmt: Id<AssignStmt>,
    source: &'a str,
) -> Option<(&'a str, &'static str, &'a str)> {
    let stmt = tree.get(stmt);
    let text = |id: AnyNodeId| source[byte_range(tree, id)].trim();
    let target = text(
        stmt.target
            .any_node_id()
            .unwrap_or_else(|| unreachable!("every alternative of `AnyExpr` is a node")),
    );
    let value = text(
        stmt.value
            .ok()?
            .any_node_id()
            .unwrap_or_else(|| unreachable!("every alternative of `AnyExpr` is a node")),
    );
    let spelling = stmt.op.op().spelling();
    let operator = if spelling == "=" {
        "="
    } else {
        spelling
            .strip_suffix('=')
            .unwrap_or_else(|| unreachable!("every assignment operator ends in `=`"))
    };
    Some((target, operator, value))
}

/// The source text of a session `let type T [extends …] = operand [else
/// { … }];` step, sliced from the wrapped submission it was parsed from.
struct RuntimeTypeBindingParts<'a> {
    name: String,
    /// The `extends A & B` clause, verbatim.
    bounds: Option<&'a str>,
    operand: &'a str,
    /// The `{ … }` block after `else`, verbatim.
    else_block: Option<&'a str>,
}

/// Split a `let type` step into its source parts. `None` for an incomplete
/// statement (the removed `type T = …` spelling is a parse error the session
/// already reported).
fn runtime_type_binding_parts<'a>(
    tree: &FileTree,
    stmt: Id<TypeBindingStmt>,
    wrapped: &'a str,
) -> Option<RuntimeTypeBindingParts<'a>> {
    let text = |id: AnyNodeId| -> &'a str { wrapped[byte_range(tree, id)].trim() };
    let stmt = tree.get(stmt);
    let (name, _) = name_at(tree, stmt.name.ok()?);
    let operand = stmt
        .init
        .ok()?
        .any_node_id()
        .unwrap_or_else(|| unreachable!("every alternative of `AnyExpr` is a node"));
    Some(RuntimeTypeBindingParts {
        name,
        bounds: stmt.bounds.map(|bounds| text(AnyNodeId::of(bounds))),
        operand: text(operand),
        else_block: stmt
            .else_branch
            .and_then(|branch| branch.block.ok())
            .map(|block| text(AnyNodeId::of(block))),
    })
}

/// Re-bind every earlier `let type` step's type for a later step, with its
/// bounds, so `T` keeps the same static meaning there. A bounded `let type`
/// must have an `else` block; the step that wrote the binding already checked
/// the bounds, so this one is not expected to run.
fn runtime_type_binding_prelude(bindings: &IndexMap<String, SessionVisibleSymbol>) -> String {
    let mut prelude = String::new();
    for (name, symbol) in bindings {
        if let SessionVisibleKind::TypeBinding { type_value, bounds } = &symbol.kind {
            if bounds.is_empty() {
                let _ = writeln!(prelude, "let type {} = {type_value};", symbol.internal);
            } else {
                let _ = writeln!(
                    prelude,
                    "let type {} {bounds} = {type_value} else {{ \
                     baml.sys.panic(\"session type `{name}` no longer satisfies its bounds\") }};",
                    symbol.internal
                );
            }
        }
    }
    prelude
}

/// Every name bound inside `node` — parameters and `let` bindings — other
/// than the step's own outer `let`, whose name becomes a Session global.
fn locally_bound_names(
    tree: &FileTree,
    node: AnyNodeId,
    outer_let: Option<&std::ops::Range<usize>>,
) -> HashSet<String> {
    let mut names = HashSet::new();
    for local in descendants(tree, node) {
        if let Some(stmt) = local.cast::<LetStmt>() {
            if let Some((name, range)) = first_pattern_name(tree, stmt)
                && outer_let != Some(&range)
            {
                names.insert(name);
            }
        } else if let Some(param) = local.cast::<Param>()
            && let Ok(name) = tree.get(param).name
        {
            names.insert(name_at(tree, name).0);
        }
    }
    names
}

/// The name tokens of the fields and variants `node` declares: member keys,
/// never Session globals.
fn structural_name_ranges(tree: &FileTree, node: AnyNodeId) -> HashSet<std::ops::Range<usize>> {
    descendants(tree, node)
        .filter_map(|child| {
            let name = if let Some(field) = child.cast::<Field>() {
                tree.get(field).name
            } else {
                tree.get(child.cast::<EnumVariant>()?).name
            };
            Some(name_at(tree, name).1)
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
    // A string or comment is one token, so a word inside one is never a
    // candidate; a backtick string's `${…}` interpolations are code tokens
    // between its text chunks, so Session names inside them still resolve.
    let tokens = lex(source);
    let mut output = String::with_capacity(source.len());
    for idx in tokens.iter() {
        let text_range = tokens.range(idx);
        let range = usize::from(text_range.start())..usize::from(text_range.end());
        let text = &source[range.clone()];
        if let Some(replacement) = forced.get(&range) {
            output.push_str(replacement);
            continue;
        }
        let replacement = if Ident::KINDS.contains(tokens.kind(idx))
            && !skipped.contains(&range)
            && !local_names.contains(text)
        {
            let prev = tokens.prev_significant(idx).map(|i| tokens.kind(i));
            let next = tokens.next_significant(idx).map(|i| tokens.kind(i));
            let reserved_package_root = next == Some(TokenKind::Dot)
                && (text == "json" || baml_builtins2::STDLIB_PACKAGE_NAMES.contains(&text));
            if prev == Some(TokenKind::Dot)
                || next == Some(TokenKind::Colon)
                || reserved_package_root
            {
                None
            } else {
                mapping.get(text)
            }
        } else {
            None
        };
        output.push_str(replacement.map_or(text, String::as_str));
    }
    output
}

/// One line of the wrapped submission's block: a statement, or the tail
/// expression the submission evaluates to.
#[derive(Clone, Copy)]
enum Element {
    Stmt(AnyStmt),
    Tail(baml_syntax::nodes::AnyExpr),
}

impl Element {
    fn any_node_id(self) -> Option<AnyNodeId> {
        match self {
            Element::Stmt(stmt) => stmt.any_node_id(),
            Element::Tail(expr) => expr.any_node_id(),
        }
    }
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
// BUG: a session enforces neither the `root` reservation nor an own item's
// precedence over a package spelled alike: a submission may declare `let root`,
// `function root()` or a `class root`, and `function log()` beside `log.info(x)`
// still reaches the stdlib, where a file and `reflect.Package.compile` refuse
// both. Session submissions should be checked by the same HIR rules.
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
    use baml_compiler2_hir::package::{dependency_named, package_dependency_closure};

    let root = if name.is_local() {
        workspace
    } else if let Some(root) = dependency_named(db, workspace, name.package()) {
        root
    } else {
        // The closure package the engine labelled this way: the one declaring
        // the name, else the one some closure member reaches by it.
        let closure = package_dependency_closure(db, workspace);
        closure
            .iter()
            .copied()
            .find(|root| root.self_name(db).as_ref() == Some(name.package()))
            .or_else(|| {
                closure.iter().find_map(|member| {
                    let root = dependency_named(db, *member, name.package())?;
                    closure.contains(&root).then_some(root)
                })
            })?
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
/// is typed by a session `let type T = …` binding, as a first compile reported at
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

    // The first parse only locates the declarations; its errors, if any, are
    // reported by the parse of the wrapped submission below.
    let parsed = baml_parser::parse(&request.source);
    let tree = &parsed.tree;
    let declarations_nodes = tree
        .list(tree.get(tree.root()).items)
        .iter()
        .filter_map(|&item| declaration(tree, item))
        .collect::<Vec<_>>();

    let mut declarations = IndexMap::new();
    let mut declaration_name_ranges = IndexMap::new();
    for declaration in &declarations_nodes {
        if let Some((name, range)) = &declaration.name {
            let symbol = SessionVisibleSymbol {
                internal: internal(name),
                kind: SessionVisibleKind::Declaration,
            };
            declaration_name_ranges.insert(range.clone(), symbol.internal.clone());
            declarations.insert(name.clone(), symbol);
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
    for &Declaration { node, .. } in &declarations_nodes {
        let range = byte_range(tree, node);
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
        let skipped = structural_name_ranges(tree, node)
            .into_iter()
            .map(|name_range| name_range.start - range.start..name_range.end - range.start)
            .collect();
        let locals = locally_bound_names(tree, node, None);
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
    let parsed = baml_parser::parse(&wrapped);
    if let Some(error) = parsed.errors.first() {
        return Err(vec![session_parse_error_diagnostic(
            error,
            &request.submission_name,
            prefix.len(),
        )]);
    }
    let tree = &parsed.tree;
    // An error-free parse of `function … { … }` is that one function whose
    // body is the block the submission's lines were wrapped in — unless the
    // submission closed the wrapper's brace itself and went on with items of
    // its own (`1 }\nlet y = 2;\nlet z = {`): a submission is statements.
    let wrapper_escaped = || {
        Err(vec![runtime_diagnostic(
            DiagnosticId::InvalidSyntax,
            &request.submission_name,
            0,
            request.source.len(),
            "a submission is a sequence of statements; it cannot close the block it runs in or declare top-level items",
        )])
    };
    let [AnyItem::Function(function)] = tree.list(tree.get(tree.root()).items) else {
        return wrapper_escaped();
    };
    let Ok(AnyFunctionBody::Expr(block)) = tree.get(*function).body else {
        return wrapper_escaped();
    };
    let block = tree.get(block);
    // A header comment or a stray `;` is not code: the last element that is
    // decides what the submission evaluates to.
    let elements = tree
        .list(block.stmts)
        .iter()
        .filter(|stmt| !matches!(stmt, AnyStmt::Header(_) | AnyStmt::Empty(_)))
        .map(|&stmt| Element::Stmt(stmt))
        .chain(block.tail.map(Element::Tail))
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
    let last_code = elements.len().checked_sub(1);

    for (index, element) in elements.iter().enumerate() {
        let (node, has_semicolon, is_statement) = match *element {
            // The tail expression, or an expression line that a trailing
            // header comment kept from being the tail.
            Element::Tail(expr) => (
                expr.any_node_id()
                    .unwrap_or_else(|| unreachable!("every alternative of `AnyExpr` is a node")),
                false,
                false,
            ),
            Element::Stmt(AnyStmt::Expr(stmt)) => {
                (AnyNodeId::of(stmt), tree.get(stmt).semi.is_some(), false)
            }
            Element::Stmt(AnyStmt::Let(stmt)) => {
                let let_stmt = tree.get(stmt);
                match (let_stmt.pattern, let_stmt.init) {
                    // `let _ = e` binds nothing: the step is `e`, a statement.
                    (Ok(AnyPattern::Wildcard(_)), Ok(init)) => (
                        init.any_node_id().unwrap_or_else(|| {
                            unreachable!("every alternative of `AnyExpr` is a node")
                        }),
                        let_stmt.semi.is_some(),
                        true,
                    ),
                    _ => (AnyNodeId::of(stmt), let_stmt.semi.is_some(), true),
                }
            }
            Element::Stmt(AnyStmt::TypeBinding(stmt)) => {
                (AnyNodeId::of(stmt), tree.get(stmt).semi.is_some(), true)
            }
            Element::Stmt(AnyStmt::Assign(stmt)) => {
                (AnyNodeId::of(stmt), tree.get(stmt).semi.is_some(), true)
            }
            Element::Stmt(
                AnyStmt::Defer(_)
                | AnyStmt::While(_)
                | AnyStmt::WhileLet(_)
                | AnyStmt::For(_)
                | AnyStmt::Test(_),
            ) => {
                let stmt = element
                    .any_node_id()
                    .unwrap_or_else(|| unreachable!("every alternative of `AnyStmt` is a node"));
                (stmt, false, true)
            }
            Element::Stmt(AnyStmt::Header(_) | AnyStmt::Empty(_)) => continue,
            Element::Stmt(AnyStmt::Bogus(_)) => {
                unreachable!("an error-free parse has no bogus statement")
            }
        };
        let wrapped_range = byte_range(tree, node);
        let source_range = wrapped_range.start.saturating_sub(prefix.len())
            ..wrapped_range.end.saturating_sub(prefix.len());
        let raw = &request.source[source_range.clone()];
        let type_binding = match *element {
            Element::Stmt(AnyStmt::TypeBinding(stmt)) => Some(stmt),
            Element::Stmt(_) | Element::Tail(_) => None,
        };
        let outer_let = match *element {
            Element::Stmt(AnyStmt::Let(stmt))
                if !matches!(tree.get(stmt).pattern, Ok(AnyPattern::Wildcard(_))) =>
            {
                Some(stmt)
            }
            Element::Stmt(_) | Element::Tail(_) => None,
        };
        let is_outer_let = outer_let.is_some();
        let outer_binding = outer_let.and_then(|stmt| first_pattern_name(tree, stmt));
        let local_names =
            locally_bound_names(tree, node, outer_binding.as_ref().map(|(_, range)| range));
        let prelude = runtime_type_binding_prelude(&active_type_bindings);
        // Only the submission's final expression is its value. An earlier
        // one is a statement, as it would be in a block: its value is
        // dropped, so an `if` with no `else` there owes no unit branch.
        let returns_value =
            Some(index) == last_code && !is_outer_let && !is_statement && !has_semicolon;
        let (generated_name, step_source, commit_global, binding) = if let Some(stmt) = type_binding
        {
            let Some(parts) = runtime_type_binding_parts(tree, stmt, &wrapped) else {
                return Err(vec![runtime_diagnostic(
                    DiagnosticId::InvalidSyntax,
                    &request.submission_name,
                    source_range.start,
                    source_range.end,
                    "invalid runtime type binding",
                )]);
            };
            let rewrite = |text: &str| {
                rewrite_identifiers(
                    text,
                    &visible_mapping,
                    &IndexMap::new(),
                    &HashSet::new(),
                    &local_names,
                )
            };
            let name = parts.name;
            let type_name = internal(&name);
            let backing_name = format!("__baml_session_{sequence}_type_value_{name}");
            let operand = rewrite(parts.operand);
            let bounds = parts.bounds.map(&rewrite).unwrap_or_default();
            // The step stores the type value itself; later steps re-bind it
            // (`runtime_type_binding_prelude`). A bounded binding checks its
            // bounds here, where it is written, and runs its `else` block on
            // a miss.
            let source = if bounds.is_empty() {
                format!("let {backing_name} = {{\n{prelude}({operand})\n}}\n")
            } else {
                let else_clause = parts
                    .else_block
                    .map(|block| format!(" else {}", rewrite(block)))
                    .unwrap_or_default();
                let checked = format!("{backing_name}_checked");
                format!(
                    "let {backing_name} = {{\n{prelude}let {checked} = ({operand});\n\
                     let type {type_name} {bounds} = {checked}{else_clause};\n{checked}\n}}\n"
                )
            };
            let symbol = SessionVisibleSymbol {
                internal: type_name,
                kind: SessionVisibleKind::TypeBinding {
                    type_value: backing_name.clone(),
                    bounds,
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
            let assignment = match *element {
                Element::Stmt(AnyStmt::Assign(stmt)) => assignment_parts(tree, stmt, &wrapped),
                Element::Stmt(_) | Element::Tail(_) => None,
            }
            .and_then(|(target, operator, rhs)| {
                let symbol = request.visible.get(target)?;
                matches!(symbol.kind, SessionVisibleKind::Let).then_some((symbol, operator, rhs))
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
                // submissions read, and a scoped `let type T = …` binding lives
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
    fn declaration_fingerprints(
        &self,
        files: &indexmap::IndexMap<String, String>,
    ) -> bex_vm_types::DeclFingerprints {
        crate::hotswap_fingerprint::declaration_fingerprints(
            files
                .iter()
                .map(|(path, text)| (path.as_str(), text.as_str())),
        )
    }

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

        let mut collected = ProjectDiagnostics::collect(&db);
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
            let owned: Vec<_> = collected
                .iter()
                .map(|diagnostic| owned_diagnostic(&db, diagnostic, true))
                .collect();
            let erase = steps_publishing_a_binding(&owned, &request.submission_name, lowered);
            if !erase.is_empty() {
                // The edit below ends the collection's revision.
                drop(collected);
                let relowered = lower_session_submission(request, &erase)?;
                db.add_session_file_in(
                    workspace,
                    runtime_source_virtual_path(&request.submission_name),
                    &relowered.source,
                );
                compile.artifact = relowered.artifact;
                compile.result_name = relowered.result_name;
                collected = ProjectDiagnostics::collect(&db);
            }
        }
        let diagnostics: Vec<_> = collected
            .iter()
            .map(|diagnostic| owned_diagnostic(&db, diagnostic, session_request.is_some()))
            .collect();
        // The one gate: the world's check decides, and a refused world
        // reports every diagnostic the collection made.
        let Ok(world) = CheckedWorld::check(&collected, workspace) else {
            return Err(diagnostics);
        };

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

        let interface = world.export_interface();
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
                world.emit_package(opt).map_err(emit_error)?,
                ArtifactKind::Package,
            ),
            Some(mut session) => {
                let file = submission_file.unwrap_or_else(|| {
                    unreachable!("the session's submission is among the files it compiles")
                });
                let submission = world
                    .emit_session_submission(file, opt)
                    .map_err(emit_error)?;
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
        let diagnostic = session_parse_error_diagnostic(
            &ParseError::AmbiguousUnion {
                range: text_size::TextRange::new(20.into(), 21.into()),
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
            text_size::TextRange::new(1.into(), 4.into()),
        );
        let secondary = baml_base::Span::new(
            related_file.file_id(&db),
            text_size::TextRange::new(2.into(), 5.into()),
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
