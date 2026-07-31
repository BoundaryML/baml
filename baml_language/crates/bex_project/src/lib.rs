//! Reusable compile-and-run runtime for BAML programs.
//!
//! Three traits define the API:
//! - **`Bex`**: core run API (`call_function`). Implemented by `Arc<BexEngine>`.
//! - **`BexRuntime`**: holds DB, `update_source`, `function_names`, `engine_is_current`, `diagnostics`.
//! - **`BexWithLsp`**: LSP capabilities on top of `BexRuntime` (requires `lsp` feature).
//!
//! Two public constructors:
//! - [`new`] — compile source files and return `Arc<dyn Bex>`.
use std::{collections::HashMap, sync::Arc};

pub use baml_builtins2::{MediaContent, MediaValue, PromptAst, PromptAstSimple};
pub use bex::{Bex, BexCallTraceResult};
pub use bex_engine::{
    CANCELLED_PANIC_CLASS, CaptureDefaults, EngineError, FunctionCallContext,
    FunctionCallContextBuilder, InboundUnionAmbiguityPolicy, UnhandledSpawnError,
    UnhandledSpawnErrorHandler, is_cancelled_engine_error, register_inbound_union_ambiguity_policy,
    value_capture::{
        CaptureKind, EncodedTraceValue, TraceCaptureConfig, TraceCaptureProducer,
        TraceDrainFailure, TraceDrainFailureReason, TraceDrainReport, TraceLogMetadata,
    },
};
pub use bex_external_types::{
    BexExternalAdt, BexExternalValue, Handle, HostReleaseFn, HostReturnTypeError, HostValueArc,
    HostValueKind, MediaKind, RuntimeTy, TyAttr, host_release_dispatch,
    runtime_ty_structurally_equal, selected_arm_equal, try_convert_rust_data, validate_host_return,
};
use indexmap::IndexMap;
use serde::Deserialize;
pub use sys_ops::SysOps;
pub use sys_types::{CallId, CancellationToken};
use thiserror::Error;

mod bex;
mod bex_lsp;
mod fs;
mod project;
mod seed;

pub struct BexArgs {
    /// Required values keyed by their type-level names and kept in declared order.
    pub required: IndexMap<String, BexExternalValue>,
    /// Supplied optional values keyed by their type-level parameter names.
    pub optional: IndexMap<String, BexExternalValue>,
}

impl From<HashMap<&str, BexExternalValue>> for BexArgs {
    fn from(m: HashMap<&str, BexExternalValue>) -> Self {
        Self {
            required: m.into_iter().map(|(k, v)| (k.to_string(), v)).collect(),
            optional: IndexMap::new(),
        }
    }
}

impl From<HashMap<String, BexExternalValue>> for BexArgs {
    fn from(m: HashMap<String, BexExternalValue>) -> Self {
        Self {
            required: m.into_iter().collect(),
            optional: IndexMap::new(),
        }
    }
}

impl From<IndexMap<String, BexExternalValue>> for BexArgs {
    fn from(required: IndexMap<String, BexExternalValue>) -> Self {
        Self {
            required,
            optional: IndexMap::new(),
        }
    }
}

/// Errors that can occur during runtime operations.
#[derive(Debug, Error)]
pub enum RuntimeError {
    #[error("{0}")]
    Other(String),

    #[error("Invalid argument: {name}")]
    InvalidArgument { name: String },

    #[error("{message}")]
    Compilation { message: String },

    #[error("{message}")]
    BytecodeCompatibility { message: String },

    #[error("{0}")]
    Engine(#[from] bex_engine::EngineError),

    #[error("Failed to convert result to owned value: {0}")]
    Access(#[from] bex_heap::AccessError),
}

/// Reserved top-level table added to the copy of `baml.toml` embedded by
/// `baml generate`. The runtime deliberately owns a small, independent TOML
/// parser for this table instead of depending on the CLI's manifest schema.
pub const BYTECODE_METADATA_TABLE: &str = "__baml_codegen";

/// Current serialized [`bex_vm_types::Program`] format. Raw Borsh layouts are
/// not a stable cross-release contract, so this version is accepted only when
/// the producing and consuming BAML releases also match exactly.
pub const BYTECODE_FORMAT_VERSION: u32 = 1;

const BYTECODE_ENVELOPE_VERSION: u32 = 1;
const BYTECODE_ENVELOPE_MAGIC: [u8; 8] = *b"BAMLBC\0\0";

#[derive(borsh::BorshDeserialize, borsh::BorshSerialize)]
struct BytecodeEnvelope {
    magic: [u8; 8],
    envelope_version: u32,
    baml_toml: String,
    program: Vec<u8>,
}

#[derive(Deserialize)]
struct EmbeddedBamlToml {
    #[serde(rename = "__baml_codegen")]
    codegen: Option<BytecodeMetadata>,
}

#[derive(Deserialize)]
struct BytecodeMetadata {
    baml_cli_version: String,
    bytecode_format_version: u32,
}

fn regeneration_guidance() -> String {
    format!(
        "Re-run `baml generate` with baml-cli {} and redeploy the complete generated `baml_sdk`. If you intentionally generate with another BAML version, install the bridge package at that exact version.",
        baml_version::CANONICAL_VERSION
    )
}

fn compatibility_error(message: impl Into<String>) -> RuntimeError {
    RuntimeError::BytecodeCompatibility {
        message: format!(
            "BAML bytecode compatibility error: {}\n{}",
            message.into(),
            regeneration_guidance()
        ),
    }
}

fn parse_bytecode_metadata(baml_toml: &str) -> Result<BytecodeMetadata, RuntimeError> {
    let manifest: EmbeddedBamlToml = toml::from_str(baml_toml).map_err(|error| {
        compatibility_error(format!(
            "the embedded baml.toml is not valid TOML ({error})."
        ))
    })?;
    manifest.codegen.ok_or_else(|| {
        compatibility_error(format!(
            "the embedded baml.toml is missing the generated [{BYTECODE_METADATA_TABLE}] table."
        ))
    })
}

fn validate_bytecode_metadata(metadata: &BytecodeMetadata) -> Result<(), RuntimeError> {
    let runtime_version = baml_version::CANONICAL_VERSION;
    if metadata.baml_cli_version != runtime_version {
        return Err(compatibility_error(format!(
            "this generated SDK was produced by baml-cli {}, but the installed bridge runtime is {}. Generated bytecode requires an exact BAML release match.",
            metadata.baml_cli_version, runtime_version
        )));
    }
    if metadata.bytecode_format_version != BYTECODE_FORMAT_VERSION {
        return Err(compatibility_error(format!(
            "this generated SDK uses bytecode format {}, but bridge runtime {} supports format {}.",
            metadata.bytecode_format_version, runtime_version, BYTECODE_FORMAT_VERSION
        )));
    }
    Ok(())
}

/// Serialize a compiled program into the versioned envelope embedded in
/// generated SDKs. `baml_toml` must be the independently parseable manifest
/// copy containing [`BYTECODE_METADATA_TABLE`].
pub fn serialize_bytecode(
    program: &bex_vm_types::Program,
    baml_toml: &str,
) -> Result<Vec<u8>, RuntimeError> {
    let metadata = parse_bytecode_metadata(baml_toml)?;
    validate_bytecode_metadata(&metadata)?;
    let program = borsh::to_vec(program).map_err(|error| RuntimeError::Compilation {
        message: format!("Failed to serialize BAML bytecode: {error}"),
    })?;
    borsh::to_vec(&BytecodeEnvelope {
        magic: BYTECODE_ENVELOPE_MAGIC,
        envelope_version: BYTECODE_ENVELOPE_VERSION,
        baml_toml: baml_toml.to_string(),
        program,
    })
    .map_err(|error| RuntimeError::Compilation {
        message: format!("Failed to serialize BAML bytecode envelope: {error}"),
    })
}

fn deserialize_bytecode(bytecode: &[u8]) -> Result<bex_vm_types::Program, RuntimeError> {
    if !bytecode.starts_with(&BYTECODE_ENVELOPE_MAGIC) {
        return Err(compatibility_error(format!(
            "bridge runtime {} cannot safely load unversioned or malformed generated bytecode.",
            baml_version::CANONICAL_VERSION
        )));
    }
    let envelope_version = bytecode
        .get(BYTECODE_ENVELOPE_MAGIC.len()..BYTECODE_ENVELOPE_MAGIC.len() + 4)
        .and_then(|bytes| <[u8; 4]>::try_from(bytes).ok())
        .map(u32::from_le_bytes)
        .ok_or_else(|| compatibility_error("the generated bytecode envelope is truncated."))?;
    if envelope_version != BYTECODE_ENVELOPE_VERSION {
        return Err(compatibility_error(format!(
            "this generated SDK uses bytecode envelope version {envelope_version}, but bridge runtime {} supports envelope version {BYTECODE_ENVELOPE_VERSION}.",
            baml_version::CANONICAL_VERSION
        )));
    }
    let envelope: BytecodeEnvelope = borsh::from_slice(bytecode).map_err(|error| {
        compatibility_error(format!(
            "the generated bytecode envelope is corrupt ({error})."
        ))
    })?;
    let metadata = parse_bytecode_metadata(&envelope.baml_toml)?;
    validate_bytecode_metadata(&metadata)?;
    borsh::from_slice(&envelope.program).map_err(|error| RuntimeError::Compilation {
        message: format!(
            "Failed to deserialize BAML bytecode produced by baml-cli {} with format {}: {error}\n{}",
            metadata.baml_cli_version,
            metadata.bytecode_format_version,
            regeneration_guidance()
        ),
    })
}

/// True iff `err` wraps an engine cancellation panic.
///
/// Centralizes the cancellation-classification logic that bridges and the
/// LSP server need to distinguish cancellation from other runtime errors.
pub fn is_cancelled_runtime_error(err: &RuntimeError) -> bool {
    matches!(err, RuntimeError::Engine(e) if is_cancelled_engine_error(e))
}

/// Keep pass-by-value so the returned `Arc<impl Bex>` does not capture caller locals;
/// taking `&VfsPath` / `&HashMap` would require returning a value that references them.
#[allow(clippy::needless_pass_by_value)]
pub fn new(
    root_path: vfs::VfsPath,
    sys_ops: SysOps,
    files: std::collections::HashMap<crate::fs::FsPath, String>,
) -> Result<Arc<impl Bex>, RuntimeError> {
    let project = project::BexProject::new(&root_path, Arc::new(sys_ops));
    project.update_all_sources(&files);
    let engine = project.take()?;
    Ok(engine)
}

/// Initialize a runtime from the versioned bytecode envelope emitted by
/// `baml generate` rather than from source files. Compatibility metadata is
/// checked before the inner Borsh-encoded program is deserialized.
///
/// This is the blessed seam for running generated bytecode: bridge crates call
/// it instead of reaching into `bex_engine` / `bex_vm_types` themselves.
#[allow(clippy::needless_pass_by_value)]
pub fn new_from_bytecode(bytecode: &[u8], sys_ops: SysOps) -> Result<Arc<dyn Bex>, RuntimeError> {
    let program = deserialize_bytecode(bytecode)?;
    let engine = bex_engine::BexEngine::new(program, Arc::new(sys_ops), Vec::new())?;
    Ok(Arc::new(engine))
}

// Schema types re-exported for `bridge_wasm`, which depends on `bex_project`
// but not `baml_project` and needs to name them in its `From` impl.
pub use baml_project::{FieldSchema, FieldSchemaField, ParamSchema, TypeSchema};
pub use bex_lsp::{
    BackgroundSpawner, BexLsp, FunctionInfo, FunctionKind, FunctionOrigin, LlmCapabilities,
    LspClientSenderTrait, LspError, PlaygroundNotification, PlaygroundSender, PlaygroundSourceFile,
    PreparedRun, ProjectDiagnostic, ProjectUpdate, TestExpandError, new_lsp,
};
pub use fs::{BamlVFS, BulkReadFileSystem, DefaultBulkReadFileSystem, FsPath};

#[cfg(test)]
mod bytecode_compatibility_tests {
    use super::*;

    fn manifest(version: &str, format: u32) -> String {
        format!(
            "[package]\nname = \"test\"\n\n[{BYTECODE_METADATA_TABLE}]\nbaml_cli_version = \"{version}\"\nbytecode_format_version = {format}\n"
        )
    }

    #[test]
    fn metadata_table_name_matches_serde_rename() {
        let parsed: EmbeddedBamlToml = toml::from_str(&manifest(
            baml_version::CANONICAL_VERSION,
            BYTECODE_FORMAT_VERSION,
        ))
        .expect("parse metadata table");
        assert!(parsed.codegen.is_some());
    }

    #[test]
    fn envelope_prefix_layout_is_stable() {
        let bytes = borsh::to_vec(&BytecodeEnvelope {
            magic: BYTECODE_ENVELOPE_MAGIC,
            envelope_version: BYTECODE_ENVELOPE_VERSION,
            baml_toml: String::new(),
            program: Vec::new(),
        })
        .expect("envelope");
        assert_eq!(&bytes[..8], &BYTECODE_ENVELOPE_MAGIC);
        assert_eq!(
            u32::from_le_bytes(bytes[8..12].try_into().expect("version prefix")),
            BYTECODE_ENVELOPE_VERSION
        );
    }

    #[test]
    fn rejects_unsupported_or_truncated_envelope_version() {
        let bytes = borsh::to_vec(&BytecodeEnvelope {
            magic: BYTECODE_ENVELOPE_MAGIC,
            envelope_version: BYTECODE_ENVELOPE_VERSION + 1,
            baml_toml: manifest(baml_version::CANONICAL_VERSION, BYTECODE_FORMAT_VERSION),
            program: Vec::new(),
        })
        .expect("envelope");
        let message = deserialize_bytecode(&bytes)
            .expect_err("unsupported envelope version")
            .to_string();
        assert!(
            message.contains("uses bytecode envelope version"),
            "{message}"
        );
        assert!(
            message.contains(&format!(
                "supports envelope version {BYTECODE_ENVELOPE_VERSION}"
            )),
            "{message}"
        );

        let truncated = &bytes[..BYTECODE_ENVELOPE_MAGIC.len() + 2];
        let message = deserialize_bytecode(truncated)
            .expect_err("truncated envelope version")
            .to_string();
        assert!(message.contains("envelope is truncated"), "{message}");
    }

    #[test]
    fn bytecode_envelope_round_trips_program_and_manifest() {
        let program = bex_vm_types::Program::default();
        let bytes = serialize_bytecode(
            &program,
            &manifest(baml_version::CANONICAL_VERSION, BYTECODE_FORMAT_VERSION),
        )
        .expect("serialize");
        let decoded = deserialize_bytecode(&bytes).expect("deserialize");
        assert_eq!(
            borsh::to_vec(&decoded).expect("decoded program"),
            borsh::to_vec(&program).expect("original program")
        );
    }

    #[test]
    fn rejects_unversioned_bytecode_with_regeneration_guidance() {
        let raw = borsh::to_vec(&bex_vm_types::Program::default()).expect("raw program");
        let error = deserialize_bytecode(&raw).expect_err("raw bytecode must be rejected");
        let message = error.to_string();
        assert!(message.contains("unversioned or malformed"), "{message}");
        assert!(message.contains("baml generate"), "{message}");
        assert!(
            message.contains(baml_version::CANONICAL_VERSION),
            "{message}"
        );
    }

    #[test]
    fn rejects_mismatched_producer_release_before_program_decode() {
        let envelope = BytecodeEnvelope {
            magic: BYTECODE_ENVELOPE_MAGIC,
            envelope_version: BYTECODE_ENVELOPE_VERSION,
            baml_toml: manifest("99.0.0", BYTECODE_FORMAT_VERSION),
            program: vec![1, 2, 3],
        };
        let bytes = borsh::to_vec(&envelope).expect("envelope");
        let message = deserialize_bytecode(&bytes)
            .expect_err("release mismatch")
            .to_string();
        assert!(message.contains("baml-cli 99.0.0"), "{message}");
        assert!(
            message.contains(baml_version::CANONICAL_VERSION),
            "{message}"
        );
        assert!(message.contains("exact BAML release match"), "{message}");
    }

    #[test]
    fn rejects_mismatched_format_before_program_decode() {
        let envelope = BytecodeEnvelope {
            magic: BYTECODE_ENVELOPE_MAGIC,
            envelope_version: BYTECODE_ENVELOPE_VERSION,
            baml_toml: manifest(baml_version::CANONICAL_VERSION, BYTECODE_FORMAT_VERSION + 1),
            program: vec![1, 2, 3],
        };
        let bytes = borsh::to_vec(&envelope).expect("envelope");
        let message = deserialize_bytecode(&bytes)
            .expect_err("format mismatch")
            .to_string();
        assert!(
            message.contains(&format!(
                "uses bytecode format {}",
                BYTECODE_FORMAT_VERSION + 1
            )),
            "{message}"
        );
        assert!(
            message.contains(&format!("supports format {BYTECODE_FORMAT_VERSION}")),
            "{message}"
        );
    }

    #[test]
    fn rejects_embedded_manifest_that_cannot_be_parsed_independently() {
        let envelope = BytecodeEnvelope {
            magic: BYTECODE_ENVELOPE_MAGIC,
            envelope_version: BYTECODE_ENVELOPE_VERSION,
            baml_toml: "[package\n".to_string(),
            program: Vec::new(),
        };
        let bytes = borsh::to_vec(&envelope).expect("envelope");
        let message = deserialize_bytecode(&bytes)
            .expect_err("invalid manifest")
            .to_string();
        assert!(
            message.contains("embedded baml.toml is not valid TOML"),
            "{message}"
        );
        assert!(message.contains("baml generate"), "{message}");
    }
}
