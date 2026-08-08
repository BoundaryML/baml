//! Compiler-neutral values crossing the runtime compilation seam.
//!
//! These types deliberately live below both `bex_engine` and `baml_project`:
//! the engine owns only an injected compiler trait object, while the concrete
//! compiler implementation is assembled in `bex_project`.

use indexmap::IndexMap;

use baml_type::{Interface, Name, QualifiedTypeName, RealizedTy, Ty};

use crate::CompilationUnit;

/// Compiler-neutral structural projection of one runtime class definition.
#[derive(Clone, Debug)]
pub struct RuntimeMountedClass {
    pub qtn: QualifiedTypeName,
    pub fields: Vec<(Name, Ty, RuntimeMountedFieldAttrs)>,
}

#[derive(Clone, Debug)]
pub struct RuntimeMountedEnum {
    pub qtn: QualifiedTypeName,
    pub variants: Vec<Name>,
}

#[derive(Clone, Debug, Default)]
pub struct RuntimeMountedFieldAttrs {
    pub alias: Option<String>,
    pub description: Option<String>,
}

/// One exact type value mounted under a source-visible export name.
#[derive(Clone, Debug)]
pub struct RuntimeTypeMount {
    pub export_name: Name,
    pub identity_name: QualifiedTypeName,
    pub ty: RealizedTy,
    pub classes: Vec<RuntimeMountedClass>,
    pub enums: Vec<RuntimeMountedEnum>,
    pub witnesses: Vec<(Interface, Vec<(Name, Name)>)>,
}

#[derive(Clone, Debug, Default)]
pub struct RuntimePackageMount {
    pub interface_blob: Vec<u8>,
    pub types: Vec<RuntimeTypeMount>,
}

/// One isolated `reflect.Package.compile` request.
#[derive(Clone, Debug, Default)]
pub struct RuntimeCompileRequest {
    /// Project-root-relative submitted paths and their source text.
    pub files: IndexMap<String, String>,
    /// Source-less dependency package name to enriched `PackageInterface` blob.
    pub packages: IndexMap<String, RuntimePackageMount>,
}

/// Severity retained from the compiler diagnostic stream.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuntimeDiagnosticSeverity {
    Error,
    Warning,
    Info,
}

/// A byte range in one of the paths submitted to the compile call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeSourceSpan {
    pub file: String,
    pub start: usize,
    pub end: usize,
}

/// Stable diagnostic data safe to retain after the transient compiler DB drops.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeCompileDiagnostic {
    pub code: String,
    pub message: String,
    pub severity: RuntimeDiagnosticSeverity,
    pub span: Option<RuntimeSourceSpan>,
}

/// Successful compiler output retained by the runtime.
#[derive(Clone, Debug)]
pub struct RuntimeCompileArtifact {
    /// Relocatable user units. Builtin/dependency definitions remain imports.
    pub units: Vec<CompilationUnit>,
    /// Enriched check surface for mounting this package in a later compile.
    pub interface_blob: Vec<u8>,
    /// Non-error diagnostics produced by the successful compilation.
    pub diagnostics: Vec<RuntimeCompileDiagnostic>,
}
