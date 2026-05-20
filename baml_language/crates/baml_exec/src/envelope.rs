// `PackEnvelope` is the on-disk shape that `baml pack` writes into the
// embedded section of a packaged binary, and that `baml-pack-host` reads
// back at startup. It wraps the compiled program with the entry metadata
// the host needs to dispatch — the target function to invoke and the
// output format to use when printing the return value.

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

/// Wire format embedded into a packaged binary.
///
/// Stable across `baml pack` / `baml-pack-host` versions built from the
/// same source tree. Version-skew is the author's responsibility; a
/// binary packed by `baml pack` ships its own host.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct PackEnvelope {
    /// The compiled BAML program.
    pub program: Program,

    /// Fully qualified name of the entry-point function, baked in at
    /// pack time. Host invokes this as the entry point.
    pub target_name: String,

    /// `argv[1]` for the running binary, per BEP-027 §"baml.argv in
    /// packaged binaries". For file-backed targets this is the file's
    /// basename; otherwise the qualified function/namespace name or
    /// the literal `"main"` for root main packages.
    pub target_identifier: String,

    /// Output serialization format, baked in at pack time.
    pub output_format: OutputFormat,
}
