//! Cargo development entry point for the packed BAML wrapper.
//! Release artifacts use `baml pack` with the same ordinary runtime host.
use std::process::ExitCode;

fn main() -> ExitCode {
    let envelope = baml_artifact::decode(
        baml_artifact::ArtifactKind::PackedProgram,
        include_bytes!(concat!(env!("OUT_DIR"), "/wrapper.bamlpack")),
    )
    .expect("build-time wrapper envelope must match the runtime");
    baml_pack_host::run(envelope)
}
