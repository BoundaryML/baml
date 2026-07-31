//! Native atomic `.bamldict` persistence.
//!
//! Dictionaries are immutable and addressed by their caller-supplied
//! [`RevisionId`]. The writer encodes to one temporary file, syncs it, then
//! publishes it with an atomic no-clobber link. A concurrent winner therefore
//! turns every loser into a cheap idempotent no-op; a complete dictionary is
//! always visible at the final path.

use std::{
    error::Error,
    fmt,
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
};

use prost::Message as _;

use super::{
    DICTIONARY_FORMAT_VERSION, DictionaryValidationError, FileRow, FunctionDictRow, FunctionKind,
    FunctionOrigin, LambdaIdentity, LambdaKind, ProgramIdentity, RevisionDictionary, RevisionId,
    SemanticLanes, SourceSnapshotId, SourceSpan, pb,
};

const DICTIONARY_DIR: &str = ".baml/dict";
const DICTIONARY_EXTENSION: &str = "bamldict";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RevisionDictionaryStore {
    dictionary_dir: PathBuf,
}

impl RevisionDictionaryStore {
    /// Locates dictionaries under `<project_root>/.baml/dict`.
    #[must_use]
    pub fn new(project_root: impl AsRef<Path>) -> Self {
        Self {
            dictionary_dir: project_root.as_ref().join(DICTIONARY_DIR),
        }
    }

    /// Uses an already-resolved dictionary directory.
    #[must_use]
    pub fn at_dictionary_dir(dictionary_dir: impl AsRef<Path>) -> Self {
        Self {
            dictionary_dir: dictionary_dir.as_ref().to_path_buf(),
        }
    }

    #[must_use]
    pub fn dictionary_dir(&self) -> &Path {
        &self.dictionary_dir
    }

    #[must_use]
    pub fn path_for(&self, revision_id: RevisionId) -> PathBuf {
        self.dictionary_dir.join(dictionary_file_name(revision_id))
    }

    /// Writes the dictionary once and returns `AlreadyExists` when this
    /// revision has already been published.
    pub fn ensure_written(
        &self,
        dictionary: &RevisionDictionary,
    ) -> Result<DictionaryWriteOutcome, DictionaryWriteError> {
        dictionary
            .validate()
            .map_err(DictionaryWriteError::InvalidDictionary)?;

        let path = self.path_for(dictionary.identity.revision_id);
        if path.is_file() {
            return Ok(DictionaryWriteOutcome {
                path,
                disposition: DictionaryWriteDisposition::AlreadyExists,
            });
        }

        let encoded =
            encode_dictionary(dictionary).map_err(DictionaryWriteError::InvalidDictionary)?;
        fs::create_dir_all(&self.dictionary_dir).map_err(DictionaryWriteError::Io)?;

        // UUID keeps independent processes from sharing a temporary path.
        // The temp lives beside the target so publication cannot cross a
        // filesystem boundary.
        let tmp_path = self.dictionary_dir.join(format!(
            ".{}.{}.{}.tmp",
            dictionary.identity.revision_id,
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        let mut tmp_guard = TempPath::new(tmp_path);
        {
            let mut tmp = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(tmp_guard.path())
                .map_err(DictionaryWriteError::Io)?;
            tmp.write_all(&encoded).map_err(DictionaryWriteError::Io)?;
            tmp.sync_all().map_err(DictionaryWriteError::Io)?;
        }

        // hard_link is the portable no-clobber publication primitive: the
        // final name appears atomically and AlreadyExists identifies a race
        // winner without replacing its inode. Both names point at the fully
        // synced temp inode until the private temp name is removed.
        let disposition = match fs::hard_link(tmp_guard.path(), &path) {
            Ok(()) => {
                tmp_guard.remove().map_err(DictionaryWriteError::Io)?;
                sync_dictionary_dir(&self.dictionary_dir).map_err(DictionaryWriteError::Io)?;
                DictionaryWriteDisposition::Written
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                tmp_guard.remove().map_err(DictionaryWriteError::Io)?;
                DictionaryWriteDisposition::AlreadyExists
            }
            Err(error) => return Err(DictionaryWriteError::Io(error)),
        };

        Ok(DictionaryWriteOutcome { path, disposition })
    }

    /// Reads and validates the content-addressed dictionary for `revision_id`.
    pub fn read(&self, revision_id: RevisionId) -> Result<RevisionDictionary, DictionaryReadError> {
        let path = self.path_for(revision_id);
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Err(DictionaryReadError::DictionaryMissing { revision_id });
            }
            Err(error) => return Err(DictionaryReadError::Io(error)),
        };
        let dictionary = decode_dictionary(&bytes).map_err(DictionaryReadError::InvalidData)?;
        if dictionary.identity.revision_id != revision_id {
            return Err(DictionaryReadError::InvalidData(invalid_data(format!(
                "dictionary path names revision {revision_id}, header names {}",
                dictionary.identity.revision_id
            ))));
        }
        Ok(dictionary)
    }
}

#[must_use]
pub fn dictionary_file_name(revision_id: RevisionId) -> String {
    format!("{revision_id}.{DICTIONARY_EXTENSION}")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DictionaryWriteDisposition {
    Written,
    AlreadyExists,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DictionaryWriteOutcome {
    pub path: PathBuf,
    pub disposition: DictionaryWriteDisposition,
}

#[derive(Debug)]
pub enum DictionaryWriteError {
    InvalidDictionary(DictionaryValidationError),
    Io(io::Error),
}

impl fmt::Display for DictionaryWriteError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidDictionary(error) => write!(formatter, "invalid dictionary: {error}"),
            Self::Io(error) => write!(formatter, "dictionary write failed: {error}"),
        }
    }
}

impl Error for DictionaryWriteError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidDictionary(error) => Some(error),
            Self::Io(error) => Some(error),
        }
    }
}

#[derive(Debug)]
pub enum DictionaryReadError {
    DictionaryMissing { revision_id: RevisionId },
    InvalidData(io::Error),
    Io(io::Error),
}

impl fmt::Display for DictionaryReadError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DictionaryMissing { revision_id } => {
                write!(formatter, "dictionary missing for revision {revision_id}")
            }
            Self::InvalidData(error) => write!(formatter, "invalid dictionary: {error}"),
            Self::Io(error) => write!(formatter, "dictionary read failed: {error}"),
        }
    }
}

impl Error for DictionaryReadError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::DictionaryMissing { .. } => None,
            Self::InvalidData(error) | Self::Io(error) => Some(error),
        }
    }
}

/// Encodes the four V1 sections. This is public for byte sinks and fixtures;
/// native callers normally use [`RevisionDictionaryStore::ensure_written`].
pub fn encode_dictionary(
    dictionary: &RevisionDictionary,
) -> Result<Vec<u8>, DictionaryValidationError> {
    dictionary.validate()?;

    let header = pb::RevisionDictionaryHeaderV1 {
        format_version: DICTIONARY_FORMAT_VERSION,
        revision_id: dictionary.identity.revision_id.0.to_vec(),
        source_snapshot_id: dictionary.identity.source_snapshot_id.0.to_vec(),
        compiler_id: dictionary.identity.compiler_id.clone(),
        function_count: dictionary.identity.function_count,
        capture_policy_version: dictionary.capture_policy_version,
        file_count: saturating_u32(dictionary.files.len()),
        dictionary_function_count: saturating_u32(dictionary.functions.len()),
        call_site_count: saturating_u32(dictionary.call_sites.len()),
    };
    let files = pb::FileTableV1 {
        rows: dictionary.files.iter().map(file_to_proto).collect(),
    };
    let functions = pb::FunctionTableV1 {
        rows: dictionary.functions.iter().map(function_to_proto).collect(),
    };
    let call_sites = pb::CallSiteTableV1 {
        rows: dictionary
            .call_sites
            .iter()
            .copied()
            .map(|row| pb::CallSiteRowV1 {
                call_site_id: row.call_site_id,
                caller_function_id: row.caller_function_id,
                callee_function_id: row.callee_function_id,
                file_id: row.file_id,
                span_start: row.span_start,
                span_end: row.span_end,
                line: row.line,
            })
            .collect(),
    };

    let sections = [
        pb::revision_dictionary_v1::Section::Header(header),
        pb::revision_dictionary_v1::Section::Files(files),
        pb::revision_dictionary_v1::Section::Functions(functions),
        pb::revision_dictionary_v1::Section::CallSites(call_sites),
    ];
    let mut output = Vec::new();
    for section in sections {
        pb::RevisionDictionaryV1 {
            section: Some(section),
        }
        .encode_length_delimited(&mut output)
        .expect("encoding a protobuf message into Vec cannot fail");
    }
    Ok(output)
}

/// Decodes all known V1 sections. Records containing only unknown future
/// sections are ignored; protobuf also skips unknown fields within known
/// sections.
pub fn decode_dictionary(bytes: &[u8]) -> io::Result<RevisionDictionary> {
    let mut input = bytes;
    let mut header = None;
    let mut files = None;
    let mut functions = None;
    let mut call_sites = None;

    while !input.is_empty() {
        let message = pb::RevisionDictionaryV1::decode_length_delimited(&mut input)
            .map_err(|error| invalid_data(format!("invalid length-delimited section: {error}")))?;
        match message.section {
            Some(pb::revision_dictionary_v1::Section::Header(value)) => {
                set_once(&mut header, value, "header")?;
            }
            Some(pb::revision_dictionary_v1::Section::Files(value)) => {
                set_once(&mut files, value, "files")?;
            }
            Some(pb::revision_dictionary_v1::Section::Functions(value)) => {
                set_once(&mut functions, value, "functions")?;
            }
            Some(pb::revision_dictionary_v1::Section::CallSites(value)) => {
                set_once(&mut call_sites, value, "call_sites")?;
            }
            // An additive future section is unknown to prost and decodes as
            // an empty oneof. Its length-delimited record has already been
            // consumed, so the V1 reader can safely continue.
            None => {}
        }
    }

    let header = header.ok_or_else(|| invalid_data("dictionary omitted header section"))?;
    if header.format_version != DICTIONARY_FORMAT_VERSION {
        return Err(invalid_data(format!(
            "unsupported dictionary format version {}; expected {}",
            header.format_version, DICTIONARY_FORMAT_VERSION
        )));
    }
    let files = files
        .ok_or_else(|| invalid_data("dictionary omitted files section"))?
        .rows
        .into_iter()
        .map(file_from_proto)
        .collect::<io::Result<Vec<_>>>()?;
    let functions = functions
        .ok_or_else(|| invalid_data("dictionary omitted functions section"))?
        .rows
        .into_iter()
        .map(function_from_proto)
        .collect::<io::Result<Vec<_>>>()?;
    let call_sites = call_sites
        .ok_or_else(|| invalid_data("dictionary omitted call_sites section"))?
        .rows
        .into_iter()
        .map(|row| super::CallSiteRow {
            call_site_id: row.call_site_id,
            caller_function_id: row.caller_function_id,
            callee_function_id: row.callee_function_id,
            file_id: row.file_id,
            span_start: row.span_start,
            span_end: row.span_end,
            line: row.line,
        })
        .collect::<Vec<_>>();

    check_count("file", header.file_count, files.len())?;
    check_count(
        "dictionary function",
        header.dictionary_function_count,
        functions.len(),
    )?;
    check_count("call-site", header.call_site_count, call_sites.len())?;

    let dictionary = RevisionDictionary {
        identity: ProgramIdentity {
            revision_id: RevisionId(fixed_hash("revision_id", header.revision_id)?),
            source_snapshot_id: SourceSnapshotId(fixed_hash(
                "source_snapshot_id",
                header.source_snapshot_id,
            )?),
            compiler_id: header.compiler_id,
            function_count: header.function_count,
        },
        capture_policy_version: header.capture_policy_version,
        files,
        functions,
        call_sites,
    };
    dictionary
        .validate()
        .map_err(|error| invalid_data(error.to_string()))?;
    Ok(dictionary)
}

fn set_once<T>(slot: &mut Option<T>, value: T, name: &str) -> io::Result<()> {
    if slot.replace(value).is_some() {
        return Err(invalid_data(format!("dictionary repeated {name} section")));
    }
    Ok(())
}

fn check_count(name: &str, expected: u32, actual: usize) -> io::Result<()> {
    if usize::try_from(expected).ok() == Some(actual) {
        Ok(())
    } else {
        Err(invalid_data(format!(
            "{name} count mismatch: header says {expected}, section has {actual}"
        )))
    }
}

fn fixed_hash(name: &str, bytes: Vec<u8>) -> io::Result<[u8; 32]> {
    let actual = bytes.len();
    bytes
        .try_into()
        .map_err(|_| invalid_data(format!("{name} must be exactly 32 bytes, got {actual}")))
}

fn optional_hash(name: &str, bytes: Option<Vec<u8>>) -> io::Result<Option<[u8; 32]>> {
    bytes.map(|bytes| fixed_hash(name, bytes)).transpose()
}

fn file_to_proto(row: &FileRow) -> pb::FileRowV1 {
    pb::FileRowV1 {
        file_id: row.file_id,
        path: row.path.clone(),
        content_hash: row.content_hash.to_vec(),
    }
}

fn file_from_proto(row: pb::FileRowV1) -> io::Result<FileRow> {
    Ok(FileRow {
        file_id: row.file_id,
        path: row.path,
        content_hash: fixed_hash("file content_hash", row.content_hash)?,
    })
}

fn function_to_proto(row: &FunctionDictRow) -> pb::FunctionDictRowV1 {
    let (kind, sys_op_name) = match &row.kind {
        FunctionKind::Bytecode => (pb::FunctionKindV1::FunctionKindBytecode as i32, None),
        FunctionKind::SysOp(name) => (
            pb::FunctionKindV1::FunctionKindSysOp as i32,
            Some(name.clone()),
        ),
        FunctionKind::Native => (pb::FunctionKindV1::FunctionKindNative as i32, None),
    };
    pb::FunctionDictRowV1 {
        function_id: row.function_id,
        fqn: row.fqn.clone(),
        display_name: row.display_name.clone(),
        declared_name: row.declared_name.clone(),
        source_span: row.source_span.map(|span| pb::SourceSpanV1 {
            file_id: span.file_id,
            span_start: span.span_start,
            span_end: span.span_end,
            line: span.line,
        }),
        kind,
        sys_op_name,
        origin: match row.origin {
            FunctionOrigin::UserDefined => pb::FunctionOriginV1::FunctionOriginUserDefined as i32,
            FunctionOrigin::Companion => pb::FunctionOriginV1::FunctionOriginCompanion as i32,
            FunctionOrigin::Internal => pb::FunctionOriginV1::FunctionOriginInternal as i32,
            FunctionOrigin::Builtin => pb::FunctionOriginV1::FunctionOriginBuiltin as i32,
            FunctionOrigin::AutoDerive => pb::FunctionOriginV1::FunctionOriginAutoDerive as i32,
        },
        definition_key: row.definition_key.clone(),
        owner_type_key: row.owner_type_key.clone(),
        lambda: row.lambda.as_ref().map(|lambda| pb::LambdaIdentityV1 {
            parent_definition_key: lambda.parent_definition_key.clone(),
            ordinal: lambda.ordinal,
            kind: match lambda.kind {
                LambdaKind::Lambda => pb::LambdaKindV1::LambdaKindLambda as i32,
                LambdaKind::SpawnedClosure => pb::LambdaKindV1::LambdaKindSpawnedClosure as i32,
                LambdaKind::Adapter => pb::LambdaKindV1::LambdaKindAdapter as i32,
            },
        }),
        package_name: row.package_name.clone(),
        namespace: row.namespace.clone(),
        capture_flags: row.capture_flags,
        def_content_hash: row.def_content_hash.to_vec(),
        semantic_lanes: row
            .semantic_lanes
            .as_ref()
            .map(|lanes| pb::SemanticLanesV1 {
                direct_interface: lanes.direct_interface.to_vec(),
                effective_interface: lanes.effective_interface.to_vec(),
                direct_implementation: lanes.direct_implementation.map(|hash| hash.to_vec()),
                effective_implementation: lanes.effective_implementation.map(|hash| hash.to_vec()),
            }),
    }
}

fn function_from_proto(row: pb::FunctionDictRowV1) -> io::Result<FunctionDictRow> {
    let kind = match pb::FunctionKindV1::try_from(row.kind)
        .map_err(|_| invalid_data(format!("unknown function kind {}", row.kind)))?
    {
        pb::FunctionKindV1::FunctionKindBytecode => {
            if row.sys_op_name.is_some() {
                return Err(invalid_data(
                    "bytecode function unexpectedly has sys_op_name",
                ));
            }
            FunctionKind::Bytecode
        }
        pb::FunctionKindV1::FunctionKindSysOp => FunctionKind::SysOp(
            row.sys_op_name
                .ok_or_else(|| invalid_data("sys-op function omitted sys_op_name"))?,
        ),
        pb::FunctionKindV1::FunctionKindNative => {
            if row.sys_op_name.is_some() {
                return Err(invalid_data("native function unexpectedly has sys_op_name"));
            }
            FunctionKind::Native
        }
        pb::FunctionKindV1::FunctionKindUnspecified => {
            return Err(invalid_data("function kind is unspecified"));
        }
    };
    let origin = match pb::FunctionOriginV1::try_from(row.origin)
        .map_err(|_| invalid_data(format!("unknown function origin {}", row.origin)))?
    {
        pb::FunctionOriginV1::FunctionOriginUserDefined => FunctionOrigin::UserDefined,
        pb::FunctionOriginV1::FunctionOriginCompanion => FunctionOrigin::Companion,
        pb::FunctionOriginV1::FunctionOriginInternal => FunctionOrigin::Internal,
        pb::FunctionOriginV1::FunctionOriginBuiltin => FunctionOrigin::Builtin,
        pb::FunctionOriginV1::FunctionOriginAutoDerive => FunctionOrigin::AutoDerive,
        pb::FunctionOriginV1::FunctionOriginUnspecified => {
            return Err(invalid_data("function origin is unspecified"));
        }
    };

    Ok(FunctionDictRow {
        function_id: row.function_id,
        fqn: row.fqn,
        display_name: row.display_name,
        declared_name: row.declared_name,
        source_span: row.source_span.map(|span| SourceSpan {
            file_id: span.file_id,
            span_start: span.span_start,
            span_end: span.span_end,
            line: span.line,
        }),
        kind,
        origin,
        definition_key: row.definition_key,
        owner_type_key: row.owner_type_key,
        lambda: row.lambda.map(lambda_from_proto).transpose()?,
        package_name: row.package_name,
        namespace: row.namespace,
        capture_flags: row.capture_flags,
        def_content_hash: fixed_hash("function def_content_hash", row.def_content_hash)?,
        semantic_lanes: row
            .semantic_lanes
            .map(semantic_lanes_from_proto)
            .transpose()?,
    })
}

fn lambda_from_proto(lambda: pb::LambdaIdentityV1) -> io::Result<LambdaIdentity> {
    let kind = match pb::LambdaKindV1::try_from(lambda.kind)
        .map_err(|_| invalid_data(format!("unknown lambda kind {}", lambda.kind)))?
    {
        pb::LambdaKindV1::LambdaKindLambda => LambdaKind::Lambda,
        pb::LambdaKindV1::LambdaKindSpawnedClosure => LambdaKind::SpawnedClosure,
        pb::LambdaKindV1::LambdaKindAdapter => LambdaKind::Adapter,
        pb::LambdaKindV1::LambdaKindUnspecified => {
            return Err(invalid_data("lambda kind is unspecified"));
        }
    };
    Ok(LambdaIdentity {
        parent_definition_key: lambda.parent_definition_key,
        ordinal: lambda.ordinal,
        kind,
    })
}

fn semantic_lanes_from_proto(lanes: pb::SemanticLanesV1) -> io::Result<SemanticLanes> {
    Ok(SemanticLanes {
        direct_interface: fixed_hash("semantic direct_interface", lanes.direct_interface)?,
        effective_interface: fixed_hash("semantic effective_interface", lanes.effective_interface)?,
        direct_implementation: optional_hash(
            "semantic direct_implementation",
            lanes.direct_implementation,
        )?,
        effective_implementation: optional_hash(
            "semantic effective_implementation",
            lanes.effective_implementation,
        )?,
    })
}

fn saturating_u32(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

fn invalid_data(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

struct TempPath {
    path: PathBuf,
    armed: bool,
}

impl TempPath {
    fn new(path: PathBuf) -> Self {
        Self { path, armed: true }
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn remove(&mut self) -> io::Result<()> {
        fs::remove_file(&self.path)?;
        self.armed = false;
        Ok(())
    }
}

impl Drop for TempPath {
    fn drop(&mut self) {
        if self.armed {
            let _ = fs::remove_file(&self.path);
        }
    }
}

#[cfg(unix)]
fn sync_dictionary_dir(path: &Path) -> io::Result<()> {
    File::open(path)?.sync_all()
}

#[cfg(not(unix))]
fn sync_dictionary_dir(_path: &Path) -> io::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU32, Ordering};

    use super::*;
    use crate::revision_dictionary::{
        CallSiteRow, FIRST_POOL_FUNCTION_ID, FileRow, FunctionDictRow, FunctionKind,
        FunctionOrigin, LambdaIdentity, LambdaKind, ProgramIdentity, RevisionDictionary,
        RevisionId, SemanticLanes, SourceSnapshotId, SourceSpan,
    };

    fn temp_project(tag: &str) -> PathBuf {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        std::env::temp_dir().join(format!(
            "baml-dictionary-{}-{}-{tag}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ))
    }

    fn dictionary() -> RevisionDictionary {
        RevisionDictionary::new(
            ProgramIdentity {
                revision_id: RevisionId([0xA5; 32]),
                source_snapshot_id: SourceSnapshotId([0x5A; 32]),
                compiler_id: "1.2.3+test".to_owned(),
                function_count: 1,
            },
            7,
            vec![FileRow {
                file_id: 2,
                path: "src/main.baml".to_owned(),
                content_hash: [3; 32],
            }],
            vec![FunctionDictRow {
                function_id: FIRST_POOL_FUNCTION_ID,
                fqn: "user.main.lambda#0".to_owned(),
                display_name: "lambda".to_owned(),
                declared_name: None,
                source_span: Some(SourceSpan {
                    file_id: 2,
                    span_start: 10,
                    span_end: 30,
                    line: 4,
                }),
                kind: FunctionKind::Bytecode,
                origin: FunctionOrigin::UserDefined,
                definition_key: "lambda:function:user.main#0".to_owned(),
                owner_type_key: Some("class:user.Owner".to_owned()),
                lambda: Some(LambdaIdentity {
                    parent_definition_key: "function:user.main".to_owned(),
                    ordinal: 0,
                    kind: LambdaKind::Lambda,
                }),
                package_name: Some("user".to_owned()),
                namespace: vec!["agent".to_owned(), "tools".to_owned()],
                capture_flags: 0x155,
                def_content_hash: [4; 32],
                semantic_lanes: Some(SemanticLanes {
                    direct_interface: [5; 32],
                    effective_interface: [6; 32],
                    direct_implementation: Some([7; 32]),
                    effective_implementation: None,
                }),
            }],
            vec![CallSiteRow {
                call_site_id: 9,
                caller_function_id: FIRST_POOL_FUNCTION_ID,
                callee_function_id: Some(FIRST_POOL_FUNCTION_ID),
                file_id: 2,
                span_start: 15,
                span_end: 20,
                line: 5,
            }],
        )
        .unwrap()
    }

    #[test]
    fn dictionary_v1_is_byte_exact_and_frozen() {
        let encoded = encode_dictionary(&dictionary()).unwrap();
        let expected = decode_hex(include_str!(
            "../../tests/fixtures/obs/v1/revision_dictionary.hex"
        ));
        assert_eq!(encoded, expected);
        assert_eq!(decode_dictionary(&expected).unwrap(), dictionary());
    }

    fn decode_hex(input: &str) -> Vec<u8> {
        let input = input.trim();
        assert_eq!(input.len() % 2, 0, "hex fixture must have whole bytes");
        (0..input.len())
            .step_by(2)
            .map(|offset| u8::from_str_radix(&input[offset..offset + 2], 16).unwrap())
            .collect()
    }

    #[test]
    fn native_dictionary_round_trips_all_sections_and_synthetic_rows() {
        let project = temp_project("roundtrip");
        let store = RevisionDictionaryStore::new(&project);
        let expected = dictionary();

        let outcome = store.ensure_written(&expected).unwrap();
        assert_eq!(outcome.disposition, DictionaryWriteDisposition::Written);
        assert_eq!(
            outcome.path.file_name().unwrap().to_str().unwrap(),
            dictionary_file_name(expected.identity.revision_id)
        );

        let actual = store.read(expected.identity.revision_id).unwrap();
        assert_eq!(actual, expected);
        assert_eq!(actual.functions[0].function_id, 0);
        assert_eq!(actual.functions[1].function_id, 1);

        fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn second_content_addressed_write_is_an_idempotent_noop() {
        let project = temp_project("idempotent");
        let store = RevisionDictionaryStore::new(&project);
        let expected = dictionary();

        let first = store.ensure_written(&expected).unwrap();
        let bytes_before = fs::read(&first.path).unwrap();
        let metadata_before = fs::metadata(&first.path).unwrap();
        let second = store.ensure_written(&expected).unwrap();

        assert_eq!(first.disposition, DictionaryWriteDisposition::Written);
        assert_eq!(
            second.disposition,
            DictionaryWriteDisposition::AlreadyExists
        );
        assert_eq!(second.path, first.path);
        assert_eq!(fs::read(&first.path).unwrap(), bytes_before);
        assert_eq!(
            fs::metadata(&first.path).unwrap().modified().unwrap(),
            metadata_before.modified().unwrap()
        );
        assert!(fs::read_dir(store.dictionary_dir()).unwrap().all(|entry| {
            !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .ends_with(".tmp")
        }));

        fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn reader_skips_an_unknown_future_section() {
        let expected = dictionary();
        let encoded = encode_dictionary(&expected).unwrap();

        // A valid length-delimited RevisionDictionaryV1 with unknown field 15
        // (wire type 2) and an empty payload, inserted between V1 records.
        let first_record_len = {
            let first_payload_len = usize::from(encoded[0]);
            first_payload_len + 1
        };
        let mut with_future_section = Vec::with_capacity(encoded.len() + 3);
        with_future_section.extend_from_slice(&encoded[..first_record_len]);
        with_future_section.extend_from_slice(&[2, 0x7A, 0]);
        with_future_section.extend_from_slice(&encoded[first_record_len..]);

        assert_eq!(decode_dictionary(&with_future_section).unwrap(), expected);
    }

    #[test]
    fn reader_rejects_non_32_byte_content_ids() {
        let expected = dictionary();
        let encoded = encode_dictionary(&expected).unwrap();
        let mut input = encoded.as_slice();
        let mut header_record =
            pb::RevisionDictionaryV1::decode_length_delimited(&mut input).unwrap();
        let Some(pb::revision_dictionary_v1::Section::Header(header)) =
            header_record.section.as_mut()
        else {
            panic!("first section must be header");
        };
        header.revision_id.pop();

        let mut malformed = Vec::new();
        header_record
            .encode_length_delimited(&mut malformed)
            .unwrap();
        malformed.extend_from_slice(input);

        let error = decode_dictionary(&malformed).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert!(error.to_string().contains("exactly 32 bytes"));
    }
}
