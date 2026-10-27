use std::sync::Arc;

use bex_engine::{
    BexEngine, CallId, CancellationToken, EngineConfig, EngineError, FunctionCallContextBuilder,
};
use sys_native::SysOpsExt;

pub fn main(artifact: &[u8], table: &'static bex_vm::CompiledBodyTable) -> std::process::ExitCode {
    let (envelope, artifact_fingerprint): (baml_exec::PackEnvelope, [u8; 32]) =
        baml_artifact::decode_with_digest(baml_artifact::ArtifactKind::PackedProgram, artifact)
            .unwrap();
    #[allow(clippy::disallowed_methods, reason = "test harness configuration")]
    let compiled_bodies = (std::env::var("BAML_COMPILED_BODIES").as_deref() != Ok("0")).then_some(
        bex_engine::CompiledBodies {
            table,
            artifact_fingerprint,
        },
    );
    let config = EngineConfig {
        artifact_telemetry: Some(envelope.telemetry),
        compiled_bodies,
        ..Default::default()
    };
    let runtime = tokio::runtime::Runtime::new().unwrap();
    match runtime.block_on(collect(envelope.program, config)) {
        Ok(value) => {
            println!("{value}");
            std::process::ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}");
            std::process::ExitCode::FAILURE
        }
    }
}

async fn collect(
    program: bex_vm_types::Program,
    config: EngineConfig,
) -> Result<String, EngineError> {
    let engine = Arc::new(BexEngine::new_with_config(
        program,
        Arc::new(sys_native::SysOps::native()),
        Vec::new(),
        config,
    )?);
    let registry = engine
        .collect_tests(CallId::next(), CancellationToken::default())
        .await?;
    let serialized = engine
        .call_function(
            "testing.TestRegistry.serialize",
            vec![registry.clone()],
            FunctionCallContextBuilder::new(CallId::next()).build(),
            true,
        )
        .await?;
    let outcomes = engine
        .call_function(
            "InitializerTestSummary",
            vec![registry],
            FunctionCallContextBuilder::new(CallId::next()).build(),
            true,
        )
        .await?;
    Ok(format!("{serialized:?}\n{outcomes:?}"))
}
