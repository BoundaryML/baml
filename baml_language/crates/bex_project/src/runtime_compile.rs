//! Concrete runtime compiler assembled above the engine/compiler dependency
//! boundary.

use std::{path::Path, sync::Arc};

use baml_base::Name;
use baml_compiler_diagnostics::Severity;
use baml_compiler2_emit::{CompileOptions, OptLevel, emit_units};
use baml_compiler2_hir::package::PackageId;
use baml_compiler2_tir::package_interface::package_interface;
use baml_project::{ProjectDatabase, collect_diagnostics};
use bex_engine::RuntimeCompiler;
use bex_vm_types::{
    RuntimeCompileArtifact, RuntimeCompileDiagnostic, RuntimeCompileRequest,
    RuntimeDiagnosticSeverity, RuntimeSourceSpan,
};

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
                file: path.to_string_lossy().into_owned(),
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
        db.set_mounted_packages(request.packages.into_iter().collect());
        for (path, source) in request.files {
            db.add_file(path, &source);
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
        let units = emit_units(&db, &options, OptLevel::One)
            .map_err(|error| {
                vec![RuntimeCompileDiagnostic {
                    code: "E_RUNTIME_EMIT".to_string(),
                    message: error.to_string(),
                    severity: RuntimeDiagnosticSeverity::Error,
                    span: None,
                }]
            })?
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
