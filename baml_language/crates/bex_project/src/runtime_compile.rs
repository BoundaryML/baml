//! Concrete runtime compiler assembled above the engine/compiler dependency
//! boundary.

use std::{collections::BTreeMap, path::Path, sync::Arc};

use baml_base::Name;
use baml_compiler_diagnostics::Severity;
use baml_compiler2_emit::{CompileOptions, OptLevel, emit_units};
use baml_compiler2_hir::package::PackageId;
use baml_compiler2_tir::package_interface::package_interface;
use baml_project::{ProjectDatabase, collect_diagnostics};
use bex_engine::RuntimeCompiler;
use bex_vm_types::{
    RuntimeCompileArtifact, RuntimeCompileDiagnostic, RuntimeCompileRequest,
    RuntimeDiagnosticSeverity, RuntimePackageMount, RuntimeSourceSpan,
};

fn enrich_runtime_mount(
    mut package: RuntimePackageMount,
) -> Result<Vec<u8>, RuntimeCompileDiagnostic> {
    use baml_compiler2_tir::package_interface::{
        ExportedFieldAttrs, ExportedImpl, ExportedImplOrigin, ExportedType, PackageInterface,
    };

    let mut interface =
        borsh::from_slice::<PackageInterface>(&package.interface_blob).map_err(|error| {
            RuntimeCompileDiagnostic {
                code: "E_RUNTIME_INTERFACE".to_string(),
                message: error.to_string(),
                severity: RuntimeDiagnosticSeverity::Error,
                span: None,
            }
        })?;
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
    borsh::to_vec(&interface).map_err(|error| RuntimeCompileDiagnostic {
        code: "E_RUNTIME_INTERFACE".to_string(),
        message: error.to_string(),
        severity: RuntimeDiagnosticSeverity::Error,
        span: None,
    })
}

/// Stateless compiler provider. A fresh database is allocated inside every
/// [`RuntimeCompiler::compile`] call and dropped before the call returns.
#[derive(Debug, Default)]
pub struct ProjectRuntimeCompiler;

pub fn runtime_compiler() -> Arc<dyn RuntimeCompiler> {
    Arc::new(ProjectRuntimeCompiler)
}

fn owned_diagnostic(
    db: &ProjectDatabase,
    diagnostic: &baml_compiler_diagnostics::Diagnostic,
) -> RuntimeCompileDiagnostic {
    let span = diagnostic.primary_span().and_then(|span| {
        db.file_id_to_path(span.file_id)
            .map(|path| RuntimeSourceSpan {
                file: path
                    .strip_prefix("<runtime>")
                    .unwrap_or(path)
                    .to_string_lossy()
                    .into_owned(),
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

impl RuntimeCompiler for ProjectRuntimeCompiler {
    fn compile(
        &self,
        request: RuntimeCompileRequest,
    ) -> Result<RuntimeCompileArtifact, Vec<RuntimeCompileDiagnostic>> {
        // This local is the transience guarantee: no handle to `db` occurs in
        // either return type, and all retained values below are deep-owned.
        let mut db = ProjectDatabase::new();
        db.set_project_root(Path::new("<runtime>"));
        let mounted = request
            .packages
            .into_iter()
            .map(|(name, package)| enrich_runtime_mount(package).map(|blob| (name, blob)))
            .collect::<Result<BTreeMap<_, _>, _>>()
            .map_err(|diagnostic| vec![diagnostic])?;
        db.set_mounted_packages(mounted);
        for (path, source) in request.files {
            // Runtime input names are package-relative. Mounting them beneath
            // the synthetic root makes `ns_foo/` namespace derivation behave
            // exactly like an ordinary project without exposing the synthetic
            // prefix in diagnostics.
            db.add_file(Path::new("<runtime>").join(path), &source);
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
            emit_test_cases: false,
        };
        let emitted = emit_units(&db, &options, OptLevel::One).map_err(|error| {
            vec![RuntimeCompileDiagnostic {
                code: "E_RUNTIME_EMIT".to_string(),
                message: error.to_string(),
                severity: RuntimeDiagnosticSeverity::Error,
                span: None,
            }]
        })?;
        let units: Vec<_> = emitted
            .into_iter()
            .filter(|unit| unit.package.as_str() == "user")
            .collect();
        Ok(RuntimeCompileArtifact {
            units,
            interface_blob,
            diagnostics,
        })
    }
}
