//! Raw ring records: the producer→consumer wire format (v2 §4.1).
//!
//! Every record is `tag: u8` followed by a fixed little-endian field layout
//! per tag; only `StartThread` carries a variable-length tail (the thread
//! name, capped at [`MAX_THREAD_NAME_LEN`] bytes). Producer and consumer live
//! in the same binary, so the tag→layout table is compiled in — there is no
//! self-describing framing on the ring.
//!
//! Producers commit whole records (the ring's `commit_len` always lands on a
//! record boundary) and never split a record across segments, so a drained
//! byte range is always a whole number of records.
//!
//! Id conventions: `call_id` counters start at 1, so `0` means "none" in
//! `parent_call_id`/`parent_thread_id` fields. `engine_id`/`process_id` are
//! deliberately absent — they are per-ring and per-file-header state.

/// Maximum thread-name bytes carried by a [`RawRecord::StartThread`] record.
pub const MAX_THREAD_NAME_LEN: usize = 256;

/// Encoded sizes, tag byte included.
pub const CALL_FUNCTION_LEN: usize = 54;
/// See [`CALL_FUNCTION_LEN`].
pub const END_FUNCTION_LEN: usize = 26;
/// Fixed prefix of a `StartThread` record; the name bytes follow.
pub const START_THREAD_FIXED_LEN: usize = 36;
/// See [`CALL_FUNCTION_LEN`].
pub const END_THREAD_LEN: usize = 18;
/// See [`CALL_FUNCTION_LEN`].
pub const SET_FUNCTION_ID_LEN: usize = 41;

/// Upper bound on any encoded record; sizes producer-side stack buffers.
pub const MAX_RECORD_LEN: usize = START_THREAD_FIXED_LEN + MAX_THREAD_NAME_LEN;

/// Caps a thread name at [`MAX_THREAD_NAME_LEN`] bytes without splitting a
/// UTF-8 character — the producer-side capture helper for `StartThread`.
#[must_use]
pub fn capped_name_bytes(name: &str) -> &[u8] {
    if name.len() <= MAX_THREAD_NAME_LEN {
        return name.as_bytes();
    }
    let mut end = MAX_THREAD_NAME_LEN;
    while end > 0 && !name.is_char_boundary(end) {
        end -= 1;
    }
    &name.as_bytes()[..end]
}

use crate::ids::{BexCallId, BexThreadId, FunctionId};

const TAG_CALL_FUNCTION: u8 = 0x01;
const TAG_END_FUNCTION: u8 = 0x02;
const TAG_START_THREAD: u8 = 0x03;
const TAG_END_THREAD: u8 = 0x04;
const TAG_SET_FUNCTION_ID: u8 = 0x05;
const CALL_SITE_FILE_ID_NONE: u32 = u32::MAX;

/// Caller-side source span captured at a profiled call site.
///
/// This is the raw source-map projection from the VM: file id plus byte offsets
/// and the 1-indexed line cached in the bytecode line table. File path and
/// retained source text remain `ProjectStore` concerns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CallSiteSourceSpan {
    pub file_id: u32,
    pub start_offset: u32,
    pub end_offset: u32,
    pub line: u32,
}

/// How a function call ended — a closed taxonomy of the ways a frame can be
/// popped. Statuses describe the frame's fate, not the program's outcome (a
/// frame unwound by an exit that is later caught still ended `Exited`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum FunctionEndStatus {
    /// Normal return.
    Ok = 0,
    /// Unwound by exception propagation.
    Errored = 1,
    /// Closed by cancellation: unwound by a `baml.panics.Cancelled` panic,
    /// or drained open by the engine when its thread was cancelled.
    Cancelled = 2,
    /// Unwound by a `baml.panics.Exit` panic (`baml.sys.exit`). The exit
    /// code is a thread-level fact (see [`ThreadEndStatus`]).
    Exited = 3,
}

/// How a logical BEX thread ended. Minimal by design; extensible.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ThreadEndStatus {
    /// Ran to completion.
    Completed = 0,
    /// Cancelled before completion.
    Cancelled = 1,
    /// Terminated by an error.
    Errored = 2,
}

/// A decoded ring record. Borrowed (`name`) from the drained byte range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RawRecord<'a> {
    /// Tag `0x01`: a function call began (pushed right after the frame push).
    CallFunction {
        /// Reserved flag bits (zero today).
        flags: u8,
        /// Logical BEX thread id (not the OS thread).
        thread_id: BexThreadId,
        /// Per-logical-thread call id (counters start at 1; always
        /// interpret together with `thread_id` — `(thread_id, call_id)` is
        /// the unique key).
        call_id: BexCallId,
        /// Intra-thread caller; `0` = thread-root call.
        parent_call_id: BexCallId,
        /// Per-run function id (see the header's function table; 0 = unattributable).
        function_id: FunctionId,
        /// Caller-side source span for the invocation that produced this call.
        call_site: Option<CallSiteSourceSpan>,
        /// Raw clock ticks ([`crate::prof::clock::now_ticks`]); the consumer
        /// converts to nanoseconds at transcode.
        ts_ticks: u64,
    },
    /// Tag `0x02`: a function call ended (return, unwind, or cancel drain).
    EndFunction {
        /// How the frame ended (see [`FunctionEndStatus`]).
        status: FunctionEndStatus,
        /// Logical BEX thread id.
        thread_id: BexThreadId,
        /// Matches the `CallFunction` it closes.
        call_id: BexCallId,
        /// Raw clock ticks; converted to ns by the consumer at transcode.
        ts_ticks: u64,
    },
    /// Tag `0x03`: a logical thread started (root or spawn).
    StartThread {
        /// Reserved flag bits (zero today).
        flags: u8,
        /// Logical BEX thread id.
        thread_id: BexThreadId,
        /// Spawning thread; `0` = engine-root thread.
        parent_thread_id: BexThreadId,
        /// Spawning call in the parent thread; `0` = none.
        parent_call_id: BexCallId,
        /// Raw clock ticks; converted to ns by the consumer at transcode.
        ts_ticks: u64,
        /// User-defined thread name, ≤ [`MAX_THREAD_NAME_LEN`] bytes of UTF-8
        /// (validated only at transcode time).
        name: &'a [u8],
    },
    /// Tag `0x04`: a logical thread ended.
    EndThread {
        /// How the thread ended.
        status: ThreadEndStatus,
        /// Logical BEX thread id.
        thread_id: BexThreadId,
        /// Raw clock ticks; converted to ns by the consumer at transcode.
        ts_ticks: u64,
    },
    /// Tag `0x05`: `$id` override for a call (reserved for the M1 language
    /// surface; the shape is fixed so the ring format doesn't change).
    SetFunctionId {
        /// Logical BEX thread id.
        thread_id: BexThreadId,
        /// The call whose `$id` is overridden.
        call_id: BexCallId,
        /// The override UUID bytes (`baml.id.new()`).
        id: [u8; 16],
        /// Raw clock ticks; converted to ns by the consumer at transcode.
        ts_ticks: u64,
    },
}

/// Why a byte range failed to decode. In a drained, committed range any of
/// these means producer-side corruption — the consumer treats them as fatal
/// for that ring, never as skippable noise.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecodeError {
    /// First byte is not a known record tag.
    UnknownTag(u8),
    /// The buffer ends mid-record.
    Truncated,
    /// A status byte holds an undefined value.
    InvalidStatus(u8),
    /// A `StartThread` name length exceeds [`MAX_THREAD_NAME_LEN`].
    NameTooLong(u16),
}

impl RawRecord<'_> {
    /// Encoded length of this record in bytes.
    #[must_use]
    pub fn encoded_len(&self) -> usize {
        match self {
            RawRecord::CallFunction { .. } => CALL_FUNCTION_LEN,
            RawRecord::EndFunction { .. } => END_FUNCTION_LEN,
            RawRecord::StartThread { name, .. } => {
                START_THREAD_FIXED_LEN + name.len().min(MAX_THREAD_NAME_LEN)
            }
            RawRecord::EndThread { .. } => END_THREAD_LEN,
            RawRecord::SetFunctionId { .. } => SET_FUNCTION_ID_LEN,
        }
    }

    /// Encodes into the front of `buf`, returning the encoded length.
    ///
    /// `StartThread` names are truncated to [`MAX_THREAD_NAME_LEN`] bytes
    /// defensively; callers are expected to cap (UTF-8-safely) at capture
    /// time.
    #[inline]
    pub fn encode(&self, buf: &mut [u8; MAX_RECORD_LEN]) -> usize {
        self.encode_to(buf)
    }

    /// Encodes into the front of `buf`, returning the encoded length, for
    /// producers that write straight into a ring slot sized to the record
    /// ([`Self::encoded_len`]) instead of zeroing [`MAX_RECORD_LEN`] bytes on a
    /// hot path.
    ///
    /// # Safety / precondition
    /// `buf.len()` MUST be at least [`Self::encoded_len`]. The fixed integer
    /// fields are written via unchecked unaligned stores (debug-asserted, not
    /// release-checked), so an undersized `buf` is undefined behavior in
    /// release builds — not a panic. Every caller satisfies this:
    /// [`Self::encode`] uses a [`MAX_RECORD_LEN`] buffer and `Ring::push_with`
    /// reserves exactly `encoded_len()`.
    #[inline]
    pub fn encode_to(&self, buf: &mut [u8]) -> usize {
        let mut w = Writer { buf, pos: 0 };
        match *self {
            RawRecord::CallFunction {
                flags,
                thread_id,
                call_id,
                parent_call_id,
                function_id,
                call_site,
                ts_ticks,
            } => {
                w.u8(TAG_CALL_FUNCTION);
                w.u8(flags);
                w.u64(thread_id.0);
                w.u64(call_id.0);
                w.u64(parent_call_id.0);
                w.u32(function_id.0);
                w.u64(ts_ticks);
                if let Some(call_site) = call_site {
                    w.u32(call_site.file_id);
                    w.u32(call_site.start_offset);
                    w.u32(call_site.end_offset);
                    w.u32(call_site.line);
                } else {
                    w.u32(CALL_SITE_FILE_ID_NONE);
                    w.u32(0);
                    w.u32(0);
                    w.u32(0);
                }
            }
            RawRecord::EndFunction {
                status,
                thread_id,
                call_id,
                ts_ticks,
            } => {
                w.u8(TAG_END_FUNCTION);
                w.u8(status as u8);
                w.u64(thread_id.0);
                w.u64(call_id.0);
                w.u64(ts_ticks);
            }
            RawRecord::StartThread {
                flags,
                thread_id,
                parent_thread_id,
                parent_call_id,
                ts_ticks,
                name,
            } => {
                let name = &name[..name.len().min(MAX_THREAD_NAME_LEN)];
                w.u8(TAG_START_THREAD);
                w.u8(flags);
                w.u64(thread_id.0);
                w.u64(parent_thread_id.0);
                w.u64(parent_call_id.0);
                w.u64(ts_ticks);
                #[expect(
                    clippy::cast_possible_truncation,
                    reason = "name.len() <= MAX_THREAD_NAME_LEN = 256"
                )]
                w.u16(name.len() as u16);
                w.bytes(name);
            }
            RawRecord::EndThread {
                status,
                thread_id,
                ts_ticks,
            } => {
                w.u8(TAG_END_THREAD);
                w.u8(status as u8);
                w.u64(thread_id.0);
                w.u64(ts_ticks);
            }
            RawRecord::SetFunctionId {
                thread_id,
                call_id,
                id,
                ts_ticks,
            } => {
                w.u8(TAG_SET_FUNCTION_ID);
                w.u64(thread_id.0);
                w.u64(call_id.0);
                w.bytes(&id);
                w.u64(ts_ticks);
            }
        }
        debug_assert_eq!(w.pos, self.encoded_len());
        w.pos
    }
}

/// Decodes the record at the front of `buf`, returning it and its encoded
/// length. Never panics on malformed input.
pub fn decode(buf: &[u8]) -> Result<(RawRecord<'_>, usize), DecodeError> {
    let mut r = Reader { buf, pos: 0 };
    let tag = r.u8()?;
    let rec = match tag {
        TAG_CALL_FUNCTION => {
            let flags = r.u8()?;
            let thread_id = BexThreadId(r.u64()?);
            let call_id = BexCallId(r.u64()?);
            let parent_call_id = BexCallId(r.u64()?);
            let function_id = FunctionId(r.u32()?);
            let ts_ticks = r.u64()?;
            let call_site_file_id = r.u32()?;
            let call_site_start_offset = r.u32()?;
            let call_site_end_offset = r.u32()?;
            let call_site_line = r.u32()?;
            RawRecord::CallFunction {
                flags,
                thread_id,
                call_id,
                parent_call_id,
                function_id,
                call_site: (call_site_file_id != CALL_SITE_FILE_ID_NONE).then_some(
                    CallSiteSourceSpan {
                        file_id: call_site_file_id,
                        start_offset: call_site_start_offset,
                        end_offset: call_site_end_offset,
                        line: call_site_line,
                    },
                ),
                ts_ticks,
            }
        }
        TAG_END_FUNCTION => RawRecord::EndFunction {
            status: match r.u8()? {
                0 => FunctionEndStatus::Ok,
                1 => FunctionEndStatus::Errored,
                2 => FunctionEndStatus::Cancelled,
                3 => FunctionEndStatus::Exited,
                bad => return Err(DecodeError::InvalidStatus(bad)),
            },
            thread_id: BexThreadId(r.u64()?),
            call_id: BexCallId(r.u64()?),
            ts_ticks: r.u64()?,
        },
        TAG_START_THREAD => {
            let flags = r.u8()?;
            let thread_id = BexThreadId(r.u64()?);
            let parent_thread_id = BexThreadId(r.u64()?);
            let parent_call_id = BexCallId(r.u64()?);
            let ts_ticks = r.u64()?;
            let name_len = r.u16()?;
            if usize::from(name_len) > MAX_THREAD_NAME_LEN {
                return Err(DecodeError::NameTooLong(name_len));
            }
            RawRecord::StartThread {
                flags,
                thread_id,
                parent_thread_id,
                parent_call_id,
                ts_ticks,
                name: r.bytes(usize::from(name_len))?,
            }
        }
        TAG_END_THREAD => RawRecord::EndThread {
            status: match r.u8()? {
                0 => ThreadEndStatus::Completed,
                1 => ThreadEndStatus::Cancelled,
                2 => ThreadEndStatus::Errored,
                bad => return Err(DecodeError::InvalidStatus(bad)),
            },
            thread_id: BexThreadId(r.u64()?),
            ts_ticks: r.u64()?,
        },
        TAG_SET_FUNCTION_ID => RawRecord::SetFunctionId {
            thread_id: BexThreadId(r.u64()?),
            call_id: BexCallId(r.u64()?),
            id: {
                let bytes = r.bytes(16)?;
                let mut id = [0u8; 16];
                id.copy_from_slice(bytes);
                id
            },
            ts_ticks: r.u64()?,
        },
        bad => return Err(DecodeError::UnknownTag(bad)),
    };
    Ok((rec, r.pos))
}

/// Iterates the records in a drained byte range.
pub fn iter(buf: &[u8]) -> RecordIter<'_> {
    RecordIter { buf }
}

/// See [`iter`]. Yields `Err` once and then stops.
pub struct RecordIter<'a> {
    buf: &'a [u8],
}

impl<'a> Iterator for RecordIter<'a> {
    type Item = Result<RawRecord<'a>, DecodeError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.buf.is_empty() {
            return None;
        }
        match decode(self.buf) {
            Ok((rec, len)) => {
                self.buf = &self.buf[len..];
                Some(Ok(rec))
            }
            Err(err) => {
                self.buf = &[];
                Some(Err(err))
            }
        }
    }
}

struct Writer<'b> {
    buf: &'b mut [u8],
    pos: usize,
}

impl Writer<'_> {
    #[inline]
    fn u8(&mut self, v: u8) {
        self.buf[self.pos] = v;
        self.pos += 1;
    }

    #[inline]
    fn u16(&mut self, v: u16) {
        debug_assert!(self.pos + 2 <= self.buf.len());
        // SAFETY: callers size `buf` >= `encoded_len()` (producer reserves
        // exactly that via `Ring::push_with`), so `pos + 2` is in bounds.
        #[expect(
            unsafe_code,
            reason = "in-bounds unaligned store; avoids non-inlined copy"
        )]
        unsafe {
            self.buf
                .as_mut_ptr()
                .add(self.pos)
                .cast::<u16>()
                .write_unaligned(v.to_le());
        }
        self.pos += 2;
    }

    #[inline]
    fn u32(&mut self, v: u32) {
        debug_assert!(self.pos + 4 <= self.buf.len());
        // SAFETY: see `u16`.
        #[expect(
            unsafe_code,
            reason = "in-bounds unaligned store; avoids non-inlined copy"
        )]
        unsafe {
            self.buf
                .as_mut_ptr()
                .add(self.pos)
                .cast::<u32>()
                .write_unaligned(v.to_le());
        }
        self.pos += 4;
    }

    #[inline]
    fn u64(&mut self, v: u64) {
        debug_assert!(self.pos + 8 <= self.buf.len());
        // SAFETY: see `u16`. A direct unaligned store lowers to one `mov` at
        // `opt-level="s"`; routing through `copy_from_slice`/`copy_nonoverlapping`
        // emitted a non-inlined call there (~5-8 ns/pair on the hot records).
        #[expect(
            unsafe_code,
            reason = "in-bounds unaligned store; avoids non-inlined copy"
        )]
        unsafe {
            self.buf
                .as_mut_ptr()
                .add(self.pos)
                .cast::<u64>()
                .write_unaligned(v.to_le());
        }
        self.pos += 8;
    }

    #[inline]
    fn bytes(&mut self, b: &[u8]) {
        self.buf[self.pos..self.pos + b.len()].copy_from_slice(b);
        self.pos += b.len();
    }
}

struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn u8(&mut self) -> Result<u8, DecodeError> {
        let v = *self.buf.get(self.pos).ok_or(DecodeError::Truncated)?;
        self.pos += 1;
        Ok(v)
    }

    fn u16(&mut self) -> Result<u16, DecodeError> {
        Ok(u16::from_le_bytes(self.array()?))
    }

    fn u32(&mut self) -> Result<u32, DecodeError> {
        Ok(u32::from_le_bytes(self.array()?))
    }

    fn u64(&mut self) -> Result<u64, DecodeError> {
        Ok(u64::from_le_bytes(self.array()?))
    }

    fn bytes(&mut self, len: usize) -> Result<&'a [u8], DecodeError> {
        let end = self.pos.checked_add(len).ok_or(DecodeError::Truncated)?;
        let b = self.buf.get(self.pos..end).ok_or(DecodeError::Truncated)?;
        self.pos = end;
        Ok(b)
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N], DecodeError> {
        let mut out = [0u8; N];
        out.copy_from_slice(self.bytes(N)?);
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip(rec: RawRecord<'_>) {
        let mut buf = [0u8; MAX_RECORD_LEN];
        let len = rec.encode(&mut buf);
        assert_eq!(len, rec.encoded_len());
        let (decoded, consumed) = decode(&buf[..len]).expect("roundtrip decode");
        assert_eq!(consumed, len);
        assert_eq!(decoded, rec);
        // Decoding must not read past the record even with trailing bytes.
        let (decoded, consumed) = decode(&buf).expect("decode with trailing bytes");
        assert_eq!(consumed, len);
        assert_eq!(decoded, rec);
    }

    #[test]
    #[expect(
        clippy::cast_possible_truncation,
        reason = "intentionally truncating casts to vary the narrow test fields"
    )]
    fn roundtrip_all_variants() {
        for v in [0u64, 1, 42, u64::MAX] {
            roundtrip(RawRecord::CallFunction {
                flags: v as u8,
                thread_id: BexThreadId(v),
                call_id: BexCallId(v.wrapping_add(1)),
                parent_call_id: BexCallId(v / 2),
                function_id: FunctionId(v as u32),
                call_site: None,
                ts_ticks: v,
            });
            roundtrip(RawRecord::EndFunction {
                status: FunctionEndStatus::Ok,
                thread_id: BexThreadId(v),
                call_id: BexCallId(v),
                ts_ticks: v,
            });
            roundtrip(RawRecord::EndThread {
                status: ThreadEndStatus::Cancelled,
                thread_id: BexThreadId(v),
                ts_ticks: v,
            });
            roundtrip(RawRecord::SetFunctionId {
                thread_id: BexThreadId(v),
                call_id: BexCallId(v),
                id: [v as u8; 16],
                ts_ticks: v,
            });
        }
        for status in [
            FunctionEndStatus::Errored,
            FunctionEndStatus::Cancelled,
            FunctionEndStatus::Exited,
        ] {
            roundtrip(RawRecord::EndFunction {
                status,
                thread_id: BexThreadId(7),
                call_id: BexCallId(9),
                ts_ticks: 11,
            });
        }
        roundtrip(RawRecord::EndThread {
            status: ThreadEndStatus::Completed,
            thread_id: BexThreadId(7),
            ts_ticks: 11,
        });
        roundtrip(RawRecord::EndThread {
            status: ThreadEndStatus::Errored,
            thread_id: BexThreadId(7),
            ts_ticks: 11,
        });
        roundtrip(RawRecord::CallFunction {
            flags: 0,
            thread_id: BexThreadId(1),
            call_id: BexCallId(2),
            parent_call_id: BexCallId(1),
            function_id: FunctionId(3),
            call_site: Some(CallSiteSourceSpan {
                file_id: 0,
                start_offset: 12,
                end_offset: 18,
                line: 4,
            }),
            ts_ticks: 99,
        });
    }

    #[test]
    fn roundtrip_thread_names() {
        for len in [0usize, 1, 100, 255, 256] {
            let name = vec![b'x'; len];
            roundtrip(RawRecord::StartThread {
                flags: 1,
                thread_id: BexThreadId(2),
                parent_thread_id: BexThreadId(0),
                parent_call_id: BexCallId(3),
                ts_ticks: 4,
                name: &name,
            });
        }
    }

    #[test]
    fn encode_truncates_oversized_name() {
        let name = vec![b'y'; 300];
        let rec = RawRecord::StartThread {
            flags: 0,
            thread_id: BexThreadId(1),
            parent_thread_id: BexThreadId(0),
            parent_call_id: BexCallId(0),
            ts_ticks: 0,
            name: &name,
        };
        let mut buf = [0u8; MAX_RECORD_LEN];
        let len = rec.encode(&mut buf);
        assert_eq!(len, START_THREAD_FIXED_LEN + MAX_THREAD_NAME_LEN);
        let (decoded, _) = decode(&buf[..len]).expect("decode");
        match decoded {
            RawRecord::StartThread { name, .. } => assert_eq!(name.len(), MAX_THREAD_NAME_LEN),
            other => panic!("wrong variant: {other:?}"),
        }
    }

    #[test]
    fn fixed_sizes_match_spec() {
        let mut buf = [0u8; MAX_RECORD_LEN];
        let call = RawRecord::CallFunction {
            flags: 0,
            thread_id: BexThreadId(1),
            call_id: BexCallId(1),
            parent_call_id: BexCallId(0),
            function_id: FunctionId(0),
            call_site: None,
            ts_ticks: 0,
        };
        assert_eq!(call.encode(&mut buf), 54);
        let end = RawRecord::EndFunction {
            status: FunctionEndStatus::Ok,
            thread_id: BexThreadId(1),
            call_id: BexCallId(1),
            ts_ticks: 0,
        };
        assert_eq!(end.encode(&mut buf), 26);
        let start_thread = RawRecord::StartThread {
            flags: 0,
            thread_id: BexThreadId(1),
            parent_thread_id: BexThreadId(0),
            parent_call_id: BexCallId(0),
            ts_ticks: 0,
            name: b"",
        };
        assert_eq!(start_thread.encode(&mut buf), 36);
        let end_thread = RawRecord::EndThread {
            status: ThreadEndStatus::Completed,
            thread_id: BexThreadId(1),
            ts_ticks: 0,
        };
        assert_eq!(end_thread.encode(&mut buf), 18);
        let set_id = RawRecord::SetFunctionId {
            thread_id: BexThreadId(1),
            call_id: BexCallId(1),
            id: [0; 16],
            ts_ticks: 0,
        };
        assert_eq!(set_id.encode(&mut buf), 41);
    }

    #[test]
    fn decode_rejects_unknown_tag_and_bad_status() {
        assert_eq!(decode(&[0x00]), Err(DecodeError::UnknownTag(0x00)));
        assert_eq!(decode(&[0x06]), Err(DecodeError::UnknownTag(0x06)));
        assert_eq!(decode(&[]), Err(DecodeError::Truncated));

        let mut buf = [0u8; MAX_RECORD_LEN];
        let len = RawRecord::EndFunction {
            status: FunctionEndStatus::Ok,
            thread_id: BexThreadId(1),
            call_id: BexCallId(1),
            ts_ticks: 0,
        }
        .encode(&mut buf);
        buf[1] = 4; // first byte past the status range {Ok,Errored,Cancelled,Exited}
        assert_eq!(decode(&buf[..len]), Err(DecodeError::InvalidStatus(4)));
    }

    #[test]
    fn decode_rejects_oversized_name_len() {
        let mut buf = [0u8; MAX_RECORD_LEN];
        let len = RawRecord::StartThread {
            flags: 0,
            thread_id: BexThreadId(1),
            parent_thread_id: BexThreadId(0),
            parent_call_id: BexCallId(0),
            ts_ticks: 0,
            name: b"hi",
        }
        .encode(&mut buf);
        // Corrupt name_len to 257 (little-endian u16 at offset 34).
        buf[START_THREAD_FIXED_LEN - 2..START_THREAD_FIXED_LEN]
            .copy_from_slice(&257u16.to_le_bytes());
        assert_eq!(decode(&buf[..len]), Err(DecodeError::NameTooLong(257)));
    }

    #[test]
    fn every_truncation_errors_without_panicking() {
        let name = vec![b'n'; 17];
        let records = [
            RawRecord::CallFunction {
                flags: 1,
                thread_id: BexThreadId(2),
                call_id: BexCallId(3),
                parent_call_id: BexCallId(4),
                function_id: FunctionId(5),
                call_site: Some(CallSiteSourceSpan {
                    file_id: 6,
                    start_offset: 7,
                    end_offset: 8,
                    line: 9,
                }),
                ts_ticks: 6,
            },
            RawRecord::StartThread {
                flags: 0,
                thread_id: BexThreadId(1),
                parent_thread_id: BexThreadId(2),
                parent_call_id: BexCallId(3),
                ts_ticks: 4,
                name: &name,
            },
            RawRecord::SetFunctionId {
                thread_id: BexThreadId(1),
                call_id: BexCallId(2),
                id: [3; 16],
                ts_ticks: 4,
            },
        ];
        for rec in records {
            let mut buf = [0u8; MAX_RECORD_LEN];
            let len = rec.encode(&mut buf);
            for cut in 0..len {
                assert_eq!(
                    decode(&buf[..cut]),
                    Err(DecodeError::Truncated),
                    "cut at {cut} of {len}"
                );
            }
        }
    }

    #[test]
    fn iterates_packed_records() {
        let name = b"worker".as_slice();
        let records = vec![
            RawRecord::StartThread {
                flags: 0,
                thread_id: BexThreadId(1),
                parent_thread_id: BexThreadId(0),
                parent_call_id: BexCallId(0),
                ts_ticks: 1,
                name,
            },
            RawRecord::CallFunction {
                flags: 0,
                thread_id: BexThreadId(1),
                call_id: BexCallId(1),
                parent_call_id: BexCallId(0),
                function_id: FunctionId(7),
                call_site: None,
                ts_ticks: 2,
            },
            RawRecord::EndFunction {
                status: FunctionEndStatus::Ok,
                thread_id: BexThreadId(1),
                call_id: BexCallId(1),
                ts_ticks: 3,
            },
            RawRecord::EndThread {
                status: ThreadEndStatus::Completed,
                thread_id: BexThreadId(1),
                ts_ticks: 4,
            },
        ];
        let mut packed = Vec::new();
        for rec in &records {
            let mut buf = [0u8; MAX_RECORD_LEN];
            let len = rec.encode(&mut buf);
            packed.extend_from_slice(&buf[..len]);
        }
        let decoded: Vec<_> = iter(&packed).map(|r| r.expect("decode")).collect();
        assert_eq!(decoded, records);

        // A corrupt tail yields exactly one error then stops.
        packed.push(0xFF);
        let results: Vec<_> = iter(&packed).collect();
        assert_eq!(results.len(), records.len() + 1);
        assert_eq!(*results.last().unwrap(), Err(DecodeError::UnknownTag(0xFF)));
    }

    #[test]
    #[expect(clippy::cast_possible_truncation, reason = "PRNG byte extraction")]
    fn decode_arbitrary_prefixes_never_panic() {
        // Cheap deterministic fuzz: pseudo-random bytes, every tag value in
        // the first position.
        let mut state = 0x9E37_79B9_7F4A_7C15u64;
        let mut bytes = Vec::with_capacity(MAX_RECORD_LEN * 2);
        for _ in 0..MAX_RECORD_LEN * 2 {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1);
            bytes.push((state >> 33) as u8);
        }
        for first in 0..=u8::MAX {
            bytes[0] = first;
            for end in 0..bytes.len() {
                let _ = decode(&bytes[..end]);
            }
        }
    }
}
