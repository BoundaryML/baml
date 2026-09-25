//! Runtime recording configuration, separate from CLI usage analytics.
use std::{path::Path, sync::Arc};

use bex_engine::{BexEngine, EngineError, TelemetryRecording};
use sys_native::SysOpsExt;

/// Use the CLI's resolved project root, including on bytecode-cache hits.
/// Engine telemetry-off mode ignores the destination without creating files.
pub(crate) fn create_engine(
    program: bex_vm_types::Program,
    argv: Vec<String>,
    project_root: &Path,
) -> Result<BexEngine, EngineError> {
    BexEngine::new_with_telemetry_recording(
        program,
        Arc::new(sys_native::SysOps::native()),
        argv,
        Some(bex_project::runtime_compiler()),
        btel_settings::clock::DEFAULT_MODE,
        TelemetryRecording::local_files(
            project_root,
            btel_settings::publisher::RecordingConfig::default(),
        ),
    )
}
