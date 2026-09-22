use std::sync::Arc;

use bex_engine::{BexEngine, TelemetryRecording};
use sys_native::SysOpsExt;

pub(crate) fn recording_engine(
    source: &str,
    captured_functions: &[&str],
    recording: TelemetryRecording,
) -> Arc<BexEngine> {
    let mut program = baml_db::testing::compile_source(source);
    // Exercise full-capture AI policy without making an LLM request.
    for object in &mut program.objects.0 {
        if let bex_vm_types::Object::Function(function) = object {
            if captured_functions
                .iter()
                .any(|name| function.name.rsplit('.').next() == Some(name))
            {
                function.body_meta = Some(bex_vm_types::FunctionMeta::Llm {
                    client: "test".into(),
                });
            }
        }
    }
    Arc::new(
        BexEngine::new_with_telemetry_recording(
            program,
            Arc::new(sys_native::SysOps::native()),
            vec![],
            None,
            btel_clock::ClockMode::Monotonic,
            recording,
        )
        .unwrap(),
    )
}
