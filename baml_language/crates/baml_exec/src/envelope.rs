// `PackEnvelope` is the on-disk shape that `baml pack` writes into the
// embedded section of a packaged binary, and that `baml-pack-host` reads
// back at startup. It wraps the compiled program with the entry metadata
// the host needs to dispatch — the target function (or functions, in
// subcommand mode) to invoke and the output format to use when printing
// the return value.
//
// On the wire the envelope is framed: an 8-byte magic plus a u32 format
// version precede the borsh payload (see [`PackEnvelope::encode_framed`]).

use std::io;

use bex_vm_types::types::Program;
use thiserror::Error;

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

/// Magic that opens a framed [`PackEnvelope`]. Eight bytes (NUL-padded
/// ASCII) so the version `u32` that follows sits naturally aligned and
/// the whole header is a fixed 12 bytes.
pub const PACK_ENVELOPE_MAGIC: [u8; 8] = *b"BAMLPKG\0";

/// Current — and only — framed-envelope format version.
///
/// A packed binary embeds the envelope next to its own matching host
/// (libsui writes both into the same executable), so a host essentially
/// never sees an envelope written by a different toolchain version. The
/// version prefix therefore exists to *fail loudly* on the rare corrupt
/// or hand-assembled binary, not to enable migration: bump it on any
/// breaking payload change and let old readers reject the frame — no
/// legacy decoder is kept.
pub const PACK_ENVELOPE_FORMAT_VERSION: u32 = 1;

/// Byte length of the frame header: magic + little-endian version.
const FRAME_HEADER_LEN: usize = PACK_ENVELOPE_MAGIC.len() + size_of::<u32>();

/// Failure modes of [`PackEnvelope::decode_framed`].
///
/// Split into variants so callers (and their error messages) can tell
/// "this section is not a framed envelope at all" apart from "this is a
/// framed envelope from a newer toolchain" — the latter names both
/// versions so the fix (repack) is obvious from the message alone.
#[derive(Debug, Error)]
pub enum PackEnvelopeDecodeError {
    /// The section is not a framed pack envelope: it is too short to
    /// hold the 12-byte header, or the magic bytes don't match. Covers
    /// sections written by a pre-framing `baml pack` as well as plain
    /// corruption.
    #[error(
        "embedded section is not a framed pack envelope (missing `BAMLPKG` magic); \
         was this binary produced by an incompatible `baml pack`?"
    )]
    BadMagic,

    /// The frame is well-formed but declares a format version this
    /// reader does not support. Per the versioning policy there is no
    /// cross-version decoding — repacking with a matching toolchain is
    /// the only fix.
    #[error(
        "unsupported pack envelope format version {found} (this host supports version \
         {supported}); repack with a matching `baml` toolchain"
    )]
    UnsupportedVersion {
        /// Version declared by the frame header.
        found: u32,
        /// Version this reader supports ([`PACK_ENVELOPE_FORMAT_VERSION`]).
        supported: u32,
    },

    /// Header checked out but the borsh payload did not decode —
    /// truncated or corrupt program bytes.
    #[error("malformed pack envelope payload: {0}")]
    Borsh(#[from] io::Error),
}

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
/// Stable across `baml pack` / `baml-pack-host` versions built from the
/// same source tree. Version-skew is the author's responsibility; a
/// binary packed by `baml pack` ships its own host.
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

impl PackEnvelope {
    /// Serialize as the framed wire format written into the packed
    /// binary's embedded section:
    ///
    /// ```text
    /// [ PACK_ENVELOPE_MAGIC : 8 bytes ]
    /// [ PACK_ENVELOPE_FORMAT_VERSION : u32 little-endian ]
    /// [ borsh(PackEnvelope) : rest of section ]
    /// ```
    pub fn encode_framed(&self) -> io::Result<Vec<u8>> {
        let mut out = Vec::with_capacity(FRAME_HEADER_LEN);
        out.extend_from_slice(&PACK_ENVELOPE_MAGIC);
        out.extend_from_slice(&PACK_ENVELOPE_FORMAT_VERSION.to_le_bytes());
        borsh::BorshSerialize::serialize(self, &mut out)?;
        Ok(out)
    }

    /// Decode the framed wire format produced by [`Self::encode_framed`].
    ///
    /// There is deliberately no fallback to older frame layouts (nor to
    /// the historical bare-borsh encoding): a packed binary carries the
    /// envelope alongside its own matching host, so cross-version reads
    /// essentially cannot occur in practice. The header exists so that
    /// when the impossible happens anyway, the failure is a precise
    /// error instead of borsh misreading unrelated bytes.
    pub fn decode_framed(bytes: &[u8]) -> Result<Self, PackEnvelopeDecodeError> {
        // Too short for magic + version, or wrong magic: either way this
        // is not a well-formed frame, and the caller can't distinguish
        // further, so both collapse into `BadMagic`.
        let Some((magic, rest)) = bytes.split_first_chunk::<8>() else {
            return Err(PackEnvelopeDecodeError::BadMagic);
        };
        if *magic != PACK_ENVELOPE_MAGIC {
            return Err(PackEnvelopeDecodeError::BadMagic);
        }
        let Some((version_bytes, payload)) = rest.split_first_chunk::<4>() else {
            return Err(PackEnvelopeDecodeError::BadMagic);
        };

        let version = u32::from_le_bytes(*version_bytes);
        if version != PACK_ENVELOPE_FORMAT_VERSION {
            return Err(PackEnvelopeDecodeError::UnsupportedVersion {
                found: version,
                supported: PACK_ENVELOPE_FORMAT_VERSION,
            });
        }

        Ok(borsh::from_slice(payload)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Minimal envelope for wire-format tests. `Program::default()`
    /// suffices: the framing tests exercise the header, not the program
    /// body (the full compiled-program roundtrip lives in
    /// `baml_cli::pack_command`'s tests).
    fn sample_envelope() -> PackEnvelope {
        PackEnvelope {
            program: Program::default(),
            mode: PackMode::Single,
            targets: vec![TargetEntry {
                qualified_name: "user.main".to_string(),
                display_name: "main".to_string(),
                subcommand_name: "main".to_string(),
            }],
            output_format: OutputFormat::Json,
        }
    }

    #[test]
    fn framed_roundtrip() {
        let framed = sample_envelope().encode_framed().unwrap();

        // The frame must open with the fixed 12-byte header.
        assert_eq!(framed[..8], PACK_ENVELOPE_MAGIC);
        assert_eq!(
            framed[8..12],
            PACK_ENVELOPE_FORMAT_VERSION.to_le_bytes(),
            "version must be little-endian right after the magic"
        );

        let decoded = PackEnvelope::decode_framed(&framed).unwrap();
        assert!(matches!(decoded.mode, PackMode::Single));
        assert_eq!(decoded.targets.len(), 1);
        assert_eq!(decoded.targets[0].qualified_name, "user.main");
        assert!(matches!(decoded.output_format, OutputFormat::Json));
    }

    #[test]
    fn decode_framed_rejects_bad_magic() {
        // Bare borsh bytes — what a pre-framing `baml pack` embedded —
        // must be rejected up front, not misread as a frame.
        let bare = borsh::to_vec(&sample_envelope()).unwrap();
        assert!(matches!(
            PackEnvelope::decode_framed(&bare),
            Err(PackEnvelopeDecodeError::BadMagic)
        ));

        // A single flipped magic byte is enough to reject.
        let mut framed = sample_envelope().encode_framed().unwrap();
        framed[0] ^= 0xFF;
        assert!(matches!(
            PackEnvelope::decode_framed(&framed),
            Err(PackEnvelopeDecodeError::BadMagic)
        ));
    }

    #[test]
    fn decode_framed_rejects_future_version() {
        let mut framed = sample_envelope().encode_framed().unwrap();
        let future = PACK_ENVELOPE_FORMAT_VERSION + 1;
        framed[8..12].copy_from_slice(&future.to_le_bytes());

        let err = PackEnvelope::decode_framed(&framed).unwrap_err();
        assert!(matches!(
            err,
            PackEnvelopeDecodeError::UnsupportedVersion {
                found: 2,
                supported: 1,
            }
        ));

        // The message must name both versions so "repack with a matching
        // toolchain" is actionable from the error text alone.
        let msg = err.to_string();
        assert!(msg.contains("version 2"), "found version missing: {msg}");
        assert!(
            msg.contains("version 1"),
            "supported version missing: {msg}"
        );
    }

    #[test]
    fn decode_framed_rejects_truncated_input() {
        let framed = sample_envelope().encode_framed().unwrap();

        // Cut inside the 12-byte header: not a well-formed frame at all.
        for len in [0, 4, 8, 11] {
            assert!(
                matches!(
                    PackEnvelope::decode_framed(&framed[..len]),
                    Err(PackEnvelopeDecodeError::BadMagic)
                ),
                "header truncated to {len} bytes must fail as BadMagic"
            );
        }

        // Cut inside the borsh payload: header parses, payload does not.
        assert!(matches!(
            PackEnvelope::decode_framed(&framed[..framed.len() - 1]),
            Err(PackEnvelopeDecodeError::Borsh(_))
        ));
    }
}
