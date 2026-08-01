use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt,
};

use super::{BYTE_CHUNK_LEN, COLLECTION_CHUNK_LEN, Cid, NODE_CODEC_VERSION, NODE_INLINE_THRESHOLD};

const TAG_NULL: u8 = 0x00;
const TAG_FALSE: u8 = 0x01;
const TAG_TRUE: u8 = 0x02;
const TAG_INT: u8 = 0x03;
const TAG_FLOAT: u8 = 0x04;
const TAG_BIGINT: u8 = 0x05;
const TAG_STRING: u8 = 0x06;
const TAG_BYTES: u8 = 0x07;
const TAG_STRING_BRANCH: u8 = 0x08;
const TAG_BYTES_BRANCH: u8 = 0x09;
const TAG_LIST_LEAF: u8 = 0x0a;
const TAG_LIST_BRANCH: u8 = 0x0b;
const TAG_MAP_LEAF: u8 = 0x0c;
const TAG_MAP_BRANCH: u8 = 0x0d;
const TAG_CLASS_LEAF: u8 = 0x0e;
const TAG_CLASS_BRANCH: u8 = 0x0f;
const TAG_ENUM: u8 = 0x10;
const TAG_MEDIA: u8 = 0x11;
const TAG_OMISSION: u8 = 0x12;

const CHILD_INLINE: u8 = 0;
const CHILD_CID: u8 = 1;

/// Schema-aware structural input to the canonical value DAG encoder.
#[derive(Clone, Debug, PartialEq)]
pub enum CanonicalValue {
    Null,
    Bool(bool),
    Int(i64),
    /// Base-10 integer text. The encoder rejects non-minimal spellings.
    BigInt(String),
    Float(f64),
    String(String),
    Bytes(Vec<u8>),
    List(Vec<Self>),
    /// String-keyed map. Input order is ignored; duplicate keys are rejected.
    Map(Vec<(String, Self)>),
    Class {
        definition_key: String,
        fields: Vec<CanonicalField>,
    },
    Enum {
        definition_key: String,
        variant: String,
    },
    Media(MediaValue),
    Omitted(OmissionValue),
}

#[derive(Clone, Debug, PartialEq)]
pub struct CanonicalField {
    pub name: String,
    pub presence: FieldPresence,
}

#[derive(Clone, Debug, PartialEq)]
pub enum FieldPresence {
    /// The declared field was not supplied.
    Absent,
    /// The field was supplied. `Present(Null)` is distinct from `Absent`.
    Present(CanonicalValue),
    /// The runtime supplied a declared default.
    DefaultFilled(CanonicalValue),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MediaValue {
    pub kind: String,
    pub mime_type: Option<String>,
    pub content: MediaContent,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MediaContent {
    Url(String),
    Base64(String),
    File(String),
    Bytes(Vec<u8>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OmissionValue {
    pub reason: String,
    pub message: String,
}

/// One independently addressable canonical node.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DagChunk {
    pub cid: Cid,
    pub canonical_bytes: Vec<u8>,
    pub logical_len: u64,
}

/// A canonical DAG and all chunks required to hydrate its root.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValueDag {
    pub root: Cid,
    pub node_codec_version: u16,
    pub logical_len: u64,
    /// Sorted by CID for stable pack append order.
    pub chunks: Vec<DagChunk>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CanonicalEncodeError {
    DuplicateMapKey(String),
    DuplicateClassField(String),
    EmptyDefinitionKey,
    InvalidBigInt(String),
    LengthOverflow,
    InvalidNode(&'static str),
    TrailingNodeBytes,
}

impl fmt::Display for CanonicalEncodeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateMapKey(key) => write!(formatter, "duplicate map key `{key}`"),
            Self::DuplicateClassField(field) => {
                write!(formatter, "duplicate class field `{field}`")
            }
            Self::EmptyDefinitionKey => formatter.write_str("definition_key must not be empty"),
            Self::InvalidBigInt(value) => {
                write!(formatter, "big integer is not minimally encoded: `{value}`")
            }
            Self::LengthOverflow => formatter.write_str("canonical value length exceeds u64"),
            Self::InvalidNode(message) => write!(formatter, "invalid canonical node: {message}"),
            Self::TrailingNodeBytes => formatter.write_str("canonical node has trailing bytes"),
        }
    }
}

impl Error for CanonicalEncodeError {}

#[derive(Clone, Debug)]
struct EncodedNode {
    bytes: Vec<u8>,
    logical_len: u64,
    descendants: BTreeMap<Cid, DagChunk>,
}

/// Encode one semantic value into a deterministic, fixed-chunked DAG.
pub fn encode_value_dag(value: &CanonicalValue) -> Result<ValueDag, CanonicalEncodeError> {
    let encoded = encode_node(value)?;
    let root = Cid::for_node(&encoded.bytes);
    let root_chunk = DagChunk {
        cid: root,
        canonical_bytes: encoded.bytes,
        logical_len: encoded.logical_len,
    };
    let mut chunks = encoded.descendants;
    chunks.insert(root, root_chunk);
    Ok(ValueDag {
        root,
        node_codec_version: NODE_CODEC_VERSION,
        logical_len: encoded.logical_len,
        chunks: chunks.into_values().collect(),
    })
}

fn encode_node(value: &CanonicalValue) -> Result<EncodedNode, CanonicalEncodeError> {
    match value {
        CanonicalValue::Null => Ok(leaf(vec![TAG_NULL], 0)),
        CanonicalValue::Bool(false) => Ok(leaf(vec![TAG_FALSE], 1)),
        CanonicalValue::Bool(true) => Ok(leaf(vec![TAG_TRUE], 1)),
        CanonicalValue::Int(value) => {
            let mut bytes = vec![TAG_INT];
            bytes.extend_from_slice(&value.to_le_bytes());
            Ok(leaf(bytes, 8))
        }
        CanonicalValue::BigInt(value) => {
            validate_bigint(value)?;
            let mut bytes = vec![TAG_BIGINT];
            put_bytes(&mut bytes, value.as_bytes())?;
            Ok(leaf(bytes, usize_to_u64(value.len())?))
        }
        CanonicalValue::Float(value) => {
            let bits = canonical_float_bits(*value);
            let mut bytes = vec![TAG_FLOAT];
            bytes.extend_from_slice(&bits.to_le_bytes());
            Ok(leaf(bytes, 8))
        }
        CanonicalValue::String(value) => {
            encode_byte_sequence(value.as_bytes(), TAG_STRING, TAG_STRING_BRANCH)
        }
        CanonicalValue::Bytes(value) => encode_byte_sequence(value, TAG_BYTES, TAG_BYTES_BRANCH),
        CanonicalValue::List(values) => encode_list(values),
        CanonicalValue::Map(entries) => encode_map(entries),
        CanonicalValue::Class {
            definition_key,
            fields,
        } => encode_class(definition_key, fields),
        CanonicalValue::Enum {
            definition_key,
            variant,
        } => {
            validate_definition_key(definition_key)?;
            let mut bytes = vec![TAG_ENUM];
            put_bytes(&mut bytes, definition_key.as_bytes())?;
            put_bytes(&mut bytes, variant.as_bytes())?;
            Ok(leaf(
                bytes,
                checked_sum([
                    usize_to_u64(definition_key.len())?,
                    usize_to_u64(variant.len())?,
                ])?,
            ))
        }
        CanonicalValue::Media(media) => encode_media(media),
        CanonicalValue::Omitted(omission) => {
            let mut bytes = vec![TAG_OMISSION];
            put_bytes(&mut bytes, omission.reason.as_bytes())?;
            put_bytes(&mut bytes, omission.message.as_bytes())?;
            Ok(leaf(
                bytes,
                checked_sum([
                    usize_to_u64(omission.reason.len())?,
                    usize_to_u64(omission.message.len())?,
                ])?,
            ))
        }
    }
}

fn encode_byte_sequence(
    bytes: &[u8],
    leaf_tag: u8,
    branch_tag: u8,
) -> Result<EncodedNode, CanonicalEncodeError> {
    if bytes.len() <= BYTE_CHUNK_LEN {
        let mut output = vec![leaf_tag];
        put_bytes(&mut output, bytes)?;
        return Ok(leaf(output, usize_to_u64(bytes.len())?));
    }

    let logical_len = usize_to_u64(bytes.len())?;
    let mut descendants = BTreeMap::new();
    let mut level = Vec::new();
    for chunk in bytes.chunks(BYTE_CHUNK_LEN) {
        let mut chunk_bytes = vec![leaf_tag];
        put_bytes(&mut chunk_bytes, chunk)?;
        let node = leaf(chunk_bytes, usize_to_u64(chunk.len())?);
        level.push(store_node(node, &mut descendants));
    }
    encode_cid_tree(
        branch_tag,
        logical_len,
        level,
        descendants,
        COLLECTION_CHUNK_LEN,
    )
}

fn encode_list(values: &[CanonicalValue]) -> Result<EncodedNode, CanonicalEncodeError> {
    if values.len() <= COLLECTION_CHUNK_LEN {
        return encode_list_leaf(values);
    }
    let logical_len = sum_value_logical_len(values)?;
    let mut descendants = BTreeMap::new();
    let mut level = Vec::new();
    for values in values.chunks(COLLECTION_CHUNK_LEN) {
        let node = encode_list_leaf(values)?;
        level.push(store_node(node, &mut descendants));
    }
    encode_cid_tree(
        TAG_LIST_BRANCH,
        logical_len,
        level,
        descendants,
        COLLECTION_CHUNK_LEN,
    )
}

fn encode_list_leaf(values: &[CanonicalValue]) -> Result<EncodedNode, CanonicalEncodeError> {
    let mut bytes = vec![TAG_LIST_LEAF];
    put_u32(&mut bytes, values.len())?;
    let mut descendants = BTreeMap::new();
    let mut logical_len = 0_u64;
    for value in values {
        let child = encode_node(value)?;
        logical_len = logical_len
            .checked_add(child.logical_len)
            .ok_or(CanonicalEncodeError::LengthOverflow)?;
        encode_child(&mut bytes, child, &mut descendants)?;
    }
    Ok(EncodedNode {
        bytes,
        logical_len,
        descendants,
    })
}

fn encode_map(entries: &[(String, CanonicalValue)]) -> Result<EncodedNode, CanonicalEncodeError> {
    let mut sorted = entries.iter().collect::<Vec<_>>();
    sorted.sort_by(|left, right| left.0.as_bytes().cmp(right.0.as_bytes()));
    reject_duplicate_names(
        sorted.iter().map(|entry| entry.0.as_str()),
        CanonicalEncodeError::DuplicateMapKey,
    )?;
    if sorted.len() <= COLLECTION_CHUNK_LEN {
        return encode_map_leaf(&sorted);
    }
    let mut descendants = BTreeMap::new();
    let mut level = Vec::new();
    let mut logical_len = 0_u64;
    for entries in sorted.chunks(COLLECTION_CHUNK_LEN) {
        let node = encode_map_leaf(entries)?;
        logical_len = logical_len
            .checked_add(node.logical_len)
            .ok_or(CanonicalEncodeError::LengthOverflow)?;
        level.push(store_node(node, &mut descendants));
    }
    encode_cid_tree(
        TAG_MAP_BRANCH,
        logical_len,
        level,
        descendants,
        COLLECTION_CHUNK_LEN,
    )
}

fn encode_map_leaf(
    entries: &[&(String, CanonicalValue)],
) -> Result<EncodedNode, CanonicalEncodeError> {
    let mut bytes = vec![TAG_MAP_LEAF];
    put_u32(&mut bytes, entries.len())?;
    let mut descendants = BTreeMap::new();
    let mut logical_len = 0_u64;
    for (key, value) in entries {
        put_bytes(&mut bytes, key.as_bytes())?;
        logical_len = logical_len
            .checked_add(usize_to_u64(key.len())?)
            .ok_or(CanonicalEncodeError::LengthOverflow)?;
        let child = encode_node(value)?;
        logical_len = logical_len
            .checked_add(child.logical_len)
            .ok_or(CanonicalEncodeError::LengthOverflow)?;
        encode_child(&mut bytes, child, &mut descendants)?;
    }
    Ok(EncodedNode {
        bytes,
        logical_len,
        descendants,
    })
}

fn encode_class(
    definition_key: &str,
    fields: &[CanonicalField],
) -> Result<EncodedNode, CanonicalEncodeError> {
    validate_definition_key(definition_key)?;
    let mut sorted = fields.iter().collect::<Vec<_>>();
    sorted.sort_by(|left, right| left.name.as_bytes().cmp(right.name.as_bytes()));
    reject_duplicate_names(
        sorted.iter().map(|field| field.name.as_str()),
        CanonicalEncodeError::DuplicateClassField,
    )?;
    if sorted.len() <= COLLECTION_CHUNK_LEN {
        return encode_class_leaf(definition_key, &sorted);
    }
    let mut descendants = BTreeMap::new();
    let mut level = Vec::new();
    let mut logical_len = usize_to_u64(definition_key.len())?;
    for fields in sorted.chunks(COLLECTION_CHUNK_LEN) {
        let node = encode_class_leaf("", fields)?;
        logical_len = logical_len
            .checked_add(node.logical_len)
            .ok_or(CanonicalEncodeError::LengthOverflow)?;
        level.push(store_node(node, &mut descendants));
    }
    let mut node = encode_cid_tree(
        TAG_CLASS_BRANCH,
        logical_len,
        level,
        descendants,
        COLLECTION_CHUNK_LEN,
    )?;
    let mut bytes = vec![TAG_CLASS_BRANCH];
    put_bytes(&mut bytes, definition_key.as_bytes())?;
    bytes.extend_from_slice(&node.bytes[1..]);
    node.bytes = bytes;
    Ok(node)
}

fn encode_class_leaf(
    definition_key: &str,
    fields: &[&CanonicalField],
) -> Result<EncodedNode, CanonicalEncodeError> {
    let mut bytes = vec![TAG_CLASS_LEAF];
    put_bytes(&mut bytes, definition_key.as_bytes())?;
    put_u32(&mut bytes, fields.len())?;
    let mut descendants = BTreeMap::new();
    let mut logical_len = usize_to_u64(definition_key.len())?;
    for field in fields {
        put_bytes(&mut bytes, field.name.as_bytes())?;
        logical_len = logical_len
            .checked_add(usize_to_u64(field.name.len())?)
            .ok_or(CanonicalEncodeError::LengthOverflow)?;
        match &field.presence {
            FieldPresence::Absent => bytes.push(0),
            FieldPresence::Present(value) => {
                bytes.push(1);
                let child = encode_node(value)?;
                logical_len = logical_len
                    .checked_add(child.logical_len)
                    .ok_or(CanonicalEncodeError::LengthOverflow)?;
                encode_child(&mut bytes, child, &mut descendants)?;
            }
            FieldPresence::DefaultFilled(value) => {
                bytes.push(2);
                let child = encode_node(value)?;
                logical_len = logical_len
                    .checked_add(child.logical_len)
                    .ok_or(CanonicalEncodeError::LengthOverflow)?;
                encode_child(&mut bytes, child, &mut descendants)?;
            }
        }
    }
    Ok(EncodedNode {
        bytes,
        logical_len,
        descendants,
    })
}

fn encode_media(media: &MediaValue) -> Result<EncodedNode, CanonicalEncodeError> {
    let mut bytes = vec![TAG_MEDIA];
    put_bytes(&mut bytes, media.kind.as_bytes())?;
    match &media.mime_type {
        None => bytes.push(0),
        Some(mime_type) => {
            bytes.push(1);
            put_bytes(&mut bytes, mime_type.as_bytes())?;
        }
    }
    let (content_tag, content) = match &media.content {
        MediaContent::Url(value) => (0, value.as_bytes()),
        MediaContent::Base64(value) => (1, value.as_bytes()),
        MediaContent::File(value) => (2, value.as_bytes()),
        MediaContent::Bytes(value) => (3, value.as_slice()),
    };
    bytes.push(content_tag);
    put_bytes(&mut bytes, content)?;
    Ok(leaf(
        bytes,
        checked_sum([
            usize_to_u64(media.kind.len())?,
            usize_to_u64(media.mime_type.as_ref().map_or(0, String::len))?,
            usize_to_u64(content.len())?,
        ])?,
    ))
}

fn encode_cid_tree(
    branch_tag: u8,
    logical_len: u64,
    mut level: Vec<Cid>,
    mut descendants: BTreeMap<Cid, DagChunk>,
    fanout: usize,
) -> Result<EncodedNode, CanonicalEncodeError> {
    while level.len() > fanout {
        let mut next = Vec::new();
        for cids in level.chunks(fanout) {
            let node = cid_branch(branch_tag, logical_len, cids)?;
            next.push(store_node(node, &mut descendants));
        }
        level = next;
    }
    let mut root = cid_branch(branch_tag, logical_len, &level)?;
    root.descendants.append(&mut descendants);
    Ok(root)
}

fn cid_branch(
    tag: u8,
    logical_len: u64,
    cids: &[Cid],
) -> Result<EncodedNode, CanonicalEncodeError> {
    let mut bytes = vec![tag];
    bytes.extend_from_slice(&logical_len.to_le_bytes());
    put_u32(&mut bytes, cids.len())?;
    for cid in cids {
        bytes.extend_from_slice(cid.as_bytes());
    }
    Ok(leaf(bytes, logical_len))
}

fn encode_child(
    output: &mut Vec<u8>,
    mut child: EncodedNode,
    descendants: &mut BTreeMap<Cid, DagChunk>,
) -> Result<(), CanonicalEncodeError> {
    descendants.append(&mut child.descendants);
    if child.bytes.len() <= NODE_INLINE_THRESHOLD {
        output.push(CHILD_INLINE);
        put_u32(output, child.bytes.len())?;
        output.extend_from_slice(&child.bytes);
    } else {
        output.push(CHILD_CID);
        let cid = Cid::for_node(&child.bytes);
        descendants.entry(cid).or_insert(DagChunk {
            cid,
            canonical_bytes: child.bytes,
            logical_len: child.logical_len,
        });
        output.extend_from_slice(cid.as_bytes());
    }
    Ok(())
}

fn store_node(node: EncodedNode, descendants: &mut BTreeMap<Cid, DagChunk>) -> Cid {
    descendants.extend(node.descendants);
    let cid = Cid::for_node(&node.bytes);
    descendants.entry(cid).or_insert(DagChunk {
        cid,
        canonical_bytes: node.bytes,
        logical_len: node.logical_len,
    });
    cid
}

fn leaf(bytes: Vec<u8>, logical_len: u64) -> EncodedNode {
    EncodedNode {
        bytes,
        logical_len,
        descendants: BTreeMap::new(),
    }
}

fn canonical_float_bits(value: f64) -> u64 {
    if value == 0.0 {
        0
    } else if value.is_nan() {
        0x7ff8_0000_0000_0000
    } else {
        value.to_bits()
    }
}

fn validate_bigint(value: &str) -> Result<(), CanonicalEncodeError> {
    let digits = value.strip_prefix('-').unwrap_or(value);
    let valid_sign = !value.starts_with('+') && value != "-0";
    let valid_digits = !digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit());
    let minimal_zero = digits == "0" || !digits.starts_with('0');
    if valid_sign && valid_digits && minimal_zero {
        Ok(())
    } else {
        Err(CanonicalEncodeError::InvalidBigInt(value.to_string()))
    }
}

fn validate_definition_key(value: &str) -> Result<(), CanonicalEncodeError> {
    if value.is_empty() {
        Err(CanonicalEncodeError::EmptyDefinitionKey)
    } else {
        Ok(())
    }
}

fn reject_duplicate_names<'a>(
    names: impl IntoIterator<Item = &'a str>,
    error: impl Fn(String) -> CanonicalEncodeError,
) -> Result<(), CanonicalEncodeError> {
    let mut previous = None;
    for name in names {
        if previous == Some(name) {
            return Err(error(name.to_string()));
        }
        previous = Some(name);
    }
    Ok(())
}

fn sum_value_logical_len(values: &[CanonicalValue]) -> Result<u64, CanonicalEncodeError> {
    values.iter().try_fold(0_u64, |total, value| {
        total
            .checked_add(encode_node(value)?.logical_len)
            .ok_or(CanonicalEncodeError::LengthOverflow)
    })
}

fn put_u32(output: &mut Vec<u8>, value: usize) -> Result<(), CanonicalEncodeError> {
    output.extend_from_slice(
        &u32::try_from(value)
            .map_err(|_| CanonicalEncodeError::LengthOverflow)?
            .to_le_bytes(),
    );
    Ok(())
}

fn put_bytes(output: &mut Vec<u8>, value: &[u8]) -> Result<(), CanonicalEncodeError> {
    put_u32(output, value.len())?;
    output.extend_from_slice(value);
    Ok(())
}

fn usize_to_u64(value: usize) -> Result<u64, CanonicalEncodeError> {
    u64::try_from(value).map_err(|_| CanonicalEncodeError::LengthOverflow)
}

fn checked_sum(values: impl IntoIterator<Item = u64>) -> Result<u64, CanonicalEncodeError> {
    values.into_iter().try_fold(0_u64, |total, value| {
        total
            .checked_add(value)
            .ok_or(CanonicalEncodeError::LengthOverflow)
    })
}

/// Parse a canonical node strictly and return every directly referenced CID,
/// including references nested inside inline children.
pub fn referenced_cids(bytes: &[u8]) -> Result<Vec<Cid>, CanonicalEncodeError> {
    let mut parser = Parser {
        bytes,
        offset: 0,
        references: BTreeSet::new(),
    };
    parser.node()?;
    if parser.offset != bytes.len() {
        return Err(CanonicalEncodeError::TrailingNodeBytes);
    }
    Ok(parser.references.into_iter().collect())
}

struct Parser<'a> {
    bytes: &'a [u8],
    offset: usize,
    references: BTreeSet<Cid>,
}

impl Parser<'_> {
    fn node(&mut self) -> Result<(), CanonicalEncodeError> {
        let tag = self.byte()?;
        match tag {
            TAG_NULL | TAG_FALSE | TAG_TRUE => {}
            TAG_INT | TAG_FLOAT => self.skip(8)?,
            TAG_BIGINT | TAG_STRING | TAG_BYTES => {
                self.length_prefixed()?;
            }
            TAG_STRING_BRANCH | TAG_BYTES_BRANCH | TAG_LIST_BRANCH | TAG_MAP_BRANCH => {
                self.skip(8)?;
                self.cid_array()?;
            }
            TAG_LIST_LEAF => {
                let count = self.u32()?;
                for _ in 0..count {
                    self.child()?;
                }
            }
            TAG_MAP_LEAF => {
                let count = self.u32()?;
                for _ in 0..count {
                    self.length_prefixed()?;
                    self.child()?;
                }
            }
            TAG_CLASS_LEAF => {
                self.length_prefixed()?;
                let count = self.u32()?;
                for _ in 0..count {
                    self.length_prefixed()?;
                    match self.byte()? {
                        0 => {}
                        1 | 2 => self.child()?,
                        _ => {
                            return Err(CanonicalEncodeError::InvalidNode(
                                "unknown class field presence",
                            ));
                        }
                    }
                }
            }
            TAG_CLASS_BRANCH => {
                self.length_prefixed()?;
                self.skip(8)?;
                self.cid_array()?;
            }
            TAG_ENUM => {
                self.length_prefixed()?;
                self.length_prefixed()?;
            }
            TAG_MEDIA => {
                self.length_prefixed()?;
                match self.byte()? {
                    0 => {}
                    1 => self.length_prefixed()?,
                    _ => {
                        return Err(CanonicalEncodeError::InvalidNode(
                            "unknown media mime presence",
                        ));
                    }
                }
                if self.byte()? > 3 {
                    return Err(CanonicalEncodeError::InvalidNode(
                        "unknown media content kind",
                    ));
                }
                self.length_prefixed()?;
            }
            TAG_OMISSION => {
                self.length_prefixed()?;
                self.length_prefixed()?;
            }
            _ => return Err(CanonicalEncodeError::InvalidNode("unknown node tag")),
        }
        Ok(())
    }

    fn child(&mut self) -> Result<(), CanonicalEncodeError> {
        match self.byte()? {
            CHILD_INLINE => {
                let length = self.u32()? as usize;
                let child = self.take(length)?;
                let references = referenced_cids(child)?;
                self.references.extend(references);
                Ok(())
            }
            CHILD_CID => {
                let cid = Cid::from_bytes(
                    self.take(32)?
                        .try_into()
                        .map_err(|_| CanonicalEncodeError::InvalidNode("short CID"))?,
                );
                self.references.insert(cid);
                Ok(())
            }
            _ => Err(CanonicalEncodeError::InvalidNode("unknown child tag")),
        }
    }

    fn cid_array(&mut self) -> Result<(), CanonicalEncodeError> {
        let count = self.u32()?;
        for _ in 0..count {
            let cid = Cid::from_bytes(
                self.take(32)?
                    .try_into()
                    .map_err(|_| CanonicalEncodeError::InvalidNode("short CID"))?,
            );
            self.references.insert(cid);
        }
        Ok(())
    }

    fn length_prefixed(&mut self) -> Result<(), CanonicalEncodeError> {
        let length = self.u32()? as usize;
        self.skip(length)
    }

    fn u32(&mut self) -> Result<u32, CanonicalEncodeError> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().map_err(
            |_| CanonicalEncodeError::InvalidNode("short u32"),
        )?))
    }

    fn byte(&mut self) -> Result<u8, CanonicalEncodeError> {
        let byte = *self
            .bytes
            .get(self.offset)
            .ok_or(CanonicalEncodeError::InvalidNode("unexpected end"))?;
        self.offset += 1;
        Ok(byte)
    }

    fn skip(&mut self, length: usize) -> Result<(), CanonicalEncodeError> {
        self.take(length).map(|_| ())
    }

    fn take(&mut self, length: usize) -> Result<&[u8], CanonicalEncodeError> {
        let end = self
            .offset
            .checked_add(length)
            .ok_or(CanonicalEncodeError::InvalidNode("length overflow"))?;
        let bytes = self
            .bytes
            .get(self.offset..end)
            .ok_or(CanonicalEncodeError::InvalidNode("unexpected end"))?;
        self.offset = end;
        Ok(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::{CanonicalField, CanonicalValue, FieldPresence, encode_value_dag, referenced_cids};

    fn root_bytes(value: &CanonicalValue) -> Vec<u8> {
        let dag = encode_value_dag(value).unwrap();
        dag.chunks
            .iter()
            .find(|chunk| chunk.cid == dag.root)
            .unwrap()
            .canonical_bytes
            .clone()
    }

    #[test]
    fn map_order_and_float_normalization_are_canonical() {
        let first = CanonicalValue::Map(vec![
            ("z".to_string(), CanonicalValue::Float(-0.0)),
            ("a".to_string(), CanonicalValue::Float(f64::NAN)),
        ]);
        let second = CanonicalValue::Map(vec![
            (
                "a".to_string(),
                CanonicalValue::Float(f64::from_bits(0x7ff0_0000_0000_0001)),
            ),
            ("z".to_string(), CanonicalValue::Float(0.0)),
        ]);
        assert_eq!(
            encode_value_dag(&first).unwrap().root,
            encode_value_dag(&second).unwrap().root
        );
    }

    #[test]
    fn absent_null_and_default_filled_are_distinct() {
        let class = |presence| CanonicalValue::Class {
            definition_key: "class:user.Profile".to_string(),
            fields: vec![CanonicalField {
                name: "nickname".to_string(),
                presence,
            }],
        };
        let absent = encode_value_dag(&class(FieldPresence::Absent))
            .unwrap()
            .root;
        let null = encode_value_dag(&class(FieldPresence::Present(CanonicalValue::Null)))
            .unwrap()
            .root;
        let default = encode_value_dag(&class(FieldPresence::DefaultFilled(CanonicalValue::Null)))
            .unwrap()
            .root;
        assert_ne!(absent, null);
        assert_ne!(absent, default);
        assert_ne!(null, default);
    }

    #[test]
    fn schema_definition_key_is_part_of_identity() {
        let value = |definition_key: &str| CanonicalValue::Enum {
            definition_key: definition_key.to_string(),
            variant: "Ready".to_string(),
        };
        assert_ne!(
            encode_value_dag(&value("enum:user.State")).unwrap().root,
            encode_value_dag(&value("enum:user.RenamedState"))
                .unwrap()
                .root
        );
    }

    #[test]
    fn long_bytes_have_fixed_chunks_and_parseable_references() {
        let mut bytes = vec![7; super::BYTE_CHUNK_LEN * 2 + 1];
        bytes[super::BYTE_CHUNK_LEN..super::BYTE_CHUNK_LEN * 2].fill(8);
        bytes[super::BYTE_CHUNK_LEN * 2] = 9;
        let value = CanonicalValue::Bytes(bytes);
        let dag = encode_value_dag(&value).unwrap();
        assert_eq!(dag.chunks.len(), 4);
        let references = referenced_cids(&root_bytes(&value)).unwrap();
        assert_eq!(references.len(), 3);
        assert!(
            references
                .iter()
                .all(|cid| dag.chunks.iter().any(|chunk| chunk.cid == *cid))
        );
    }

    #[test]
    fn strict_parser_rejects_trailing_and_unknown_tags() {
        assert!(referenced_cids(&[0x00, 0x00]).is_err());
        assert!(referenced_cids(&[0xff]).is_err());
    }

    #[test]
    fn repeated_64k_transcript_content_clears_twenty_x_dedup_gate() {
        let prompt = "p".repeat(64 * 1024);
        let transcript = CanonicalValue::List(
            (0..64)
                .map(|sequence| {
                    CanonicalValue::Map(vec![
                        (
                            "content".to_string(),
                            CanonicalValue::String(prompt.clone()),
                        ),
                        ("sequence".to_string(), CanonicalValue::Int(sequence)),
                    ])
                })
                .collect(),
        );
        let dag = encode_value_dag(&transcript).unwrap();
        let stored_bytes = dag
            .chunks
            .iter()
            .map(|chunk| chunk.canonical_bytes.len())
            .sum::<usize>();
        let repeated_payload_bytes = prompt.len() * 64;
        assert!(
            stored_bytes <= repeated_payload_bytes / 20,
            "{stored_bytes} stored bytes did not clear 20x gate for {repeated_payload_bytes} input bytes"
        );
    }

    #[test]
    fn golden_v1_scalar_and_schema_nodes() {
        let null = encode_value_dag(&CanonicalValue::Null).unwrap();
        assert_eq!(root_bytes(&CanonicalValue::Null), vec![0x00]);
        assert_eq!(
            null.root.to_hex(),
            "7f07df906471581c362ba1a3101fcb9ee0b49b07bac845b24d2bbc31f11f197f"
        );

        let value = CanonicalValue::Class {
            definition_key: "class:user.Person".to_string(),
            fields: vec![
                CanonicalField {
                    name: "age".to_string(),
                    presence: FieldPresence::Present(CanonicalValue::Int(42)),
                },
                CanonicalField {
                    name: "nickname".to_string(),
                    presence: FieldPresence::Absent,
                },
            ],
        };
        assert_eq!(
            encode_value_dag(&value).unwrap().root.to_hex(),
            include_str!("../../tests/fixtures/obs/v1/canonical_class_person.cid").trim()
        );
        assert_eq!(
            hex(&root_bytes(&value)),
            include_str!("../../tests/fixtures/obs/v1/canonical_class_person.hex").trim()
        );
    }

    fn hex(bytes: &[u8]) -> String {
        use std::fmt::Write as _;

        bytes.iter().fold(
            String::with_capacity(bytes.len().saturating_mul(2)),
            |mut output, byte| {
                write!(&mut output, "{byte:02x}").expect("writing to String cannot fail");
                output
            },
        )
    }
}
