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

    #[test]
    fn cid_wire_form() {
        let wire = cid_wire(&[0u8; 32]);
        assert!(wire.starts_with("bamlv_1_"));
        assert_eq!(wire.len(), 8 + 43);
        assert!(!wire.contains('='));
    }
}
