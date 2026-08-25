//! CAS codec 2 — `FunctionTableV1` (streams spec §4.6).
//!
//! Published once per engine at profiler activation; identical programs
//! dedupe by CID. Function kind/origin are codec-level codes so this leaf
//! crate never depends on VM metadata types; 255 (and any unlisted value)
//! decodes to `None`.

use crate::ids::FunctionId;

const BODY_VERSION: u16 = 1;
const MAX_ENCODED_STRING: usize = 64 * 1024;
const CODE_UNKNOWN: u8 = 255;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FunctionKindCode {
    Bytecode,
    SysOp,
    Native,
    NativeUnresolved,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FunctionOriginCode {
    UserDefined,
    Companion,
    Internal,
    Builtin,
    AutoDerive,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FunctionSourceSpan {
    pub file_id: u32,
    pub start: u32,
    pub end: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FunctionTableEntry {
    pub function_id: FunctionId,
    pub fqn: String,
    pub display_name: String,
    pub definition_key: Option<String>,
    pub kind: Option<FunctionKindCode>,
    /// Sysop name for `SysOp` entries.
    pub kind_detail: Option<String>,
    pub origin: Option<FunctionOriginCode>,
    pub source_file: Option<String>,
    pub source_span: Option<FunctionSourceSpan>,
    pub package_name: Option<String>,
    pub namespace: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FunctionTableFile {
    pub file_id: u32,
    pub path: String,
}

/// Durable per-engine function/file tables. Functions sorted by
/// `function_id` ascending, files by `file_id` ascending; encoding is
/// deterministic (identical tables ⇒ identical bytes on every platform).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FunctionTable {
    pub functions: Vec<FunctionTableEntry>,
    pub files: Vec<FunctionTableFile>,
}

impl FunctionTable {
    #[must_use]
    pub fn function(&self, id: FunctionId) -> Option<&FunctionTableEntry> {
        self.functions
            .binary_search_by_key(&id.0, |entry| entry.function_id.0)
            .ok()
            .map(|index| &self.functions[index])
    }

    #[must_use]
    pub fn file_path(&self, file_id: u32) -> Option<&str> {
        self.files
            .binary_search_by_key(&file_id, |file| file.file_id)
            .ok()
            .map(|index| self.files[index].path.as_str())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FunctionTableError {
    StringTooLong,
    Truncated,
    UnsupportedVersion(u16),
    InvalidTag,
    InvalidUtf8,
    Unordered,
    TrailingBytes,
}

pub fn encode_function_table(table: &FunctionTable) -> Result<Vec<u8>, FunctionTableError> {
    debug_assert!(
        table
            .functions
            .windows(2)
            .all(|pair| pair[0].function_id.0 < pair[1].function_id.0),
        "functions must be sorted by function_id"
    );
    debug_assert!(
        table
            .files
            .windows(2)
            .all(|pair| pair[0].file_id < pair[1].file_id),
        "files must be sorted by file_id"
    );
    let mut body = Vec::with_capacity(64 + table.functions.len() * 128);
    body.extend_from_slice(&BODY_VERSION.to_be_bytes());
    body.extend_from_slice(
        &u32::try_from(table.functions.len())
            .map_err(|_| FunctionTableError::Truncated)?
            .to_be_bytes(),
    );
    for function in &table.functions {
        body.extend_from_slice(&function.function_id.0.to_be_bytes());
        encode_string(&mut body, &function.fqn)?;
        encode_string(&mut body, &function.display_name)?;
        encode_optional_string(&mut body, function.definition_key.as_deref())?;
        body.push(function.kind.map_or(CODE_UNKNOWN, |kind| kind as u8));
        encode_optional_string(&mut body, function.kind_detail.as_deref())?;
        body.push(function.origin.map_or(CODE_UNKNOWN, |origin| origin as u8));
        encode_optional_string(&mut body, function.source_file.as_deref())?;
        match function.source_span {
            None => body.push(0),
            Some(span) => {
                body.push(1);
                body.extend_from_slice(&span.file_id.to_be_bytes());
                body.extend_from_slice(&span.start.to_be_bytes());
                body.extend_from_slice(&span.end.to_be_bytes());
            }
        }
        encode_optional_string(&mut body, function.package_name.as_deref())?;
        body.extend_from_slice(
            &u16::try_from(function.namespace.len())
                .map_err(|_| FunctionTableError::Truncated)?
                .to_be_bytes(),
        );
        for part in &function.namespace {
            encode_string(&mut body, part)?;
        }
    }
    body.extend_from_slice(
        &u32::try_from(table.files.len())
            .map_err(|_| FunctionTableError::Truncated)?
            .to_be_bytes(),
    );
    for file in &table.files {
        body.extend_from_slice(&file.file_id.to_be_bytes());
        encode_string(&mut body, &file.path)?;
    }
    Ok(body)
}

pub fn decode_function_table(bytes: &[u8]) -> Result<FunctionTable, FunctionTableError> {
    let mut cursor = Cursor::new(bytes);
    let version = cursor.u16()?;
    if version != BODY_VERSION {
        return Err(FunctionTableError::UnsupportedVersion(version));
    }
    let function_count = cursor.u32()? as usize;
    let mut functions = Vec::with_capacity(function_count.min(64 * 1024));
    for _ in 0..function_count {
        let function_id = FunctionId(cursor.u32()?);
        if functions
            .last()
            .is_some_and(|previous: &FunctionTableEntry| previous.function_id.0 >= function_id.0)
        {
            return Err(FunctionTableError::Unordered);
        }
        let fqn = cursor.string()?;
        let display_name = cursor.string()?;
        let definition_key = cursor.optional_string()?;
        let kind = match cursor.u8()? {
            0 => Some(FunctionKindCode::Bytecode),
            1 => Some(FunctionKindCode::SysOp),
            2 => Some(FunctionKindCode::Native),
            3 => Some(FunctionKindCode::NativeUnresolved),
            _ => None,
        };
        let kind_detail = cursor.optional_string()?;
        let origin = match cursor.u8()? {
            0 => Some(FunctionOriginCode::UserDefined),
            1 => Some(FunctionOriginCode::Companion),
            2 => Some(FunctionOriginCode::Internal),
            3 => Some(FunctionOriginCode::Builtin),
            4 => Some(FunctionOriginCode::AutoDerive),
            _ => None,
        };
        let source_file = cursor.optional_string()?;
        let source_span = match cursor.u8()? {
            0 => None,
            1 => Some(FunctionSourceSpan {
                file_id: cursor.u32()?,
                start: cursor.u32()?,
                end: cursor.u32()?,
            }),
            _ => return Err(FunctionTableError::InvalidTag),
        };
        let package_name = cursor.optional_string()?;
        let namespace_count = cursor.u16()? as usize;
        let mut namespace = Vec::with_capacity(namespace_count.min(64));
        for _ in 0..namespace_count {
            namespace.push(cursor.string()?);
        }
        functions.push(FunctionTableEntry {
            function_id,
            fqn,
            display_name,
            definition_key,
            kind,
            kind_detail,
            origin,
            source_file,
            source_span,
            package_name,
            namespace,
        });
    }
    let file_count = cursor.u32()? as usize;
    let mut files = Vec::with_capacity(file_count.min(64 * 1024));
    for _ in 0..file_count {
        let file_id = cursor.u32()?;
        if files
            .last()
            .is_some_and(|previous: &FunctionTableFile| previous.file_id >= file_id)
        {
            return Err(FunctionTableError::Unordered);
        }
        files.push(FunctionTableFile {
            file_id,
            path: cursor.string()?,
        });
    }
    if !cursor.is_empty() {
        return Err(FunctionTableError::TrailingBytes);
    }
    Ok(FunctionTable { functions, files })
}

fn encode_string(output: &mut Vec<u8>, value: &str) -> Result<(), FunctionTableError> {
    if value.len() > MAX_ENCODED_STRING {
        return Err(FunctionTableError::StringTooLong);
    }
    output.extend_from_slice(
        &u32::try_from(value.len())
            .map_err(|_| FunctionTableError::StringTooLong)?
            .to_be_bytes(),
    );
    output.extend_from_slice(value.as_bytes());
    Ok(())
}

fn encode_optional_string(
    output: &mut Vec<u8>,
    value: Option<&str>,
) -> Result<(), FunctionTableError> {
    match value {
        None => output.push(0),
        Some(value) => {
            output.push(1);
            encode_string(output, value)?;
        }
    }
    Ok(())
}

struct Cursor<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Cursor<'a> {
    const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn take(&mut self, count: usize) -> Result<&'a [u8], FunctionTableError> {
        let end = self
            .offset
            .checked_add(count)
            .ok_or(FunctionTableError::Truncated)?;
        let slice = self
            .bytes
            .get(self.offset..end)
            .ok_or(FunctionTableError::Truncated)?;
        self.offset = end;
        Ok(slice)
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N], FunctionTableError> {
        self.take(N)?
            .try_into()
            .map_err(|_| FunctionTableError::Truncated)
    }

    fn u8(&mut self) -> Result<u8, FunctionTableError> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> Result<u16, FunctionTableError> {
        Ok(u16::from_be_bytes(self.array()?))
    }

    fn u32(&mut self) -> Result<u32, FunctionTableError> {
        Ok(u32::from_be_bytes(self.array()?))
    }

    fn string(&mut self) -> Result<String, FunctionTableError> {
        let length = usize::try_from(self.u32()?).map_err(|_| FunctionTableError::Truncated)?;
        let value =
            std::str::from_utf8(self.take(length)?).map_err(|_| FunctionTableError::InvalidUtf8)?;
        Ok(value.to_owned())
    }

    fn optional_string(&mut self) -> Result<Option<String>, FunctionTableError> {
        match self.u8()? {
            0 => Ok(None),
            1 => self.string().map(Some),
            _ => Err(FunctionTableError::InvalidTag),
        }
    }

    fn is_empty(&self) -> bool {
        self.offset == self.bytes.len()
    }
}

#[cfg(test)]
mod tests {
    use sha2::{Digest, Sha256};

    use super::*;

    fn fixture() -> FunctionTable {
        FunctionTable {
            functions: vec![
                FunctionTableEntry {
                    function_id: FunctionId(1),
                    fqn: "app.Score".to_string(),
                    display_name: "Score".to_string(),
                    definition_key: Some("function:app.Score".to_string()),
                    kind: Some(FunctionKindCode::Bytecode),
                    kind_detail: None,
                    origin: Some(FunctionOriginCode::UserDefined),
                    source_file: Some("app.baml".to_string()),
                    source_span: Some(FunctionSourceSpan {
                        file_id: 3,
                        start: 10,
                        end: 90,
                    }),
                    package_name: Some("app".to_string()),
                    namespace: vec!["inner".to_string()],
                },
                FunctionTableEntry {
                    function_id: FunctionId(2),
                    fqn: "baml.sys.print".to_string(),
                    display_name: "print".to_string(),
                    definition_key: None,
                    kind: Some(FunctionKindCode::SysOp),
                    kind_detail: Some("print".to_string()),
                    origin: Some(FunctionOriginCode::Builtin),
                    source_file: None,
                    source_span: None,
                    package_name: None,
                    namespace: Vec::new(),
                },
            ],
            files: vec![FunctionTableFile {
                file_id: 3,
                path: "app.baml".to_string(),
            }],
        }
    }

    #[test]
    fn function_table_round_trips_with_truncation_and_trailing_checks() {
        let table = fixture();
        let encoded = encode_function_table(&table).unwrap();
        assert_eq!(decode_function_table(&encoded), Ok(table));
        for cut in 0..encoded.len() {
            assert!(decode_function_table(&encoded[..cut]).is_err(), "cut {cut}");
        }
        let mut trailing = encoded;
        trailing.push(0);
        assert_eq!(
            decode_function_table(&trailing),
            Err(FunctionTableError::TrailingBytes)
        );
    }

    #[test]
    fn unknown_kind_and_origin_codes_decode_to_none() {
        let mut table = fixture();
        table.functions[0].kind = None;
        table.functions[0].origin = None;
        let encoded = encode_function_table(&table).unwrap();
        let decoded = decode_function_table(&encoded).unwrap();
        assert_eq!(decoded.functions[0].kind, None);
        assert_eq!(decoded.functions[0].origin, None);
    }

    #[test]
    fn function_table_golden_checksum_is_cross_platform() {
        let encoded = encode_function_table(&fixture()).unwrap();
        assert_eq!(
            hex::encode(Sha256::digest(&encoded)),
            "2df672d2dcdd5170932c4e167ae9f4608a6ae6015172231406b3fc7f2084912e"
        );
    }
}
