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
}
