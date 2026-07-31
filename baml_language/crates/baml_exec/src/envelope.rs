// `PackEnvelope` is the on-disk shape that `baml pack` writes into the
// embedded section of a packaged binary, and that `baml-pack-host` reads
// back at startup. It wraps the compiled program with the entry metadata
// the host needs to dispatch — the target function (or functions, in
// subcommand mode) to invoke and the output format to use when printing
// the return value.

use std::{error::Error, fmt, mem::size_of};

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

/// Magic bytes at the start of every encoded [`PackEnvelope`].
///
/// Keep this separate from [`PACK_SECTION_NAME`]: the section name helps
/// locate bytes in a native binary, while this prefix proves that the located
/// bytes use the pack-envelope wire format.
pub const PACK_ENVELOPE_MAGIC: [u8; 8] = *b"BAMLPACK";

/// Current pack-envelope wire-format version.
///
/// Packed binaries ship with their matching host, so version skew is an
/// error rather than a compatibility path. Bump this whenever the borsh
/// payload becomes incompatible; do not add a legacy decoder.
pub const PACK_ENVELOPE_VERSION: u32 = 3;

/// Number of bytes before the borsh payload: 8-byte magic + LE `u32` version.
pub const PACK_ENVELOPE_HEADER_LEN: usize = PACK_ENVELOPE_MAGIC.len() + size_of::<u32>();

// Match borsh's `to_vec` starting capacity while reserving the prefix in the
// same allocation. This avoids allocating a second program-sized buffer and
// copying the complete payload just to prepend the header.
const INITIAL_PAYLOAD_CAPACITY: usize = 1024;

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

/// Effective host policy persisted into packaged binaries from
/// `baml.toml [observability]`.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PackObservability {
    pub enabled: bool,
    pub capture_values: bool,
    pub capture_logs: bool,
    pub latency_trigger_ms: Option<u64>,
}

impl Default for PackObservability {
    fn default() -> Self {
        Self {
            enabled: true,
            capture_values: true,
            capture_logs: false,
            latency_trigger_ms: None,
        }
    }
}

/// Wire format embedded into a packaged binary.
///
/// Stable across `baml pack` / `baml-pack-host` versions built from the
/// same source tree. Version-skew is the author's responsibility; a
/// binary packed by `baml pack` ships its own host.
#[derive(Clone, Debug, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PackEnvelope {
    /// Compile identity omitted from `Program`'s own borsh representation.
    ///
    /// The host reattaches this before engine construction. `None` triggers
    /// the deterministic program-content fallback for identity-less inputs.
    pub program_identity: Option<bex_vm_types::ProgramIdentity>,

    /// Source rows used to rebuild the revision dictionary in the packaged
    /// host. Empty means the host keeps the complete embedded metadata-table
    /// fallback.
    pub source_files: Vec<bex_vm_types::ProgramSourceFile>,

    /// The compiled BAML program.
    pub program: Program,

    /// Dispatch shape — single target or subcommand multiplex.
    pub mode: PackMode,

    /// One entry per packed target. In [`PackMode::Single`] this is
    /// exactly one element; in [`PackMode::Subcommand`] one or more.
    pub targets: Vec<TargetEntry>,

    /// Output serialization format, baked in at pack time.
    pub output_format: OutputFormat,

    /// Durable capture and trigger policy baked in with the program.
    pub observability: PackObservability,
}

/// Error returned when an embedded pack envelope cannot be decoded.
#[derive(Debug)]
pub enum PackEnvelopeDecodeError {
    /// The bytes end before the complete magic/version header.
    TruncatedHeader {
        /// Minimum number of bytes required for the header.
        expected: usize,
        /// Number of bytes supplied by the embedded section.
        actual: usize,
    },
    /// The embedded section is not a pack envelope.
    InvalidMagic {
        /// Bytes found where [`PACK_ENVELOPE_MAGIC`] was expected.
        actual: [u8; PACK_ENVELOPE_MAGIC.len()],
    },
    /// The envelope was written with an incompatible wire version.
    UnsupportedVersion {
        /// Version supported by this host.
        expected: u32,
        /// Version stored in the envelope.
        actual: u32,
    },
    /// The versioned borsh payload is malformed or truncated.
    InvalidPayload(std::io::Error),
}

impl fmt::Display for PackEnvelopeDecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TruncatedHeader { expected, actual } => write!(
                f,
                "pack envelope header is truncated: expected at least {expected} bytes, found {actual}"
            ),
            Self::InvalidMagic { actual } => write!(
                f,
                "invalid pack envelope magic: expected {:?}, found {actual:02x?}",
                PACK_ENVELOPE_MAGIC
            ),
            Self::UnsupportedVersion { expected, actual } => write!(
                f,
                "unsupported pack envelope version {actual}; this host supports version {expected}"
            ),
            Self::InvalidPayload(error) => {
                write!(
                    f,
                    "invalid pack envelope v{PACK_ENVELOPE_VERSION} payload: {error}"
                )
            }
        }
    }
}

impl Error for PackEnvelopeDecodeError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidPayload(error) => Some(error),
            Self::TruncatedHeader { .. }
            | Self::InvalidMagic { .. }
            | Self::UnsupportedVersion { .. } => None,
        }
    }
}

/// Encode a pack envelope with the mandatory magic/version prefix.
///
/// All writers must use this helper instead of serializing [`PackEnvelope`]
/// directly so the host can reject incompatible payloads before borsh starts
/// interpreting them.
pub fn encode_pack_envelope(envelope: &PackEnvelope) -> std::io::Result<Vec<u8>> {
    let mut encoded = Vec::with_capacity(PACK_ENVELOPE_HEADER_LEN + INITIAL_PAYLOAD_CAPACITY);
    encoded.extend_from_slice(&PACK_ENVELOPE_MAGIC);
    encoded.extend_from_slice(&PACK_ENVELOPE_VERSION.to_le_bytes());
    borsh::to_writer(&mut encoded, envelope)?;
    Ok(encoded)
}

/// Decode and validate an encoded pack envelope.
///
/// There is intentionally no fallback for the former unversioned, bare-borsh
/// representation: a packed binary embeds its matching host, and accepting an
/// ambiguous legacy payload would make incompatible format changes fail late.
pub fn decode_pack_envelope(bytes: &[u8]) -> Result<PackEnvelope, PackEnvelopeDecodeError> {
    let header =
        bytes
            .get(..PACK_ENVELOPE_HEADER_LEN)
            .ok_or(PackEnvelopeDecodeError::TruncatedHeader {
                expected: PACK_ENVELOPE_HEADER_LEN,
                actual: bytes.len(),
            })?;

    let actual_magic = header[..PACK_ENVELOPE_MAGIC.len()]
        .try_into()
        .expect("magic slice has a fixed length");
    if actual_magic != PACK_ENVELOPE_MAGIC {
        return Err(PackEnvelopeDecodeError::InvalidMagic {
            actual: actual_magic,
        });
    }

    let version = u32::from_le_bytes(
        header[PACK_ENVELOPE_MAGIC.len()..]
            .try_into()
            .expect("version slice has a fixed length"),
    );
    if version != PACK_ENVELOPE_VERSION {
        return Err(PackEnvelopeDecodeError::UnsupportedVersion {
            expected: PACK_ENVELOPE_VERSION,
            actual: version,
        });
    }

    borsh::from_slice(&bytes[PACK_ENVELOPE_HEADER_LEN..])
        .map_err(PackEnvelopeDecodeError::InvalidPayload)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_rejects_every_truncated_header_length() {
        let complete_header = [
            PACK_ENVELOPE_MAGIC.as_slice(),
            PACK_ENVELOPE_VERSION.to_le_bytes().as_slice(),
        ]
        .concat();

        for actual in 0..PACK_ENVELOPE_HEADER_LEN {
            let error = decode_pack_envelope(&complete_header[..actual]).unwrap_err();
            assert!(matches!(
                error,
                PackEnvelopeDecodeError::TruncatedHeader {
                    expected: PACK_ENVELOPE_HEADER_LEN,
                    actual: found,
                } if found == actual
            ));
        }
    }

    #[test]
    fn decode_rejects_wrong_magic_before_reading_payload() {
        let mut bytes = vec![0; PACK_ENVELOPE_HEADER_LEN];
        bytes[..PACK_ENVELOPE_MAGIC.len()].copy_from_slice(b"NOTAPACK");
        bytes[PACK_ENVELOPE_MAGIC.len()..].copy_from_slice(&PACK_ENVELOPE_VERSION.to_le_bytes());

        let error = decode_pack_envelope(&bytes).unwrap_err();
        assert!(matches!(
            error,
            PackEnvelopeDecodeError::InvalidMagic {
                actual
            } if actual == *b"NOTAPACK"
        ));
        assert!(error.to_string().contains("invalid pack envelope magic"));
    }

    #[test]
    fn decode_rejects_unsupported_version_before_reading_payload() {
        let unsupported = PACK_ENVELOPE_VERSION + 1;
        let mut bytes = Vec::from(PACK_ENVELOPE_MAGIC);
        bytes.extend_from_slice(&unsupported.to_le_bytes());

        let error = decode_pack_envelope(&bytes).unwrap_err();
        assert!(matches!(
            error,
            PackEnvelopeDecodeError::UnsupportedVersion {
                expected: PACK_ENVELOPE_VERSION,
                actual,
            } if actual == unsupported
        ));
        assert!(error.to_string().contains(&unsupported.to_string()));
    }

    #[test]
    fn decode_rejects_truncated_payload() {
        let mut bytes = Vec::from(PACK_ENVELOPE_MAGIC);
        bytes.extend_from_slice(&PACK_ENVELOPE_VERSION.to_le_bytes());

        let error = decode_pack_envelope(&bytes).unwrap_err();
        assert!(matches!(error, PackEnvelopeDecodeError::InvalidPayload(_)));
        assert!(error.to_string().contains(&format!(
            "invalid pack envelope v{PACK_ENVELOPE_VERSION} payload"
        )));
    }
}
