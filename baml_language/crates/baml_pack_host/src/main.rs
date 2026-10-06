use std::process::ExitCode;

fn main() -> ExitCode {
    let envelope = libsui::find_section(baml_exec::PACK_SECTION_NAME)
        .map_err(|error| format!("Failed to read embedded section: {error}"))
        .and_then(|section| {
            section.ok_or_else(|| {
                "No embedded BAML package found. Build this binary with `baml pack`.".into()
            })
        })
        .and_then(|section| {
            baml_artifact::decode(baml_artifact::ArtifactKind::PackedProgram, section)
                .map_err(|error| format!("Failed to deserialize pack envelope: {error}"))
        });
    match envelope {
        Ok(envelope) => baml_pack_host::run(envelope),
        Err(error) => {
            baml_exec::print_error(error);
            ExitCode::FAILURE
        }
    }
}
