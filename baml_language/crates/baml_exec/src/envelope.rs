// `PackEnvelope` is the on-disk shape that `baml pack` writes into the
// embedded section of a packaged binary, and that `baml-pack-host` reads
// back at startup. It wraps the compiled program with the entry metadata
// the host needs to dispatch — the target function (or functions, in
// subcommand mode) to invoke and the output format to use when printing
// the return value.

use bex_vm_types::types::Program;

use crate::output::OutputFormat;

/// Name of the embedded section that holds the [`PackEnvelope`] inside a
/// packaged binary. Read by `baml_pack_host` at startup, written by
/// `baml_cli::pack_command` at pack time. Both ends reference this
/// const so a rename can't desync — a stale literal on one side would
/// only surface at runtime when a packed binary fails to load.
///
/// Fits the 16-byte Mach-O `sectname` cap. Plain `[a-z]` so every
/// libsui backend (Mach-O / ELF / PE-resource) handles it cleanly.
pub const PACK_SECTION_NAME: &str = "baaaaaaaaaaaaaml";

/// One entry-point baked into a packaged binary.
#[derive(Clone, Debug, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct TargetEntry {
    /// Fully qualified name of the target function (engine form, includes
    /// any `user.` prefix). Used to look up the function at dispatch.
    pub qualified_name: String,

    /// Display name (qualified, but with the `user.` prefix stripped).
    /// Drives the per-target help text and `argv[1]` in single-target mode.
    pub display_name: String,

    /// CLI subcommand name in subcommand mode — the last `.`-segment of
    /// `display_name`. In single-target mode this is the value packed
    /// binaries surface as `argv[1]`.
    pub subcommand_name: String,
}

/// Dispatch shape baked into the binary.
#[derive(Clone, Debug, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub enum PackMode {
    /// One target, no subcommand layer. Flags on the binary bind directly
    /// to the target's parameters: `./summarize --text=hi`.
    Single,
    /// Multiple targets, each as a subcommand: `./cli summarize --text=hi`.
    /// Also used when the user passes a single `-f/--function`: the
    /// subcommand layer is forced so signature changes are visible.
    Subcommand,
}

/// Wire format embedded into a packaged binary.
///
/// Wrapped in a `baml_artifact::ArtifactKind::PackedProgram` envelope at the
/// CLI/host boundary, so a host built with a different format or canary
/// fingerprint rejects it before Borsh decodes this type.
#[derive(Clone, Debug, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PackEnvelope {
    /// The compiled BAML program.
    pub program: Program,

    /// Dispatch shape — single target or subcommand multiplex.
    pub mode: PackMode,

    /// One entry per packed target. In [`PackMode::Single`] this is
    /// exactly one element; in [`PackMode::Subcommand`] one or more.
    pub targets: Vec<TargetEntry>,

    /// Output serialization format, baked in at pack time.
    pub output_format: OutputFormat,
    /// Recording defaults and the runtime overrides permitted by the publisher.
    pub telemetry: btel_settings::artifact::ArtifactTelemetry,
}

impl PackEnvelope {
    /// Dispatch metadata and the complete versioned program are part of the build.
    pub fn telemetry_digest(
        &self,
    ) -> Result<btel_settings::artifact::BytecodeDigest, btel_settings::artifact::PolicyError> {
        let mut unsigned = self.clone();
        unsigned.telemetry.embedded = None;
        let payload = baml_artifact::encode(baml_artifact::ArtifactKind::PackedProgram, &unsigned)
            .map_err(|_| {
                btel_settings::artifact::PolicyError(
                    "Could not encode telemetry build fingerprint.".into(),
                )
            })?;
        btel_settings::artifact::build_digest(&payload, &unsigned.telemetry)
    }
    pub fn verify_telemetry(&self) -> Result<(), btel_settings::artifact::PolicyError> {
        if let Some(embedded) = &self.telemetry.embedded {
            if embedded.bytecode_digest != self.telemetry_digest()? {
                return Err(btel_settings::artifact::PolicyError("Embedded telemetry does not match this packed artifact. Rebuild with `baml pack --embed-telemetry`.".into()));
            }
            if !embedded.is_well_formed() {
                return Err(btel_settings::artifact::PolicyError("The embedded telemetry credential is invalid. Rebuild with `baml pack --embed-telemetry`.".into()));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use btel_settings::artifact::{
        ArtifactTelemetry, EmbeddedIngestion, IngestionDestination, PublicCredential,
    };

    use super::*;

    fn embedded_envelope() -> PackEnvelope {
        let mut envelope = PackEnvelope {
            program: Program::default(),
            mode: PackMode::Single,
            targets: vec![TargetEntry {
                qualified_name: "user.main".into(),
                display_name: "main".into(),
                subcommand_name: "main".into(),
            }],
            output_format: OutputFormat::Json,
            telemetry: ArtifactTelemetry::default(),
        };
        envelope.telemetry.embedded = Some(Box::new(EmbeddedIngestion {
            token: PublicCredential::new(format!("bdry_public_{}", "a".repeat(64))),
            token_id: "token".into(),
            build_id: "build".into(),
            product_id: "product".into(),
            destination: IngestionDestination {
                org_id: "org".into(),
                project_id: "project".into(),
                environment_id: "environment".into(),
            },
            bytecode_digest: envelope.telemetry_digest().unwrap(),
        }));
        envelope
    }

    #[test]
    fn packed_credentials_require_a_build_and_complete_destination() {
        let valid = embedded_envelope();
        valid.verify_telemetry().unwrap();
        for field in [
            "token",
            "build_id",
            "org_id",
            "project_id",
            "environment_id",
        ] {
            let mut envelope = valid.clone();
            let embedded = envelope.telemetry.embedded.as_mut().unwrap();
            match field {
                "token" => embedded.token = PublicCredential::new("bdry_secret_not_public".into()),
                "build_id" => embedded.build_id.clear(),
                "org_id" => embedded.destination.org_id.clear(),
                "project_id" => embedded.destination.project_id.clear(),
                "environment_id" => embedded.destination.environment_id.clear(),
                _ => unreachable!(),
            }
            assert_eq!(
                envelope.verify_telemetry().unwrap_err().to_string(),
                r#"The embedded telemetry credential is invalid. Rebuild with `baml pack --embed-telemetry`."#,
                "invalid {field} must fail even when the envelope digest matches"
            );
        }
    }

    #[test]
    fn packed_fingerprint_still_covers_dispatch_metadata() {
        let mut envelope = embedded_envelope();
        envelope.targets[0].qualified_name = "user.other".into();
        assert_eq!(
            envelope.verify_telemetry().unwrap_err().to_string(),
            r#"Embedded telemetry does not match this packed artifact. Rebuild with `baml pack --embed-telemetry`."#
        );
    }
}
