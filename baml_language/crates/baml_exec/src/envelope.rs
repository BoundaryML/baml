// `PackEnvelope` is the on-disk shape that `baml pack` writes into the
// embedded section of a packaged binary, and that `baml-pack-host` reads
// back at startup. It wraps the compiled program with the entry metadata
// the host needs to dispatch — the target function to invoke and the
// output format to use when printing the return value.

use bex_vm_types::types::Program;

use crate::output::OutputFormat;

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
