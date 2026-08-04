//! §7.4 canonical value encoding + CID (C9 contract — FROZEN once the
//! golden fixtures land).
//!
//! CID = BLAKE3-256, domain-prefixed and version-bound:
//! `blake3("baml-value-node-v1\0" || canonical_node_bytes)` for DAG nodes,
//! `blake3("baml-value-chunk-v1\0" || raw_chunk_bytes)` for string/bytes
//! chunks. Canonical node bytes are **schema-erased structural
//! encodings**: class and enum identity is carried as the `definition_key`
//! string (`class:pkg.Ns.Name` / `enum:pkg.Ns.Name`, the MIR dotted form)
//! and enum variants by name — renames change CIDs by design (§4.4).
//!
//! Determinism rules (the whole point):
//! - map entries sort by key UTF-8 bytes; duplicate keys keep the LAST
//!   occurrence (matching IndexMap insert-overwrite semantics);
//! - floats: NaN normalizes to the single quiet bit pattern
//!   0x7FF8_0000_0000_0000; ±0.0 and ±inf are preserved (distinct values);
//! - bigints: minimal ASCII decimal (no `+`, no leading zeros, `-` only on
//!   negatives, `0` for zero);
//! - class fields carry a presence byte: 0 absent, 1 null, 2 value,
//!   3 default-filled (today's capture emits 1 and 2; 0/3 are reserved for
//!   higher-fidelity capture and MUST NOT be re-purposed);
//! - strings/bytes chunk at a fixed 128 KiB; lists/maps hold at most 128
//!   entries per node (longer ones build a 128-ary tree of segment nodes);
//! - a child inlines into its parent iff its canonical encoding is
//!   ≤ 128 bytes AND it references no other nodes; otherwise it is a
//!   32-byte CID reference. (Keeps hot leaves overhead-free while every
//!   node stays under the §7.4 2 KiB target.)
//!
//! Node byte layout: `tag u8` then payload; varints are avoided — all
//! integers little-endian fixed width; strings length-prefixed u32.

use rustc_hash::FxHashMap;

pub const NODE_CODEC_VERSION: u32 = 1;
pub const NODE_DOMAIN: &[u8] = b"baml-value-node-v1\0";
pub const CHUNK_DOMAIN: &[u8] = b"baml-value-chunk-v1\0";
/// Fixed chunk size for long strings/bytes (§7.4).
pub const CHUNK_BYTES: usize = 128 * 1024;
/// Max entries per list/map node (§7.4).
pub const NODE_FANOUT: usize = 128;
/// A child inlines iff its encoding is at most this long and leaf-only.
pub const INLINE_CHILD_MAX: usize = 128;
/// Map keys longer than this encode indirectly (a string-node ref marked
/// by the 0xFFFF_FFFF length sentinel) so entry frames stay bounded.
pub const MAX_DIRECT_KEY: usize = 4096;

pub mod tag {
    pub const NULL: u8 = 0;
    pub const BOOL: u8 = 1;
    pub const INT: u8 = 2;
    pub const FLOAT: u8 = 3;
    pub const BIGINT: u8 = 4;
    pub const STRING: u8 = 5;
    pub const BYTES: u8 = 6;
    pub const LIST: u8 = 7;
    pub const MAP: u8 = 8;
    pub const CLASS: u8 = 9;
    pub const ENUM: u8 = 10;
    pub const MEDIA: u8 = 11;
    pub const OMITTED: u8 = 12;
    /// Internal 128-ary segment node for long lists/maps.
    pub const SEGMENT: u8 = 13;
    /// Long string/bytes body: logical_len + chunk CIDs.
    pub const CHUNKED: u8 = 14;
}

/// Field presence (§7.4: absent, null, and default-filled are distinct).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Presence {
    Absent = 0,
    Null = 1,
    Value = 2,
    DefaultFilled = 3,
}

/// The canonical value model — the encoder's input. Hosts (the engine's
/// TraceValue, SDK captures) convert into this; the C9 fixtures freeze the
/// bytes this produces.
#[derive(Debug, Clone, PartialEq)]
pub enum CanonValue {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    /// Any-precision integer as a decimal string (canonicalized on encode).
    Bigint(String),
    String(String),
    Bytes(Vec<u8>),
    List(Vec<CanonValue>),
    /// Key-value entries; encoder sorts by key bytes (last wins on dup).
    Map(Vec<(String, CanonValue)>),
    Class {
        /// `class:<dotted>` (§4.4 definition key convention).
        definition_key: String,
        /// Declared order is NOT preserved — fields sort by name bytes.
        fields: Vec<(String, Presence, Option<CanonValue>)>,
    },
    Enum {
        definition_key: String,
        variant: String,
    },
    Media {
        /// e.g. `image`, `audio`, `pdf` — the capture-side kind string.
        kind: String,
        mime: Option<String>,
        /// 0 url, 1 base64(decoded upstream or kept textual), 2 file.
        content_kind: u8,
        content: String,
    },
    Omitted {
        reason: u8,
        message: String,
    },
}

/// One encoded DAG: the root CID plus every node/chunk body this value
/// produced (dedup happens at the store layer — the encoder just emits).
#[derive(Debug, Default)]
pub struct CanonEncoded {
    pub root_cid: [u8; 32],
    /// (cid, canonical node bytes), children before parents.
    pub nodes: Vec<([u8; 32], Vec<u8>)>,
    /// (cid, raw chunk bytes) for long strings/bytes.
    pub chunks: Vec<([u8; 32], Vec<u8>)>,
    /// Logical length of the root (bytes of the value's own encoding
    /// summed over the tree — the `DagRef.logical_len` field).
    pub logical_len: u64,
}

#[must_use]
pub fn cid_for_node(bytes: &[u8]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(NODE_DOMAIN);
    hasher.update(bytes);
    *hasher.finalize().as_bytes()
}

#[must_use]
pub fn cid_for_chunk(bytes: &[u8]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(CHUNK_DOMAIN);
    hasher.update(bytes);
    *hasher.finalize().as_bytes()
}

/// Public CID wire form: `bamlv_1_<base64url-unpadded>` (TASK/2 ruling).
#[must_use]
pub fn cid_wire(cid: &[u8; 32]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::with_capacity(8 + 43);
    out.push_str("bamlv_1_");
    for chunk in cid.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = u32::from(b[0]) << 16 | u32::from(b[1]) << 8 | u32::from(b[2]);
        out.push(ALPHABET[(n >> 18) as usize & 63] as char);
        out.push(ALPHABET[(n >> 12) as usize & 63] as char);
        if chunk.len() > 1 {
            out.push(ALPHABET[(n >> 6) as usize & 63] as char);
        }
        if chunk.len() > 2 {
            out.push(ALPHABET[n as usize & 63] as char);
        }
    }
    out
}

/// Encode one value into its canonical DAG.
#[must_use]
pub fn encode(value: &CanonValue) -> CanonEncoded {
    let mut out = CanonEncoded::default();
    let mut seen: FxHashMap<[u8; 32], ()> = FxHashMap::default();
    let root = encode_node(value, &mut out, &mut seen);
    match root {
        Encoded::Inline(bytes) => {
            // Roots are always materialized as nodes (a DagRef needs a CID).
            let cid = cid_for_node(&bytes);
            if seen.insert(cid, ()).is_none() {
                out.nodes.push((cid, bytes));
            }
            out.root_cid = cid;
        }
        Encoded::Ref(cid) => out.root_cid = cid,
    }
    out
}

enum Encoded {
    /// Canonical bytes small enough (and leaf-only) to inline in a parent.
    Inline(Vec<u8>),
    Ref([u8; 32]),
}

fn put_str(out: &mut Vec<u8>, s: &str) {
    out.extend_from_slice(&u32::try_from(s.len()).unwrap_or(u32::MAX).to_le_bytes());
    out.extend_from_slice(s.as_bytes());
}

/// Write one child slot: 0 = inline (u16 len + bytes), 1 = ref (32 B cid).
fn put_child(out: &mut Vec<u8>, child: &Encoded) {
    match child {
        Encoded::Inline(bytes) => {
            out.push(0);
            out.extend_from_slice(&u16::try_from(bytes.len()).unwrap_or(u16::MAX).to_le_bytes());
            out.extend_from_slice(bytes);
        }
        Encoded::Ref(cid) => {
            out.push(1);
            out.extend_from_slice(cid);
        }
    }
}

/// Materialize an encoding as a node, dedup-aware.
fn materialize(
    bytes: Vec<u8>,
    out: &mut CanonEncoded,
    seen: &mut FxHashMap<[u8; 32], ()>,
) -> Encoded {
    let cid = cid_for_node(&bytes);
    if seen.insert(cid, ()).is_none() {
        out.nodes.push((cid, bytes));
    }
    Encoded::Ref(cid)
}

/// Inline iff small and leaf-only, else materialize.
fn finish(
    bytes: Vec<u8>,
    leaf_only: bool,
    out: &mut CanonEncoded,
    seen: &mut FxHashMap<[u8; 32], ()>,
) -> Encoded {
    if leaf_only && bytes.len() <= INLINE_CHILD_MAX {
        Encoded::Inline(bytes)
    } else {
        materialize(bytes, out, seen)
    }
}

fn canonical_bigint(s: &str) -> String {
    let (neg, digits) = match s.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, s.strip_prefix('+').unwrap_or(s)),
    };
    let trimmed = digits.trim_start_matches('0');
    if trimmed.is_empty() || !trimmed.bytes().all(|b| b.is_ascii_digit()) {
        if digits.bytes().all(|b| b == b'0') && !digits.is_empty() {
            return "0".to_string();
        }
        // Not a decimal integer: keep verbatim (never silently alias).
        return s.to_string();
    }
    if neg {
        format!("-{trimmed}")
    } else {
        trimmed.to_string()
    }
}

/// Long string/bytes body → CHUNKED node (or the raw inline body).
fn encode_body(
    tag_byte: u8,
    body: &[u8],
    out: &mut CanonEncoded,
    seen: &mut FxHashMap<[u8; 32], ()>,
) -> Encoded {
    out.logical_len += body.len() as u64;
    if body.len() <= CHUNK_BYTES {
        let mut bytes = Vec::with_capacity(5 + body.len());
        bytes.push(tag_byte);
        bytes.extend_from_slice(&u32::try_from(body.len()).unwrap_or(u32::MAX).to_le_bytes());
        bytes.extend_from_slice(body);
        return finish(bytes, true, out, seen);
    }
    // Fixed 128 KiB chunking; the CHUNKED node references chunk CIDs.
    let mut bytes = Vec::new();
    bytes.push(tag::CHUNKED);
    bytes.push(tag_byte); // the logical kind (STRING or BYTES)
    bytes.extend_from_slice(&(body.len() as u64).to_le_bytes());
    let chunk_count = body.len().div_ceil(CHUNK_BYTES);
    bytes.extend_from_slice(&u32::try_from(chunk_count).unwrap_or(u32::MAX).to_le_bytes());
    for chunk in body.chunks(CHUNK_BYTES) {
        let cid = cid_for_chunk(chunk);
        if seen.insert(cid, ()).is_none() {
            out.chunks.push((cid, chunk.to_vec()));
        }
        bytes.extend_from_slice(&cid);
    }
    materialize(bytes, out, seen)
}

/// Encode `children` (already-encoded slots) into ≤128-ary nodes; returns
/// the sequence to embed in the parent (len ≤ 128 after segmentation).
fn segment_children(
    header_tag: u8,
    children: Vec<Encoded>,
    out: &mut CanonEncoded,
    seen: &mut FxHashMap<[u8; 32], ()>,
) -> Vec<Encoded> {
    let mut level = children;
    while level.len() > NODE_FANOUT {
        let mut next = Vec::with_capacity(level.len().div_ceil(NODE_FANOUT));
        for group in level.chunks(NODE_FANOUT) {
            let mut bytes = Vec::new();
            bytes.push(tag::SEGMENT);
            bytes.push(header_tag);
            bytes.extend_from_slice(&u32::try_from(group.len()).unwrap_or(u32::MAX).to_le_bytes());
            for child in group {
                put_child(&mut bytes, child);
            }
            next.push(materialize(bytes, out, seen));
        }
        level = next;
    }
    level
}

fn encode_node(
    value: &CanonValue,
    out: &mut CanonEncoded,
    seen: &mut FxHashMap<[u8; 32], ()>,
) -> Encoded {
    match value {
        CanonValue::Null => {
            out.logical_len += 1;
            Encoded::Inline(vec![tag::NULL])
        }
        CanonValue::Bool(b) => {
            out.logical_len += 2;
            Encoded::Inline(vec![tag::BOOL, u8::from(*b)])
        }
        CanonValue::Int(i) => {
            out.logical_len += 9;
            let mut bytes = vec![tag::INT];
            bytes.extend_from_slice(&i.to_le_bytes());
            Encoded::Inline(bytes)
        }
        CanonValue::Float(f) => {
            out.logical_len += 9;
            let normalized = if f.is_nan() {
                f64::from_bits(0x7FF8_0000_0000_0000)
            } else {
                *f
            };
            let mut bytes = vec![tag::FLOAT];
            bytes.extend_from_slice(&normalized.to_bits().to_le_bytes());
            Encoded::Inline(bytes)
        }
        CanonValue::Bigint(s) => {
            let canonical = canonical_bigint(s);
            out.logical_len += canonical.len() as u64;
            let mut bytes = vec![tag::BIGINT];
            put_str(&mut bytes, &canonical);
            finish(bytes, true, out, seen)
        }
        CanonValue::String(s) => encode_body(tag::STRING, s.as_bytes(), out, seen),
        CanonValue::Bytes(b) => encode_body(tag::BYTES, b, out, seen),
        CanonValue::List(items) => {
            let encoded: Vec<Encoded> = items.iter().map(|v| encode_node(v, out, seen)).collect();
            let top = segment_children(tag::LIST, encoded, out, seen);
            let mut bytes = Vec::new();
            bytes.push(tag::LIST);
            bytes.extend_from_slice(&u64::try_from(items.len()).unwrap_or(u64::MAX).to_le_bytes());
            bytes.extend_from_slice(&u32::try_from(top.len()).unwrap_or(u32::MAX).to_le_bytes());
            for child in &top {
                put_child(&mut bytes, child);
            }
            finish(bytes, items.is_empty(), out, seen)
        }
        CanonValue::Map(entries) => {
            // Sort by key bytes; duplicates keep the LAST occurrence.
            let mut sorted: Vec<(usize, &(String, CanonValue))> =
                entries.iter().enumerate().collect();
            sorted.sort_by(|a, b| a.1.0.as_bytes().cmp(b.1.0.as_bytes()).then(a.0.cmp(&b.0)));
            sorted.dedup_by(|later, earlier| {
                if later.1.0 == earlier.1.0 {
                    // keep `later` (higher original index): overwrite.
                    earlier.1 = later.1;
                    earlier.0 = later.0;
                    true
                } else {
                    false
                }
            });
            let encoded: Vec<Encoded> = sorted
                .iter()
                .map(|(_, (key, value))| {
                    let child = encode_node(value, out, seen);
                    let mut bytes = Vec::new();
                    out.logical_len += key.len() as u64;
                    if key.len() > MAX_DIRECT_KEY {
                        // Indirect key: sentinel length + string-node ref
                        // (bounds every entry frame under the u16 limit).
                        bytes.extend_from_slice(&u32::MAX.to_le_bytes());
                        let key_node = encode_body(tag::STRING, key.as_bytes(), out, seen);
                        let key_node = match key_node {
                            Encoded::Inline(inner) => materialize(inner, out, seen),
                            r @ Encoded::Ref(_) => r,
                        };
                        put_child(&mut bytes, &key_node);
                    } else {
                        put_str(&mut bytes, key);
                    }
                    // The (key, child-slot) pair is the entry encoding.
                    put_child(&mut bytes, &child);
                    Encoded::Inline(bytes)
                })
                .collect();
            // Entries are inline pair-encodings; segment on count only.
            let top = segment_children(tag::MAP, encoded, out, seen);
            let mut bytes = Vec::new();
            bytes.push(tag::MAP);
            bytes.extend_from_slice(
                &u64::try_from(sorted.len())
                    .unwrap_or(u64::MAX)
                    .to_le_bytes(),
            );
            bytes.extend_from_slice(&u32::try_from(top.len()).unwrap_or(u32::MAX).to_le_bytes());
            for child in &top {
                put_child(&mut bytes, child);
            }
            finish(bytes, sorted.is_empty(), out, seen)
        }
        CanonValue::Class {
            definition_key,
            fields,
        } => {
            let mut sorted: Vec<&(String, Presence, Option<CanonValue>)> = fields.iter().collect();
            sorted.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
            let mut bytes = Vec::new();
            bytes.push(tag::CLASS);
            put_str(&mut bytes, definition_key);
            out.logical_len += definition_key.len() as u64;
            bytes.extend_from_slice(
                &u32::try_from(sorted.len())
                    .unwrap_or(u32::MAX)
                    .to_le_bytes(),
            );
            for (name, presence, value) in sorted {
                debug_assert_eq!(
                    matches!(presence, Presence::Value | Presence::DefaultFilled),
                    value.is_some(),
                    "slot present iff presence is Value/DefaultFilled (decode contract)"
                );
                put_str(&mut bytes, name);
                out.logical_len += name.len() as u64;
                bytes.push(*presence as u8);
                if let Some(value) = value {
                    let child = encode_node(value, out, seen);
                    put_child(&mut bytes, &child);
                }
            }
            materialize(bytes, out, seen)
        }
        CanonValue::Enum {
            definition_key,
            variant,
        } => {
            out.logical_len += (definition_key.len() + variant.len()) as u64;
            let mut bytes = vec![tag::ENUM];
            put_str(&mut bytes, definition_key);
            put_str(&mut bytes, variant);
            finish(bytes, true, out, seen)
        }
        CanonValue::Media {
            kind,
            mime,
            content_kind,
            content,
        } => {
            out.logical_len += content.len() as u64;
            let mut bytes = vec![tag::MEDIA];
            put_str(&mut bytes, kind);
            match mime {
                Some(m) => {
                    bytes.push(1);
                    put_str(&mut bytes, m);
                }
                None => bytes.push(0),
            }
            bytes.push(*content_kind);
            let body = encode_body(tag::BYTES, content.as_bytes(), out, seen);
            put_child(&mut bytes, &body);
            materialize(bytes, out, seen)
        }
        CanonValue::Omitted { reason, message } => {
            out.logical_len += 1 + message.len() as u64;
            let mut bytes = vec![tag::OMITTED, *reason];
            put_str(&mut bytes, message);
            finish(bytes, true, out, seen)
        }
    }
}

/// CID references found in one canonical node's bytes.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct NodeRefs {
    pub nodes: Vec<[u8; 32]>,
    pub chunks: Vec<[u8; 32]>,
}

/// Structural ref scan over the FROZEN node layout — the GC closure walk
/// (§6.7 mark). Returns `None` on malformed bytes (the caller treats the
/// node as ref-free rather than failing the whole mark).
#[must_use]
pub fn node_refs(bytes: &[u8]) -> Option<NodeRefs> {
    let mut refs = NodeRefs::default();
    scan_node(bytes, &mut refs)?;
    Some(refs)
}

fn rd_u32(bytes: &[u8], at: &mut usize) -> Option<u32> {
    let v = u32::from_le_bytes(bytes.get(*at..*at + 4)?.try_into().ok()?);
    *at += 4;
    Some(v)
}

fn skip_str(bytes: &[u8], at: &mut usize) -> Option<()> {
    let len = rd_u32(bytes, at)? as usize;
    *at = at.checked_add(len)?;
    (*at <= bytes.len()).then_some(())
}

/// One child slot: 0 inline (u16 len + bytes — recursed, entries may hold
/// refs) | 1 ref. `map_entry` selects entry-vs-node semantics for inline
/// payloads.
fn scan_slot(bytes: &[u8], at: &mut usize, map_entry: bool, refs: &mut NodeRefs) -> Option<()> {
    match *bytes.get(*at)? {
        0 => {
            *at += 1;
            let len = u16::from_le_bytes(bytes.get(*at..*at + 2)?.try_into().ok()?) as usize;
            *at += 2;
            let inner = bytes.get(*at..*at + len)?;
            *at += len;
            if map_entry {
                scan_map_entry(inner, refs)?;
            } else {
                scan_node(inner, refs)?;
            }
            Some(())
        }
        1 => {
            *at += 1;
            refs.nodes.push(bytes.get(*at..*at + 32)?.try_into().ok()?);
            *at += 32;
            Some(())
        }
        _ => None,
    }
}

/// Map entry: key (u32 len + bytes, or 0xFFFF_FFFF sentinel + key slot)
/// then the value slot.
fn scan_map_entry(bytes: &[u8], refs: &mut NodeRefs) -> Option<()> {
    let mut at = 0usize;
    let key_len = rd_u32(bytes, &mut at)?;
    if key_len == u32::MAX {
        scan_slot(bytes, &mut at, false, refs)?;
    } else {
        at = at.checked_add(key_len as usize)?;
        if at > bytes.len() {
            return None;
        }
    }
    scan_slot(bytes, &mut at, false, refs)
}

fn scan_node(bytes: &[u8], refs: &mut NodeRefs) -> Option<()> {
    let mut at = 0usize;
    let t = *bytes.first()?;
    at += 1;
    match t {
        tag::NULL
        | tag::BOOL
        | tag::INT
        | tag::FLOAT
        | tag::BIGINT
        | tag::STRING
        | tag::BYTES
        | tag::ENUM
        | tag::OMITTED => Some(()),
        tag::CHUNKED => {
            at += 1 + 8; // kind + logical_len
            let count = rd_u32(bytes, &mut at)? as usize;
            for _ in 0..count {
                refs.chunks.push(bytes.get(at..at + 32)?.try_into().ok()?);
                at += 32;
            }
            Some(())
        }
        tag::LIST | tag::MAP => {
            at += 8; // total count u64
            let n = rd_u32(bytes, &mut at)? as usize;
            for _ in 0..n {
                scan_slot(bytes, &mut at, t == tag::MAP, refs)?;
            }
            Some(())
        }
        tag::SEGMENT => {
            let header_tag = *bytes.get(at)?;
            at += 1;
            let n = rd_u32(bytes, &mut at)? as usize;
            for _ in 0..n {
                scan_slot(bytes, &mut at, header_tag == tag::MAP, refs)?;
            }
            Some(())
        }
        tag::CLASS => {
            skip_str(bytes, &mut at)?;
            let n = rd_u32(bytes, &mut at)? as usize;
            for _ in 0..n {
                skip_str(bytes, &mut at)?;
                let presence = *bytes.get(at)?;
                at += 1;
                if presence >= 2 {
                    scan_slot(bytes, &mut at, false, refs)?;
                }
            }
            Some(())
        }
        tag::MEDIA => {
            skip_str(bytes, &mut at)?;
            match *bytes.get(at)? {
                0 => at += 1,
                _ => {
                    at += 1;
                    skip_str(bytes, &mut at)?;
                }
            }
            at += 1; // content_kind
            scan_slot(bytes, &mut at, false, refs)
        }
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Decode: the exact inverse of `encode` (§8.4 `get()` hydration path).
//
// The decoder walks the FROZEN layout above, fetching referenced nodes and
// chunks through a [`DagSource`]. Hydration is byte- and depth-budgeted
// (§8.4 "bounded by construction"): when a budget binds, the subtree is
// **elided whole** — never decoded partially — replaced by a synthetic
// [`CanonValue::Omitted`] with [`ELIDED_REASON`], and its CID is recorded
// in [`DecodeBudget::elided`] as the child handle for selective descent.
// ---------------------------------------------------------------------------

/// Synthetic `Omitted.reason` for budget/depth-elided subtrees. DECODE-side
/// only — the encoder never produces it; engine reasons stay 0..=4.
pub const ELIDED_REASON: u8 = 255;

/// Hostile-input guard: segment trees deeper than this are malformed (a
/// legitimate 128-ary tree of u64-count entries is ≤ 10 levels).
const SEGMENT_DEPTH_MAX: u32 = 64;

/// Where a decode fetches referenced bytes from (the CAS, a test map, …).
pub trait DagSource {
    /// Canonical node bytes for `cid` ([`ChunkKind::Node`]-class content).
    fn node(&mut self, cid: &[u8; 32]) -> Option<Vec<u8>>;
    /// Raw chunk bytes for `cid` ([`ChunkKind::Chunk`]-class content).
    fn chunk(&mut self, cid: &[u8; 32]) -> Option<Vec<u8>>;
}

#[derive(Debug, PartialEq, Eq)]
pub enum DecodeError {
    /// Structurally invalid canonical bytes (never store corruption — packs
    /// are CRC-checked below this layer).
    Malformed(&'static str),
    MissingNode([u8; 32]),
    MissingChunk([u8; 32]),
}

impl std::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DecodeError::Malformed(what) => write!(f, "malformed canonical value: {what}"),
            DecodeError::MissingNode(cid) => {
                write!(f, "missing value node {}", cid_wire(cid))
            }
            DecodeError::MissingChunk(cid) => {
                write!(f, "missing value chunk {}", cid_wire(cid))
            }
        }
    }
}

/// §8.4 `get()` budget: fetched-byte ceiling + ref-descent ceiling.
/// Segment/chunk internals count toward bytes but NOT depth (they are
/// encoding plumbing, invisible to the value's logical shape).
#[derive(Debug)]
pub struct DecodeBudget {
    pub max_bytes: usize,
    pub max_depth: u32,
    /// Bytes of referenced nodes/chunks fetched so far.
    pub spent: usize,
    /// CIDs elided by budget or depth — the handles to descend into next.
    pub elided: Vec<[u8; 32]>,
}

impl DecodeBudget {
    #[must_use]
    pub fn unbounded() -> DecodeBudget {
        DecodeBudget {
            max_bytes: usize::MAX,
            max_depth: u32::MAX,
            spent: 0,
            elided: Vec::new(),
        }
    }

    /// The `get(max_bytes=…, depth=…)` shape.
    #[must_use]
    pub fn bounded(max_bytes: usize, max_depth: u32) -> DecodeBudget {
        DecodeBudget {
            max_bytes,
            max_depth,
            spent: 0,
            elided: Vec::new(),
        }
    }

    fn remaining(&self) -> usize {
        self.max_bytes.saturating_sub(self.spent)
    }

    fn elide(&mut self, cid: [u8; 32]) -> CanonValue {
        self.elided.push(cid);
        CanonValue::Omitted {
            reason: ELIDED_REASON,
            message: cid_wire(&cid),
        }
    }
}

/// Full decode of one canonical value from its root node bytes.
pub fn decode(root: &[u8], src: &mut dyn DagSource) -> Result<CanonValue, DecodeError> {
    let mut budget = DecodeBudget::unbounded();
    decode_budgeted(root, src, &mut budget)
}

/// Budgeted decode; inspect `budget.elided`/`spent` afterwards.
pub fn decode_budgeted(
    root: &[u8],
    src: &mut dyn DagSource,
    budget: &mut DecodeBudget,
) -> Result<CanonValue, DecodeError> {
    decode_node_bytes(root, src, budget, 0)
}

/// Bounds-checked little-endian reader over one node's bytes.
struct Rd<'a> {
    b: &'a [u8],
    at: usize,
}

impl<'a> Rd<'a> {
    fn new(b: &'a [u8]) -> Rd<'a> {
        Rd { b, at: 0 }
    }
    fn u8(&mut self) -> Result<u8, DecodeError> {
        let v = *self
            .b
            .get(self.at)
            .ok_or(DecodeError::Malformed("truncated u8"))?;
        self.at += 1;
        Ok(v)
    }
    fn u16(&mut self) -> Result<u16, DecodeError> {
        let s = self
            .b
            .get(self.at..self.at + 2)
            .ok_or(DecodeError::Malformed("truncated u16"))?;
        self.at += 2;
        Ok(u16::from_le_bytes(s.try_into().expect("2 bytes")))
    }
    fn u32(&mut self) -> Result<u32, DecodeError> {
        let s = self
            .b
            .get(self.at..self.at + 4)
            .ok_or(DecodeError::Malformed("truncated u32"))?;
        self.at += 4;
        Ok(u32::from_le_bytes(s.try_into().expect("4 bytes")))
    }
    fn u64(&mut self) -> Result<u64, DecodeError> {
        let s = self
            .b
            .get(self.at..self.at + 8)
            .ok_or(DecodeError::Malformed("truncated u64"))?;
        self.at += 8;
        Ok(u64::from_le_bytes(s.try_into().expect("8 bytes")))
    }
    fn take(&mut self, n: usize) -> Result<&'a [u8], DecodeError> {
        let s = self
            .b
            .get(
                self.at
                    ..self
                        .at
                        .checked_add(n)
                        .ok_or(DecodeError::Malformed("length overflow"))?,
            )
            .ok_or(DecodeError::Malformed("truncated bytes"))?;
        self.at += n;
        Ok(s)
    }
    fn cid(&mut self) -> Result<[u8; 32], DecodeError> {
        Ok(self.take(32)?.try_into().expect("32 bytes"))
    }
    fn str(&mut self) -> Result<String, DecodeError> {
        let len = self.u32()? as usize;
        let bytes = self.take(len)?;
        String::from_utf8(bytes.to_vec()).map_err(|_| DecodeError::Malformed("non-UTF-8 string"))
    }
    fn done(&self) -> bool {
        self.at == self.b.len()
    }
}

/// One child slot as written by `put_child`.
enum RawSlot<'a> {
    Inline(&'a [u8]),
    Ref([u8; 32]),
}

fn read_raw_slot<'a>(rd: &mut Rd<'a>) -> Result<RawSlot<'a>, DecodeError> {
    match rd.u8()? {
        0 => {
            let len = rd.u16()? as usize;
            Ok(RawSlot::Inline(rd.take(len)?))
        }
        1 => Ok(RawSlot::Ref(rd.cid()?)),
        _ => Err(DecodeError::Malformed("unknown slot marker")),
    }
}

/// Fetch a referenced node's bytes, charging the byte budget. `None` means
/// the budget elided it (caller substitutes the placeholder).
fn fetch_node(
    cid: [u8; 32],
    src: &mut dyn DagSource,
    budget: &mut DecodeBudget,
) -> Result<Option<Vec<u8>>, DecodeError> {
    if budget.remaining() == 0 {
        return Ok(None);
    }
    let bytes = src.node(&cid).ok_or(DecodeError::MissingNode(cid))?;
    budget.spent += bytes.len();
    if budget.spent > budget.max_bytes {
        return Ok(None);
    }
    Ok(Some(bytes))
}

/// Decode a VALUE-position ref (a real child, not a segment): depth-gated.
fn decode_ref(
    cid: [u8; 32],
    src: &mut dyn DagSource,
    budget: &mut DecodeBudget,
    depth: u32,
) -> Result<CanonValue, DecodeError> {
    if depth >= budget.max_depth {
        return Ok(budget.elide(cid));
    }
    let Some(bytes) = fetch_node(cid, src, budget)? else {
        return Ok(budget.elide(cid));
    };
    decode_node_bytes(&bytes, src, budget, depth + 1)
}

/// LIST-context slot: an element (inline or ref), where a ref may be an
/// internal SEGMENT node to splice.
fn decode_list_slot(
    slot: RawSlot<'_>,
    src: &mut dyn DagSource,
    budget: &mut DecodeBudget,
    depth: u32,
    seg_depth: u32,
    out: &mut Vec<CanonValue>,
) -> Result<(), DecodeError> {
    match slot {
        RawSlot::Inline(inner) => {
            out.push(decode_node_bytes(inner, src, budget, depth)?);
            Ok(())
        }
        RawSlot::Ref(cid) => {
            if seg_depth > SEGMENT_DEPTH_MAX {
                return Err(DecodeError::Malformed("segment tree too deep"));
            }
            let Some(bytes) = fetch_node(cid, src, budget)? else {
                out.push(budget.elide(cid));
                return Ok(());
            };
            if bytes.first() == Some(&tag::SEGMENT) {
                let mut rd = Rd::new(&bytes);
                let _ = rd.u8()?; // SEGMENT
                let header = rd.u8()?;
                if header != tag::LIST {
                    return Err(DecodeError::Malformed("list segment with wrong header"));
                }
                let n = rd.u32()? as usize;
                for _ in 0..n {
                    let slot = read_raw_slot(&mut rd)?;
                    decode_list_slot(slot, src, budget, depth, seg_depth + 1, out)?;
                }
                Ok(())
            } else {
                // A real element: charge depth for the descent.
                if depth >= budget.max_depth {
                    budget.spent -= bytes.len(); // not descended; refund
                    out.push(budget.elide(cid));
                    return Ok(());
                }
                out.push(decode_node_bytes(&bytes, src, budget, depth + 1)?);
                Ok(())
            }
        }
    }
}

/// MAP-context slot: an inline entry `[key][value-slot]`, or a ref to an
/// internal SEGMENT of entries.
fn decode_map_slot(
    slot: RawSlot<'_>,
    src: &mut dyn DagSource,
    budget: &mut DecodeBudget,
    depth: u32,
    seg_depth: u32,
    out: &mut Vec<(String, CanonValue)>,
) -> Result<(), DecodeError> {
    match slot {
        RawSlot::Inline(entry) => {
            let mut rd = Rd::new(entry);
            let key_len = rd.u32()?;
            let key = if key_len == u32::MAX {
                // Indirect key (> MAX_DIRECT_KEY): a string-node slot.
                let key_slot = read_raw_slot(&mut rd)?;
                let key_value = match key_slot {
                    RawSlot::Inline(inner) => decode_node_bytes(inner, src, budget, depth)?,
                    RawSlot::Ref(cid) => {
                        // Keys are structural: never elide, never depth-gate
                        // (a map without its keys is not a map).
                        let bytes = src.node(&cid).ok_or(DecodeError::MissingNode(cid))?;
                        budget.spent += bytes.len();
                        decode_node_bytes(&bytes, src, budget, depth)?
                    }
                };
                match key_value {
                    CanonValue::String(s) => s,
                    _ => return Err(DecodeError::Malformed("indirect map key is not a string")),
                }
            } else {
                let bytes = rd.take(key_len as usize)?;
                String::from_utf8(bytes.to_vec())
                    .map_err(|_| DecodeError::Malformed("non-UTF-8 map key"))?
            };
            let value = match read_raw_slot(&mut rd)? {
                RawSlot::Inline(inner) => decode_node_bytes(inner, src, budget, depth)?,
                RawSlot::Ref(cid) => decode_ref(cid, src, budget, depth)?,
            };
            if !rd.done() {
                return Err(DecodeError::Malformed("trailing bytes in map entry"));
            }
            out.push((key, value));
            Ok(())
        }
        RawSlot::Ref(cid) => {
            if seg_depth > SEGMENT_DEPTH_MAX {
                return Err(DecodeError::Malformed("segment tree too deep"));
            }
            let Some(bytes) = fetch_node(cid, src, budget)? else {
                // An elided segment loses ENTRIES, not a value — surface it
                // as an elided entry keyed by the cid so nothing silently
                // disappears.
                out.push((format!("$elided:{}", cid_wire(&cid)), budget.elide(cid)));
                return Ok(());
            };
            if bytes.first() != Some(&tag::SEGMENT) {
                return Err(DecodeError::Malformed("map slot ref is not a segment"));
            }
            let mut rd = Rd::new(&bytes);
            let _ = rd.u8()?;
            let header = rd.u8()?;
            if header != tag::MAP {
                return Err(DecodeError::Malformed("map segment with wrong header"));
            }
            let n = rd.u32()? as usize;
            for _ in 0..n {
                let slot = read_raw_slot(&mut rd)?;
                decode_map_slot(slot, src, budget, depth, seg_depth + 1, out)?;
            }
            Ok(())
        }
    }
}

fn decode_node_bytes(
    bytes: &[u8],
    src: &mut dyn DagSource,
    budget: &mut DecodeBudget,
    depth: u32,
) -> Result<CanonValue, DecodeError> {
    let mut rd = Rd::new(bytes);
    match rd.u8()? {
        tag::NULL => Ok(CanonValue::Null),
        tag::BOOL => Ok(CanonValue::Bool(rd.u8()? != 0)),
        tag::INT => Ok(CanonValue::Int(i64::from_le_bytes(
            rd.take(8)?.try_into().expect("8 bytes"),
        ))),
        tag::FLOAT => Ok(CanonValue::Float(f64::from_bits(rd.u64()?))),
        tag::BIGINT => Ok(CanonValue::Bigint(rd.str()?)),
        tag::STRING => {
            let len = rd.u32()? as usize;
            let body = rd.take(len)?;
            Ok(CanonValue::String(
                String::from_utf8(body.to_vec())
                    .map_err(|_| DecodeError::Malformed("non-UTF-8 string body"))?,
            ))
        }
        tag::BYTES => {
            let len = rd.u32()? as usize;
            Ok(CanonValue::Bytes(rd.take(len)?.to_vec()))
        }
        tag::CHUNKED => {
            let inner = rd.u8()?;
            let logical_len = rd.u64()?;
            let count = rd.u32()? as usize;
            let mut cids = Vec::with_capacity(count);
            for _ in 0..count {
                cids.push(rd.cid()?);
            }
            // Never assemble a partial body (§8.4 "never decode a partial
            // value as whole"): if the whole body cannot fit the budget,
            // elide it and hand back the chunk CIDs.
            let logical =
                usize::try_from(logical_len).map_err(|_| DecodeError::Malformed("body too big"))?;
            if logical > budget.remaining() {
                let placeholder = cids.first().copied().map_or(
                    CanonValue::Omitted {
                        reason: ELIDED_REASON,
                        message: "empty chunked body".to_string(),
                    },
                    |first| {
                        for extra in cids.iter().skip(1) {
                            budget.elided.push(*extra);
                        }
                        budget.elide(first)
                    },
                );
                return Ok(placeholder);
            }
            let mut body = Vec::with_capacity(logical);
            for cid in &cids {
                let chunk = src.chunk(cid).ok_or(DecodeError::MissingChunk(*cid))?;
                budget.spent += chunk.len();
                body.extend_from_slice(&chunk);
            }
            if body.len() as u64 != logical_len {
                return Err(DecodeError::Malformed("chunked body length mismatch"));
            }
            match inner {
                tag::STRING => Ok(CanonValue::String(
                    String::from_utf8(body)
                        .map_err(|_| DecodeError::Malformed("non-UTF-8 chunked string"))?,
                )),
                tag::BYTES => Ok(CanonValue::Bytes(body)),
                _ => Err(DecodeError::Malformed("chunked body with unknown kind")),
            }
        }
        tag::LIST => {
            let total = rd.u64()?;
            let n = rd.u32()? as usize;
            let mut items = Vec::with_capacity(usize::try_from(total).unwrap_or(0).min(4096));
            for _ in 0..n {
                let slot = read_raw_slot(&mut rd)?;
                decode_list_slot(slot, src, budget, depth, 0, &mut items)?;
            }
            Ok(CanonValue::List(items))
        }
        tag::MAP => {
            let total = rd.u64()?;
            let n = rd.u32()? as usize;
            let mut entries = Vec::with_capacity(usize::try_from(total).unwrap_or(0).min(4096));
            for _ in 0..n {
                let slot = read_raw_slot(&mut rd)?;
                decode_map_slot(slot, src, budget, depth, 0, &mut entries)?;
            }
            Ok(CanonValue::Map(entries))
        }
        tag::CLASS => {
            let definition_key = rd.str()?;
            let n = rd.u32()? as usize;
            let mut fields = Vec::with_capacity(n.min(4096));
            for _ in 0..n {
                let name = rd.str()?;
                let presence = match rd.u8()? {
                    0 => Presence::Absent,
                    1 => Presence::Null,
                    2 => Presence::Value,
                    3 => Presence::DefaultFilled,
                    _ => return Err(DecodeError::Malformed("unknown presence byte")),
                };
                let value = if matches!(presence, Presence::Value | Presence::DefaultFilled) {
                    Some(match read_raw_slot(&mut rd)? {
                        RawSlot::Inline(inner) => decode_node_bytes(inner, src, budget, depth)?,
                        RawSlot::Ref(cid) => decode_ref(cid, src, budget, depth)?,
                    })
                } else {
                    None
                };
                fields.push((name, presence, value));
            }
            Ok(CanonValue::Class {
                definition_key,
                fields,
            })
        }
        tag::ENUM => Ok(CanonValue::Enum {
            definition_key: rd.str()?,
            variant: rd.str()?,
        }),
        tag::MEDIA => {
            let kind = rd.str()?;
            let mime = match rd.u8()? {
                0 => None,
                1 => Some(rd.str()?),
                _ => return Err(DecodeError::Malformed("unknown media mime flag")),
            };
            let content_kind = rd.u8()?;
            let body = match read_raw_slot(&mut rd)? {
                RawSlot::Inline(inner) => decode_node_bytes(inner, src, budget, depth)?,
                RawSlot::Ref(cid) => decode_ref(cid, src, budget, depth)?,
            };
            let content = match body {
                CanonValue::Bytes(b) => String::from_utf8(b)
                    .map_err(|_| DecodeError::Malformed("non-UTF-8 media content"))?,
                CanonValue::String(s) => s,
                // Budget-elided media body: keep the placeholder text.
                CanonValue::Omitted { message, .. } => message,
                _ => return Err(DecodeError::Malformed("media body is not bytes")),
            };
            Ok(CanonValue::Media {
                kind,
                mime,
                content_kind,
                content,
            })
        }
        tag::OMITTED => Ok(CanonValue::Omitted {
            reason: rd.u8()?,
            message: rd.str()?,
        }),
        tag::SEGMENT => Err(DecodeError::Malformed("segment node at value position")),
        _ => Err(DecodeError::Malformed("unknown node tag")),
    }
}

// ---------------------------------------------------------------------------
// Schema-erased JSON rendering (the `get()` output shape).
// ---------------------------------------------------------------------------

/// Engine omission-reason names (frozen mapping, trace_heap
/// `canonical_code`).
fn omission_reason_name(reason: u8) -> &'static str {
    match reason {
        0 => "omittedArgument",
        1 => "unsupportedValue",
        2 => "hostOwnedValue",
        3 => "invalidRuntimeValue",
        4 => "cyclicReference",
        ELIDED_REASON => "elided",
        _ => "unknown",
    }
}

/// Render a decoded value as schema-erased JSON: classes become objects
/// (plus `$type`), enums/media/bytes/omissions become tagged objects,
/// budget-elided subtrees become `{"$elided": "<cid>"}`.
#[must_use]
pub fn to_json(value: &CanonValue) -> serde_json::Value {
    use serde_json::{Map, Value, json};
    match value {
        CanonValue::Null => Value::Null,
        CanonValue::Bool(b) => json!(b),
        CanonValue::Int(i) => json!(i),
        CanonValue::Float(f) => {
            if f.is_finite() {
                serde_json::Number::from_f64(*f).map_or_else(|| json!(f.to_string()), Value::Number)
            } else {
                json!(f.to_string())
            }
        }
        CanonValue::Bigint(s) => json!({ "$bigint": s }),
        CanonValue::String(s) => json!(s),
        CanonValue::Bytes(b) => {
            use base64::Engine as _;
            json!({
                "$bytes": {
                    "len": b.len(),
                    "base64": base64::engine::general_purpose::STANDARD.encode(b),
                }
            })
        }
        CanonValue::List(items) => Value::Array(items.iter().map(to_json).collect()),
        CanonValue::Map(entries) => {
            let mut obj = Map::new();
            for (key, value) in entries {
                obj.insert(key.clone(), to_json(value));
            }
            Value::Object(obj)
        }
        CanonValue::Class {
            definition_key,
            fields,
        } => {
            let mut obj = Map::new();
            obj.insert(
                "$type".to_string(),
                json!(
                    definition_key
                        .strip_prefix("class:")
                        .unwrap_or(definition_key)
                ),
            );
            for (name, presence, value) in fields {
                match presence {
                    Presence::Absent => {}
                    Presence::Null => {
                        obj.insert(name.clone(), Value::Null);
                    }
                    Presence::Value | Presence::DefaultFilled => {
                        if let Some(value) = value {
                            obj.insert(name.clone(), to_json(value));
                        }
                    }
                }
            }
            Value::Object(obj)
        }
        CanonValue::Enum {
            definition_key,
            variant,
        } => json!({
            "$enum": definition_key.strip_prefix("enum:").unwrap_or(definition_key),
            "value": variant,
        }),
        CanonValue::Media {
            kind,
            mime,
            content_kind,
            content,
        } => json!({
            "$media": {
                "kind": kind,
                "mime": mime,
                "content_kind": match content_kind { 0 => "url", 1 => "base64", 2 => "file", _ => "unknown" },
                "content": content,
            }
        }),
        CanonValue::Omitted { reason, message } if *reason == ELIDED_REASON => {
            json!({ "$elided": message })
        }
        CanonValue::Omitted { reason, message } => json!({
            "$omitted": { "reason": omission_reason_name(*reason), "message": message }
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cid_of(value: &CanonValue) -> [u8; 32] {
        encode(value).root_cid
    }

    #[test]
    fn scalars_are_deterministic_and_distinct() {
        assert_eq!(cid_of(&CanonValue::Null), cid_of(&CanonValue::Null));
        assert_ne!(cid_of(&CanonValue::Null), cid_of(&CanonValue::Bool(false)));
        assert_ne!(cid_of(&CanonValue::Int(0)), cid_of(&CanonValue::Float(0.0)));
        // All NaNs collapse; ±0.0 stay distinct.
        assert_eq!(
            cid_of(&CanonValue::Float(f64::NAN)),
            cid_of(&CanonValue::Float(f64::from_bits(0x7FF8_dead_beef_0001)))
        );
        assert_ne!(
            cid_of(&CanonValue::Float(0.0)),
            cid_of(&CanonValue::Float(-0.0))
        );
    }

    #[test]
    fn bigint_canonicalizes() {
        for (a, b) in [("007", "7"), ("+42", "42"), ("-007", "-7"), ("000", "0")] {
            assert_eq!(
                cid_of(&CanonValue::Bigint(a.to_string())),
                cid_of(&CanonValue::Bigint(b.to_string())),
                "{a} vs {b}"
            );
        }
        assert_ne!(
            cid_of(&CanonValue::Bigint("7".to_string())),
            cid_of(&CanonValue::Int(7)),
            "bigint 7 and int 7 are different encodings by design"
        );
    }

    #[test]
    fn map_order_is_canonical_and_last_dup_wins() {
        let ab = CanonValue::Map(vec![
            ("a".into(), CanonValue::Int(1)),
            ("b".into(), CanonValue::Int(2)),
        ]);
        let ba = CanonValue::Map(vec![
            ("b".into(), CanonValue::Int(2)),
            ("a".into(), CanonValue::Int(1)),
        ]);
        assert_eq!(cid_of(&ab), cid_of(&ba));

        let dup = CanonValue::Map(vec![
            ("k".into(), CanonValue::Int(1)),
            ("k".into(), CanonValue::Int(2)),
        ]);
        let last = CanonValue::Map(vec![("k".into(), CanonValue::Int(2))]);
        assert_eq!(cid_of(&dup), cid_of(&last));
    }

    #[test]
    fn class_identity_is_definition_key() {
        let a = CanonValue::Class {
            definition_key: "class:user.Box".into(),
            fields: vec![("x".into(), Presence::Value, Some(CanonValue::Int(1)))],
        };
        let renamed = CanonValue::Class {
            definition_key: "class:user.Crate".into(),
            fields: vec![("x".into(), Presence::Value, Some(CanonValue::Int(1)))],
        };
        assert_ne!(cid_of(&a), cid_of(&renamed), "renames change CIDs");
        let null_field = CanonValue::Class {
            definition_key: "class:user.Box".into(),
            fields: vec![("x".into(), Presence::Null, None)],
        };
        let absent_field = CanonValue::Class {
            definition_key: "class:user.Box".into(),
            fields: vec![("x".into(), Presence::Absent, None)],
        };
        assert_ne!(cid_of(&null_field), cid_of(&absent_field));
    }

    #[test]
    fn dedupe_shares_identical_subtrees() {
        let shared = CanonValue::String("x".repeat(4000));
        let value = CanonValue::List(vec![shared.clone(), shared.clone(), shared]);
        let encoded = encode(&value);
        // One node for the string (deduped), one for the list.
        assert_eq!(encoded.nodes.len(), 2, "{:?}", encoded.nodes.len());
    }

    #[test]
    fn long_strings_chunk_and_share() {
        let body = "y".repeat(CHUNK_BYTES * 2 + 10);
        let encoded = encode(&CanonValue::String(body.clone()));
        // Three 128 KiB slots, but the two identical full chunks dedupe to
        // one stored chunk + the 10-byte tail.
        assert_eq!(encoded.chunks.len(), 2);
        assert_eq!(encoded.logical_len, body.len() as u64);

        // Transcript-append shape: the whole-chunk prefix re-derives the
        // same chunk CIDs, so only the changed tail is new.
        let longer = format!("{}{}", body, "z".repeat(5));
        let e2 = encode(&CanonValue::String(longer));
        let shared: usize = e2
            .chunks
            .iter()
            .filter(|(cid, _)| encoded.chunks.iter().any(|(c, _)| c == cid))
            .count();
        assert_eq!(shared, 1, "full 128 KiB prefix chunks dedupe");
        assert_eq!(e2.chunks.len(), 2, "one shared prefix chunk + new tail");
    }

    #[test]
    fn wide_lists_segment_at_128() {
        let items: Vec<CanonValue> = (0..1000).map(CanonValue::Int).collect();
        let encoded = encode(&CanonValue::List(items));
        // 1000 inline ints → 8 segment nodes (⌈1000/128⌉) + 1 list node.
        assert_eq!(encoded.nodes.len(), 9);
    }

    #[test]
    fn node_refs_cover_the_closure() {
        let value = CanonValue::Map(vec![
            ("big".into(), CanonValue::String("q".repeat(300_000))),
            (
                "cls".into(),
                CanonValue::Class {
                    definition_key: "class:user.Box".into(),
                    fields: vec![(
                        "inner".into(),
                        Presence::Value,
                        Some(CanonValue::String("r".repeat(5000))),
                    )],
                },
            ),
            ("small".into(), CanonValue::Int(1)),
        ]);
        let encoded = encode(&value);
        // Walk the closure from the root; every emitted node/chunk must be
        // reachable.
        let by_cid: FxHashMap<[u8; 32], &Vec<u8>> =
            encoded.nodes.iter().map(|(c, b)| (*c, b)).collect();
        let mut reached_nodes = vec![encoded.root_cid];
        let mut reached_chunks: Vec<[u8; 32]> = Vec::new();
        let mut i = 0;
        while i < reached_nodes.len() {
            let bytes = by_cid[&reached_nodes[i]];
            let refs = node_refs(bytes).expect("frozen layout scans");
            for n in refs.nodes {
                if !reached_nodes.contains(&n) {
                    reached_nodes.push(n);
                }
            }
            for c in refs.chunks {
                if !reached_chunks.contains(&c) {
                    reached_chunks.push(c);
                }
            }
            i += 1;
        }
        assert_eq!(reached_nodes.len(), encoded.nodes.len());
        assert_eq!(reached_chunks.len(), encoded.chunks.len());
    }

    struct MapSource {
        nodes: FxHashMap<[u8; 32], Vec<u8>>,
        chunks: FxHashMap<[u8; 32], Vec<u8>>,
    }

    impl MapSource {
        fn of(encoded: &CanonEncoded) -> MapSource {
            MapSource {
                nodes: encoded.nodes.iter().cloned().collect(),
                chunks: encoded.chunks.iter().cloned().collect(),
            }
        }
        fn root_bytes(&self, encoded: &CanonEncoded) -> Vec<u8> {
            self.nodes[&encoded.root_cid].clone()
        }
    }

    impl DagSource for MapSource {
        fn node(&mut self, cid: &[u8; 32]) -> Option<Vec<u8>> {
            self.nodes.get(cid).cloned()
        }
        fn chunk(&mut self, cid: &[u8; 32]) -> Option<Vec<u8>> {
            self.chunks.get(cid).cloned()
        }
    }

    fn round_trip(value: &CanonValue) -> CanonValue {
        let encoded = encode(value);
        let mut src = MapSource::of(&encoded);
        let root = src.root_bytes(&encoded);
        decode(&root, &mut src).expect("decodes")
    }

    /// The kitchen-sink value in ALREADY-CANONICAL shape (sorted map keys,
    /// sorted class fields, canonical bigint) so decode == input exactly.
    fn kitchen_sink() -> CanonValue {
        CanonValue::Map(vec![
            ("aa".into(), CanonValue::Int(-7)),
            ("big".into(), CanonValue::Bigint("42".into())),
            (
                "cls".into(),
                CanonValue::Class {
                    definition_key: "class:user.Box".into(),
                    fields: vec![
                        ("label".into(), Presence::Null, None),
                        ("width".into(), Presence::Value, Some(CanonValue::Int(3))),
                    ],
                },
            ),
            (
                "enm".into(),
                CanonValue::Enum {
                    definition_key: "enum:user.Color".into(),
                    variant: "Red".into(),
                },
            ),
            ("flt".into(), CanonValue::Float(-0.0)),
            (
                "lst".into(),
                CanonValue::List(vec![
                    CanonValue::Bool(true),
                    CanonValue::Null,
                    CanonValue::Bytes(vec![1, 2, 3]),
                ]),
            ),
            (
                "med".into(),
                CanonValue::Media {
                    kind: "image".into(),
                    mime: Some("image/png".into()),
                    content_kind: 0,
                    content: "https://example.test/x.png".into(),
                },
            ),
            (
                "omt".into(),
                CanonValue::Omitted {
                    reason: 4,
                    message: "CyclicReference".into(),
                },
            ),
            ("str".into(), CanonValue::String("hello".into())),
        ])
    }

    #[test]
    fn decode_round_trips_every_variant() {
        let value = kitchen_sink();
        assert_eq!(round_trip(&value), value);
    }

    #[test]
    fn decode_round_trips_chunked_wide_and_deep() {
        // Chunked string (3 chunk slots, 2 distinct after dedupe).
        let long = CanonValue::String("y".repeat(CHUNK_BYTES * 2 + 10));
        assert_eq!(round_trip(&long), long);

        // Wide list (segment splice) and wide map.
        let wide_list = CanonValue::List((0..1000).map(CanonValue::Int).collect());
        assert_eq!(round_trip(&wide_list), wide_list);

        let wide_map = CanonValue::Map(
            (0..300)
                .map(|i| (format!("k{i:04}"), CanonValue::Int(i)))
                .collect(),
        );
        assert_eq!(round_trip(&wide_map), wide_map);

        // Indirect (long) map key.
        let long_key = CanonValue::Map(vec![
            ("a".into(), CanonValue::Int(1)),
            ("k".repeat(MAX_DIRECT_KEY + 10), CanonValue::Int(2)),
        ]);
        assert_eq!(round_trip(&long_key), long_key);

        // Nested classes through refs.
        let nested = CanonValue::Class {
            definition_key: "class:user.Outer".into(),
            fields: vec![(
                "inner".into(),
                Presence::Value,
                Some(CanonValue::Class {
                    definition_key: "class:user.Inner".into(),
                    fields: vec![(
                        "data".into(),
                        Presence::Value,
                        Some(CanonValue::String("x".repeat(5000))),
                    )],
                }),
            )],
        };
        assert_eq!(round_trip(&nested), nested);
    }

    #[test]
    fn decode_canonicalizes_non_canonical_input() {
        // Unsorted map with a duplicate key + sloppy bigint decode to the
        // canonical twin (sorted, last-dup-wins, minimal decimal).
        let sloppy = CanonValue::Map(vec![
            ("b".into(), CanonValue::Bigint("+007".into())),
            ("a".into(), CanonValue::Int(1)),
            ("b".into(), CanonValue::Bigint("0042".into())),
        ]);
        let canonical = CanonValue::Map(vec![
            ("a".into(), CanonValue::Int(1)),
            ("b".into(), CanonValue::Bigint("42".into())),
        ]);
        assert_eq!(round_trip(&sloppy), canonical);
    }

    #[test]
    fn decode_reencode_is_byte_identical() {
        // Strongest inverse proof: decode → encode reproduces the SAME
        // root CID and node/chunk set.
        for value in [
            kitchen_sink(),
            CanonValue::String("z".repeat(CHUNK_BYTES * 3 + 7)),
            CanonValue::List(
                (0..500)
                    .map(|i| CanonValue::String(format!("s{i}")))
                    .collect(),
            ),
        ] {
            let encoded = encode(&value);
            let mut src = MapSource::of(&encoded);
            let root = src.root_bytes(&encoded);
            let decoded = decode(&root, &mut src).expect("decodes");
            let re = encode(&decoded);
            assert_eq!(re.root_cid, encoded.root_cid, "root CID must round-trip");
            assert_eq!(re.nodes.len(), encoded.nodes.len());
            assert_eq!(re.chunks.len(), encoded.chunks.len());
        }
    }

    #[test]
    fn budget_elides_whole_subtrees_and_reports_cids() {
        // A class with one huge field and one small field: a byte budget
        // keeps the small field and elides the huge one WHOLE.
        let value = CanonValue::Class {
            definition_key: "class:user.Doc".into(),
            fields: vec![
                (
                    "body".into(),
                    Presence::Value,
                    Some(CanonValue::String("x".repeat(CHUNK_BYTES * 2))),
                ),
                ("title".into(), Presence::Value, Some(CanonValue::Int(7))),
            ],
        };
        let encoded = encode(&value);
        let mut src = MapSource::of(&encoded);
        let root = src.root_bytes(&encoded);
        let mut budget = DecodeBudget::bounded(1024, u32::MAX);
        let decoded = decode_budgeted(&root, &mut src, &mut budget).expect("decodes");
        assert!(!budget.elided.is_empty(), "elided CIDs reported");
        let CanonValue::Class { fields, .. } = &decoded else {
            panic!("class expected")
        };
        let body = fields.iter().find(|f| f.0 == "body").unwrap();
        assert!(
            matches!(&body.2, Some(CanonValue::Omitted { reason, .. }) if *reason == ELIDED_REASON),
            "huge field elided whole: {body:?}"
        );
        let title = fields.iter().find(|f| f.0 == "title").unwrap();
        assert_eq!(title.2, Some(CanonValue::Int(7)), "small field survives");

        // Depth budget: depth=0 elides every ref child but keeps inlines.
        let mut depth0 = DecodeBudget::bounded(usize::MAX, 0);
        let shallow = decode_budgeted(&root, &mut src, &mut depth0).expect("decodes");
        let CanonValue::Class { fields, .. } = &shallow else {
            panic!("class expected")
        };
        assert_eq!(
            fields.iter().find(|f| f.0 == "title").unwrap().2,
            Some(CanonValue::Int(7)),
            "inline children are free at any depth"
        );
    }

    #[test]
    fn decode_missing_node_errors() {
        let value = CanonValue::List(vec![CanonValue::String("q".repeat(5000))]);
        let encoded = encode(&value);
        let mut src = MapSource::of(&encoded);
        let root = src.root_bytes(&encoded);
        src.nodes.retain(|cid, _| *cid == encoded.root_cid);
        assert!(matches!(
            decode(&root, &mut src),
            Err(DecodeError::MissingNode(_))
        ));
    }

    #[test]
    fn to_json_shapes() {
        let json = to_json(&kitchen_sink());
        assert_eq!(json["aa"], serde_json::json!(-7));
        assert_eq!(json["big"]["$bigint"], "42");
        assert_eq!(json["cls"]["$type"], "user.Box");
        assert_eq!(json["cls"]["width"], 3);
        assert_eq!(json["cls"]["label"], serde_json::Value::Null);
        assert_eq!(json["enm"]["$enum"], "user.Color");
        assert_eq!(json["enm"]["value"], "Red");
        assert_eq!(json["lst"][2]["$bytes"]["len"], 3);
        assert_eq!(json["med"]["$media"]["kind"], "image");
        assert_eq!(json["omt"]["$omitted"]["reason"], "cyclicReference");
        assert_eq!(json["str"], "hello");
        // Elided placeholder renders as a handle.
        let elided = to_json(&CanonValue::Omitted {
            reason: ELIDED_REASON,
            message: "bamlv_1_XYZ".into(),
        });
        assert_eq!(elided["$elided"], "bamlv_1_XYZ");
    }

    #[test]
    fn cid_wire_form() {
        let wire = cid_wire(&[0u8; 32]);
        assert!(wire.starts_with("bamlv_1_"));
        assert_eq!(wire.len(), 8 + 43);
        assert!(!wire.contains('='));
    }
}
