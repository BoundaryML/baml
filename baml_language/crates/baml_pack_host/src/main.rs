// The `baml pack` host binary: the packed program lives in an OS-native
// section patched into this executable; see the crate docs.

// `process::exit` is intentional — the BAML target may call
// `baml.sys.exit(code)` to short-circuit the engine with a specific
// exit code, which we honour by terminating the process directly.
#![allow(clippy::print_stdout, clippy::print_stderr, clippy::exit)]

use std::process::ExitCode;

use baml_exec::{PACK_SECTION_NAME, PackEnvelope, print_error};

fn extract_envelope() -> Result<PackEnvelope, String> {
    let section = libsui::find_section(PACK_SECTION_NAME)
        .map_err(|e| format!("Failed to read embedded section: {e}"))?
        .ok_or("No embedded BAML package found. This binary must be built with `baml pack`.")?;

    let envelope: PackEnvelope =
        baml_artifact::decode(baml_artifact::ArtifactKind::PackedProgram, section)
            .map_err(|e| format!("Failed to deserialize pack envelope: {e}"))?;
    envelope
        .verify_telemetry()
        .map_err(|error| error.to_string())?;
    Ok(envelope)
}

fn main() -> ExitCode {
    match extract_envelope() {
        Ok(envelope) => baml_pack_host::run_envelope(envelope, None),
        Err(e) => {
            print_error(e);
            ExitCode::FAILURE
        }
    }
}
