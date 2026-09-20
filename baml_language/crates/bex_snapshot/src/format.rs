//! The snapshot container: magic, header, sections, checksum.
//!
//! ```text
//! magic            8 bytes   "BAMLSNAP"
//! format_version   u32 LE
//! header_len       u32 LE
//! header           Borsh(HeaderWire)
//! program section  u8 present, then a section when present
//! state section    section
//! checksum         32 bytes  SHA-256 of every preceding byte
//!
//! section := codec u8 (0 raw, 1 zstd) | raw_len u64 LE | stored_len u64 LE | bytes
//! ```
//!
//! The header sits outside the compressed sections so that a reader can
//! identify a file without decompressing it. The state section is described
//! in `wire.rs`.
//!
//! Every length is checked against the input size and against a fixed limit
//! before anything is allocated.

use borsh::{BorshDeserialize, BorshSerialize};
use sha2::{Digest, Sha256};

use crate::{FORMAT_VERSION, SnapshotError, SnapshotHeader};

pub(crate) const MAGIC: &[u8; 8] = b"BAMLSNAP";
const CHECKSUM_LEN: usize = 32;
const MAX_HEADER_LEN: usize = 1 << 20;
/// Upper bound on the decompressed size of one section.
pub(crate) const MAX_SECTION_LEN: u64 = 1 << 30;

const CODEC_RAW: u8 = 0;
const CODEC_ZSTD: u8 = 1;
const ZSTD_LEVEL: i32 = 1;

#[derive(BorshSerialize, BorshDeserialize)]
struct HeaderWire {
    runtime_build: String,
    program_hash: [u8; 32],
    run_id: String,
    segment: u32,
    seq: u64,
    created_unix_ms: u64,
}

/// The decoded container, with both sections decompressed.
pub(crate) struct Container {
    pub header: SnapshotHeader,
    pub program: Option<Vec<u8>>,
    pub state: Vec<u8>,
}

/// Sizes recorded while encoding, for `WriteStats`.
pub(crate) struct EncodedSizes {
    pub state_stored: u64,
    pub compress_ms: f64,
}

fn format_err(message: impl Into<String>) -> SnapshotError {
    SnapshotError::Format(message.into())
}

fn write_section(out: &mut Vec<u8>, raw: &[u8], compress: bool) -> Result<u64, SnapshotError> {
    // The reader refuses a larger section, so refuse to write one: a snapshot
    // that can never be restored must not be reported as written.
    if raw.len() as u64 > MAX_SECTION_LEN {
        return Err(format_err(format!(
            "snapshot section of {} bytes exceeds the limit of {MAX_SECTION_LEN} bytes",
            raw.len()
        )));
    }
    let (codec, stored) = if compress {
        let stored = zstd::bulk::compress(raw, ZSTD_LEVEL).map_err(SnapshotError::Io)?;
        (CODEC_ZSTD, std::borrow::Cow::Owned(stored))
    } else {
        (CODEC_RAW, std::borrow::Cow::Borrowed(raw))
    };
    out.push(codec);
    out.extend_from_slice(&(raw.len() as u64).to_le_bytes());
    out.extend_from_slice(&(stored.len() as u64).to_le_bytes());
    out.extend_from_slice(&stored);
    Ok(stored.len() as u64)
}

pub(crate) fn encode(
    header: &SnapshotHeader,
    program: Option<&[u8]>,
    state: &[u8],
    compress: bool,
) -> Result<(Vec<u8>, EncodedSizes), SnapshotError> {
    let header_bytes = borsh::to_vec(&HeaderWire {
        runtime_build: header.runtime_build.clone(),
        program_hash: header.program_hash,
        run_id: header.run_id.clone(),
        segment: header.segment,
        seq: header.seq,
        created_unix_ms: header.created_unix_ms,
    })
    .map_err(SnapshotError::Io)?;
    if header_bytes.len() > MAX_HEADER_LEN {
        return Err(format_err("snapshot header is too large"));
    }

    let mut out = Vec::with_capacity(state.len() / 2 + header_bytes.len() + 128);
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
    out.extend_from_slice(
        &u32::try_from(header_bytes.len())
            .unwrap_or(u32::MAX)
            .to_le_bytes(),
    );
    out.extend_from_slice(&header_bytes);

    let started = std::time::Instant::now();
    match program {
        Some(program) => {
            out.push(1);
            write_section(&mut out, program, compress)?;
        }
        None => out.push(0),
    }
    let state_stored = write_section(&mut out, state, compress)?;
    let compress_ms = if compress {
        started.elapsed().as_secs_f64() * 1000.0
    } else {
        0.0
    };

    let checksum = Sha256::digest(&out);
    out.extend_from_slice(&checksum);
    Ok((
        out,
        EncodedSizes {
            state_stored,
            compress_ms,
        },
    ))
}

/// A bounds-checked cursor over the input.
struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, len: usize) -> Result<&'a [u8], SnapshotError> {
        let end = self
            .pos
            .checked_add(len)
            .filter(|end| *end <= self.bytes.len())
            .ok_or_else(|| format_err("snapshot is truncated"))?;
        let slice = &self.bytes[self.pos..end];
        self.pos = end;
        Ok(slice)
    }

    fn u8(&mut self) -> Result<u8, SnapshotError> {
        Ok(self.take(1)?[0])
    }

    fn u32(&mut self) -> Result<u32, SnapshotError> {
        let mut buf = [0u8; 4];
        buf.copy_from_slice(self.take(4)?);
        Ok(u32::from_le_bytes(buf))
    }

    fn u64(&mut self) -> Result<u64, SnapshotError> {
        let mut buf = [0u8; 8];
        buf.copy_from_slice(self.take(8)?);
        Ok(u64::from_le_bytes(buf))
    }
}

fn read_header_from(reader: &mut Reader<'_>) -> Result<SnapshotHeader, SnapshotError> {
    if reader.take(MAGIC.len())? != MAGIC {
        return Err(format_err("not a BAML snapshot (bad magic)"));
    }
    let format_version = reader.u32()?;
    if format_version != FORMAT_VERSION {
        let hint = if format_version < FORMAT_VERSION {
            "the snapshot was written by an older runtime and cannot be resumed by this one; \
             start the run again"
        } else {
            "the snapshot was written by a newer runtime"
        };
        return Err(SnapshotError::Mismatch(format!(
            "snapshot format version {format_version} is not supported (this build reads \
             version {FORMAT_VERSION}): {hint}"
        )));
    }
    let header_len = reader.u32()? as usize;
    if header_len > MAX_HEADER_LEN {
        return Err(format_err("snapshot header length is out of range"));
    }
    let wire = HeaderWire::try_from_slice(reader.take(header_len)?)
        .map_err(|err| format_err(format!("snapshot header does not decode: {err}")))?;
    Ok(SnapshotHeader {
        format_version,
        runtime_build: wire.runtime_build,
        program_hash: wire.program_hash,
        run_id: wire.run_id,
        segment: wire.segment,
        seq: wire.seq,
        created_unix_ms: wire.created_unix_ms,
    })
}

fn read_section(reader: &mut Reader<'_>, decode: bool) -> Result<Vec<u8>, SnapshotError> {
    let codec = reader.u8()?;
    let raw_len = reader.u64()?;
    let stored_len = reader.u64()?;
    if raw_len > MAX_SECTION_LEN {
        return Err(format_err("snapshot section is larger than the limit"));
    }
    let stored_len =
        usize::try_from(stored_len).map_err(|_| format_err("snapshot section length overflows"))?;
    let stored = reader.take(stored_len)?;
    if !decode {
        return Ok(Vec::new());
    }
    let raw = match codec {
        CODEC_RAW => stored.to_vec(),
        CODEC_ZSTD => {
            use std::io::Read;
            // Read at most one byte past the declared size, so that a
            // mis-declared (or hostile) frame cannot expand without bound.
            let mut raw = Vec::new();
            zstd::stream::read::Decoder::new(stored)
                .map_err(|err| format_err(format!("zstd stream does not open: {err}")))?
                .take(raw_len + 1)
                .read_to_end(&mut raw)
                .map_err(|err| format_err(format!("zstd stream does not decode: {err}")))?;
            raw
        }
        other => return Err(format_err(format!("unknown section codec {other}"))),
    };
    if raw.len() as u64 != raw_len {
        return Err(format_err("snapshot section has the wrong decoded size"));
    }
    Ok(raw)
}

/// Parse the header only. Does not verify the checksum.
pub(crate) fn read_header(bytes: &[u8]) -> Result<SnapshotHeader, SnapshotError> {
    read_header_from(&mut Reader { bytes, pos: 0 })
}

/// Verify the checksum and decode the container. With `want_state == false`
/// the state section is skipped (its bounds are still checked).
pub(crate) fn decode(bytes: &[u8], want_state: bool) -> Result<Container, SnapshotError> {
    if bytes.len() < MAGIC.len() + CHECKSUM_LEN {
        return Err(format_err("snapshot is truncated"));
    }
    let (body, checksum) = bytes.split_at(bytes.len() - CHECKSUM_LEN);
    // Check the magic first so that a file of another kind gets the clearer
    // message.
    if &body[..MAGIC.len()] != MAGIC {
        return Err(format_err("not a BAML snapshot (bad magic)"));
    }
    if Sha256::digest(body).as_slice() != checksum {
        return Err(format_err(
            "snapshot checksum does not match (file is corrupt)",
        ));
    }

    let mut reader = Reader {
        bytes: body,
        pos: 0,
    };
    let header = read_header_from(&mut reader)?;
    let program = match reader.u8()? {
        0 => None,
        1 => Some(read_section(&mut reader, true)?),
        other => return Err(format_err(format!("unknown program section tag {other}"))),
    };
    let state = read_section(&mut reader, want_state)?;
    if reader.pos != body.len() {
        return Err(format_err("snapshot has trailing bytes"));
    }
    Ok(Container {
        header,
        program,
        state,
    })
}
