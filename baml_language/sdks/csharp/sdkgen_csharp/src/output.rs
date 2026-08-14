//! C#-specific output validation.
//!
//! Filesystem installation belongs exclusively to
//! `baml_codegen_types::write_generated_output`.

use std::{
    collections::BTreeSet,
    fmt,
    fmt::Write as _,
    path::{Component, Path, PathBuf},
};

use baml_codegen_types::GeneratedOutputFile;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::routing::is_safe_portable_segment;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct GeneratedFile {
    pub relative_path: PathBuf,
    pub contents: Vec<u8>,
}

impl GeneratedFile {
    #[must_use]
    pub(crate) fn new(relative_path: impl Into<PathBuf>, contents: impl Into<Vec<u8>>) -> Self {
        Self {
            relative_path: relative_path.into(),
            contents: contents.into(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct GenerationMetadata {
    pub schema_version: u32,
    pub cli_version: String,
    pub required_bridge_version: String,
    pub program_identity: String,
    pub program_fingerprint: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct GeneratedTree {
    pub(crate) metadata: GenerationMetadata,
    /// Exact compiler-produced bytes embedded into generated C# source.
    pub(crate) program_bytes: Vec<u8>,
    pub(crate) files: Vec<GeneratedFile>,
    pub(crate) recursive_aliases: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ManifestEntry {
    pub relative_path: String,
    pub sha256: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct GenerationManifest {
    pub schema_version: u32,
    pub cli_version: String,
    pub required_bridge_version: String,
    pub program_identity: String,
    pub program_fingerprint: String,
    pub files: Vec<ManifestEntry>,
}

#[derive(Debug)]
pub enum OutputValidationError {
    UnsafeGeneratedPath(PathBuf),
    DuplicateCaseInsensitivePath(PathBuf),
    InvalidGeneratedSource(PathBuf, &'static str),
    InvalidMetadata(&'static str),
    ProgramFingerprintMismatch { expected: String, actual: String },
    RecursiveAliases(Vec<String>),
}

impl fmt::Display for OutputValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsafeGeneratedPath(path) => {
                write!(formatter, "unsafe generated file path `{}`", path.display())
            }
            Self::DuplicateCaseInsensitivePath(path) => write!(
                formatter,
                "case-insensitive generated path collision at `{}`",
                path.display()
            ),
            Self::InvalidGeneratedSource(path, reason) => {
                write!(
                    formatter,
                    "invalid generated source `{}`: {reason}",
                    path.display()
                )
            }
            Self::InvalidMetadata(reason) => {
                write!(formatter, "invalid generation metadata: {reason}")
            }
            Self::ProgramFingerprintMismatch { expected, actual } => write!(
                formatter,
                "program fingerprint mismatch: metadata contains `{expected}`, exact bytes hash to `{actual}`"
            ),
            Self::RecursiveAliases(aliases) => write!(
                formatter,
                "recursive aliases are unsupported by C# generation: {}",
                aliases.join(", ")
            ),
        }
    }
}

impl std::error::Error for OutputValidationError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        None
    }
}

pub(crate) fn validate_and_collect(
    tree: &GeneratedTree,
) -> Result<(GenerationManifest, Vec<GeneratedOutputFile>), OutputValidationError> {
    let manifest = validate_tree(tree)?;
    let files = tree
        .files
        .iter()
        .map(|file| GeneratedOutputFile::new(file.relative_path.clone(), file.contents.clone()))
        .collect::<Vec<_>>();
    Ok((manifest, files))
}

fn validate_tree(tree: &GeneratedTree) -> Result<GenerationManifest, OutputValidationError> {
    if !tree.recursive_aliases.is_empty() {
        let mut aliases = tree.recursive_aliases.clone();
        aliases.sort();
        aliases.dedup();
        return Err(OutputValidationError::RecursiveAliases(aliases));
    }
    if tree.metadata.schema_version == 0 {
        return Err(OutputValidationError::InvalidMetadata(
            "schema version must be non-zero",
        ));
    }
    if tree.metadata.cli_version.is_empty()
        || tree.metadata.required_bridge_version.is_empty()
        || tree.metadata.program_identity.is_empty()
    {
        return Err(OutputValidationError::InvalidMetadata(
            "versions and program identity must be non-empty",
        ));
    }
    if tree.program_bytes.is_empty() {
        return Err(OutputValidationError::InvalidMetadata(
            "program bytecode must be non-empty",
        ));
    }
    let actual_fingerprint = sha256(&tree.program_bytes);
    if tree.metadata.program_fingerprint != actual_fingerprint {
        return Err(OutputValidationError::ProgramFingerprintMismatch {
            expected: tree.metadata.program_fingerprint.clone(),
            actual: actual_fingerprint,
        });
    }
    if tree.files.is_empty() {
        return Err(OutputValidationError::InvalidMetadata(
            "generated file inventory must be non-empty",
        ));
    }

    let mut seen = BTreeSet::new();
    let mut files = tree
        .files
        .iter()
        .map(|file| (portable_path(&file.relative_path), file))
        .collect::<Vec<_>>();
    files.sort_by(|left, right| left.0.cmp(&right.0));
    let mut entries = Vec::with_capacity(files.len());
    for (portable, file) in files {
        validate_generated_path(&file.relative_path)?;
        if !seen.insert(portable.to_lowercase()) {
            return Err(OutputValidationError::DuplicateCaseInsensitivePath(
                file.relative_path.clone(),
            ));
        }
        validate_source(file)?;
        entries.push(ManifestEntry {
            relative_path: portable,
            sha256: sha256(&file.contents),
        });
    }

    Ok(GenerationManifest {
        schema_version: tree.metadata.schema_version,
        cli_version: tree.metadata.cli_version.clone(),
        required_bridge_version: tree.metadata.required_bridge_version.clone(),
        program_identity: tree.metadata.program_identity.clone(),
        program_fingerprint: tree.metadata.program_fingerprint.clone(),
        files: entries,
    })
}

fn validate_generated_path(path: &Path) -> Result<(), OutputValidationError> {
    if path.as_os_str().is_empty()
        || path.is_absolute()
        || path.components().any(|component| match component {
            Component::Normal(value) => value
                .to_str()
                .is_none_or(|segment| !is_safe_portable_segment(segment)),
            _ => true,
        })
        || !path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.ends_with(".g.cs"))
    {
        return Err(OutputValidationError::UnsafeGeneratedPath(
            path.to_path_buf(),
        ));
    }
    Ok(())
}

fn validate_source(file: &GeneratedFile) -> Result<(), OutputValidationError> {
    let path = file.relative_path.clone();
    let source = std::str::from_utf8(&file.contents).map_err(|_| {
        OutputValidationError::InvalidGeneratedSource(path.clone(), "source is not UTF-8")
    })?;
    if file.contents.starts_with(&[0xef, 0xbb, 0xbf]) {
        return Err(OutputValidationError::InvalidGeneratedSource(
            path,
            "UTF-8 BOM is forbidden",
        ));
    }
    if source.contains('\r') {
        return Err(OutputValidationError::InvalidGeneratedSource(
            path,
            "CRLF/CR line endings are forbidden",
        ));
    }
    if !source.starts_with("// <auto-generated />\n#nullable enable\n") {
        return Err(OutputValidationError::InvalidGeneratedSource(
            path,
            "standard generated header is missing",
        ));
    }
    Ok(())
}

fn portable_path(path: &Path) -> String {
    path.components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .fold(String::with_capacity(64), |mut encoded, byte| {
            write!(&mut encoded, "{byte:02x}").expect("writing to String cannot fail");
            encoded
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEADER: &str = "// <auto-generated />\n#nullable enable\n";

    fn tree(files: &[(&str, &str)]) -> GeneratedTree {
        let program_bytes = b"canonical-bytecode".to_vec();
        GeneratedTree {
            metadata: GenerationMetadata {
                schema_version: 1,
                cli_version: "1.2.3".to_string(),
                required_bridge_version: "1.2.3".to_string(),
                program_identity: "user-program".to_string(),
                program_fingerprint: sha256(&program_bytes),
            },
            program_bytes,
            files: files
                .iter()
                .map(|(path, body)| GeneratedFile::new(path, format!("{HEADER}{body}\n")))
                .collect(),
            recursive_aliases: Vec::new(),
        }
    }

    #[test]
    fn valid_tree_produces_deterministic_manifest_and_writer_inventory() {
        let generated = tree(&[
            ("Zed.g.cs", "class Zed {}"),
            ("Acme/Alpha.g.cs", "class Alpha {}"),
        ]);
        let (manifest, files) = validate_and_collect(&generated).unwrap();
        assert_eq!(
            manifest
                .files
                .iter()
                .map(|entry| entry.relative_path.as_str())
                .collect::<Vec<_>>(),
            ["Acme/Alpha.g.cs", "Zed.g.cs"]
        );
        assert!(manifest.files.iter().all(|entry| entry.sha256.len() == 64));
        assert_eq!(files.len(), 2);
    }

    #[test]
    fn invalid_inputs_fail_before_reaching_the_shared_writer() {
        let mut recursive = tree(&[("Value.g.cs", "class Replacement {}")]);
        recursive.recursive_aliases = vec!["Node".to_string(), "Node".to_string()];
        assert_eq!(
            validate_and_collect(&recursive).unwrap_err().to_string(),
            "recursive aliases are unsupported by C# generation: Node"
        );

        let collision = tree(&[("Foo.g.cs", "class Foo {}"), ("foo.g.cs", "class Foo2 {}")]);
        assert!(matches!(
            validate_and_collect(&collision),
            Err(OutputValidationError::DuplicateCaseInsensitivePath(_))
        ));

        for invalid in [
            GeneratedFile::new("../Escape.g.cs", format!("{HEADER}class Escape {{}}\n")),
            GeneratedFile::new("NUL.g.cs", format!("{HEADER}class Device {{}}\n")),
            GeneratedFile::new("Bad.cs", format!("{HEADER}class Bad {{}}\n")),
            GeneratedFile::new("Bad<Name.g.cs", format!("{HEADER}class Bad {{}}\n")),
            GeneratedFile::new("Trailing /Name.g.cs", format!("{HEADER}class Bad {{}}\n")),
            GeneratedFile::new(
                "Bom.g.cs",
                [vec![0xef, 0xbb, 0xbf], HEADER.as_bytes().to_vec()].concat(),
            ),
            GeneratedFile::new("CrLf.g.cs", "// <auto-generated />\r\n#nullable enable\r\n"),
        ] {
            let invalid_tree = GeneratedTree {
                metadata: recursive.metadata.clone(),
                program_bytes: recursive.program_bytes.clone(),
                files: vec![invalid],
                recursive_aliases: Vec::new(),
            };
            assert!(validate_and_collect(&invalid_tree).is_err());
        }

        let mut corrupt_fingerprint = tree(&[("Value.g.cs", "class Replacement {}")]);
        corrupt_fingerprint.metadata.program_fingerprint = "0".repeat(64);
        assert!(matches!(
            validate_and_collect(&corrupt_fingerprint),
            Err(OutputValidationError::ProgramFingerprintMismatch { .. })
        ));
        let mut empty_bytecode = tree(&[("Value.g.cs", "class Replacement {}")]);
        empty_bytecode.program_bytes.clear();
        assert!(matches!(
            validate_and_collect(&empty_bytecode),
            Err(OutputValidationError::InvalidMetadata(
                "program bytecode must be non-empty"
            ))
        ));
        assert!(matches!(
            validate_and_collect(&tree(&[])),
            Err(OutputValidationError::InvalidMetadata(
                "generated file inventory must be non-empty"
            ))
        ));
    }
}
