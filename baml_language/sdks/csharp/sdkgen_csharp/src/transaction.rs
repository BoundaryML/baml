//! Deterministic whole-directory commit for generator-owned C# output.

use std::{
    collections::BTreeSet,
    fmt,
    fmt::Write as _,
    fs, io,
    path::{Component, Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::routing::is_safe_portable_segment;

pub(crate) const MANIFEST_FILE: &str = ".baml-generator-manifest.json";
const STAGING_MARKER: &str = ".baml-generator-staging";
const STAGING_MARKER_CONTENT: &[u8] = b"baml-generator-owned-staging-v1\n";

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

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
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
    /// Exact compiler-produced bytes embedded into generated C# source. The
    /// transaction validates them but never writes a loose bytecode file.
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
pub enum TransactionError {
    UnsafeOutputPath(PathBuf),
    OutputParentMissing(PathBuf),
    UnsafeGeneratedPath(PathBuf),
    DuplicateCaseInsensitivePath(PathBuf),
    InvalidGeneratedSource(PathBuf, &'static str),
    InvalidMetadata(&'static str),
    ProgramFingerprintMismatch {
        expected: String,
        actual: String,
    },
    RecursiveAliases(Vec<String>),
    UnownedPath(PathBuf),
    Io {
        operation: &'static str,
        path: PathBuf,
        source: io::Error,
    },
    ReplacementRollbackFailed {
        replacement: io::Error,
        rollback: io::Error,
        backup: PathBuf,
    },
    ManifestSerialization(serde_json::Error),
}

impl fmt::Display for TransactionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsafeOutputPath(path) => {
                write!(f, "unsafe generated output path `{}`", path.display())
            }
            Self::OutputParentMissing(path) => write!(
                f,
                "generated output parent does not exist: `{}`",
                path.display()
            ),
            Self::UnsafeGeneratedPath(path) => {
                write!(f, "unsafe generated file path `{}`", path.display())
            }
            Self::DuplicateCaseInsensitivePath(path) => write!(
                f,
                "case-insensitive generated path collision at `{}`",
                path.display()
            ),
            Self::InvalidGeneratedSource(path, reason) => {
                write!(f, "invalid generated source `{}`: {reason}", path.display())
            }
            Self::InvalidMetadata(reason) => write!(f, "invalid generation metadata: {reason}"),
            Self::ProgramFingerprintMismatch { expected, actual } => write!(
                f,
                "program fingerprint mismatch: metadata contains `{expected}`, exact bytes hash to `{actual}`"
            ),
            Self::RecursiveAliases(aliases) => write!(
                f,
                "recursive aliases are unsupported by C# generation: {}",
                aliases.join(", ")
            ),
            Self::UnownedPath(path) => write!(
                f,
                "refusing to replace or remove unowned path `{}`",
                path.display()
            ),
            Self::Io {
                operation,
                path,
                source,
            } => write!(f, "{operation} `{}`: {source}", path.display()),
            Self::ReplacementRollbackFailed {
                replacement,
                rollback,
                backup,
            } => write!(
                f,
                "output replacement failed ({replacement}) and rollback failed ({rollback}); last complete output remains at `{}`",
                backup.display()
            ),
            Self::ManifestSerialization(source) => {
                write!(f, "serialize generation manifest: {source}")
            }
        }
    }
}

impl std::error::Error for TransactionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::ReplacementRollbackFailed { replacement, .. } => Some(replacement),
            Self::ManifestSerialization(source) => Some(source),
            _ => None,
        }
    }
}

pub(crate) fn commit_generated_tree(
    project_root: &Path,
    output_directory: &Path,
    tree: &GeneratedTree,
) -> Result<GenerationManifest, TransactionError> {
    commit_generated_tree_with_rename(project_root, output_directory, tree, |from, to| {
        fs::rename(from, to)
    })
}

fn commit_generated_tree_with_rename(
    project_root: &Path,
    output_directory: &Path,
    tree: &GeneratedTree,
    mut rename: impl FnMut(&Path, &Path) -> io::Result<()>,
) -> Result<GenerationManifest, TransactionError> {
    validate_output_path(output_directory)?;
    let manifest = validate_tree(tree)?;
    let target = project_root.join(output_directory);
    let parent = target
        .parent()
        .ok_or_else(|| TransactionError::UnsafeOutputPath(output_directory.to_path_buf()))?;
    if !parent.is_dir() {
        return Err(TransactionError::OutputParentMissing(parent.to_path_buf()));
    }
    let canonical_root = fs::canonicalize(project_root)
        .map_err(|source| io_error("canonicalize project root", project_root, source))?;
    let canonical_parent = fs::canonicalize(parent)
        .map_err(|source| io_error("canonicalize output parent", parent, source))?;
    if !canonical_parent.starts_with(&canonical_root) {
        return Err(TransactionError::UnsafeOutputPath(
            output_directory.to_path_buf(),
        ));
    }
    let stem = target
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| TransactionError::UnsafeOutputPath(output_directory.to_path_buf()))?;
    let staging = parent.join(format!(".{stem}.baml-staging"));
    let backup = parent.join(format!(".{stem}.baml-backup"));
    let lock_directory = std::env::temp_dir().join("baml-sdkgen-csharp-locks");
    if path_lexists(&lock_directory) {
        reject_symlink(&lock_directory)?;
    }
    fs::create_dir_all(&lock_directory)
        .map_err(|source| io_error("create generation lock directory", &lock_directory, source))?;
    reject_symlink(&lock_directory)?;
    let lock_path = generation_lock_path(&lock_directory, &canonical_parent, stem);
    if path_lexists(&lock_path) {
        reject_symlink(&lock_path)?;
    }
    let lock = fs::OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(&lock_path)
        .map_err(|source| io_error("open generation lock", &lock_path, source))?;
    try_lock_exclusive(&lock)
        .map_err(|source| io_error("acquire generation lock", &lock_path, source))?;

    recover_or_clean_backup(&target, &backup, &mut rename)?;
    clean_staging(&staging)?;
    write_staging_tree(&staging, tree, &manifest)?;
    validate_staged_tree(&staging, &manifest)?;
    let staging_marker = staging.join(STAGING_MARKER);
    fs::remove_file(&staging_marker)
        .map_err(|source| io_error("remove staging marker", &staging_marker, source))?;

    if path_lexists(&target) {
        require_owned_output(&target)?;
        rename(&target, &backup)
            .map_err(|source| io_error("move prior output to backup", &target, source))?;
        if let Err(replacement) = rename(&staging, &target) {
            return match rename(&backup, &target) {
                Ok(()) => Err(io_error("replace generated output", &target, replacement)),
                Err(rollback) => Err(TransactionError::ReplacementRollbackFailed {
                    replacement,
                    rollback,
                    backup,
                }),
            };
        }
        // The new tree is already complete and installed. A backup cleanup
        // failure must not turn a successful commit into an apparent failed
        // generation; the next invocation validates and removes the residue.
        let _ = remove_owned_output(&backup);
    } else {
        rename(&staging, &target)
            .map_err(|source| io_error("install generated output", &target, source))?;
    }

    Ok(manifest)
}

fn validate_output_path(path: &Path) -> Result<(), TransactionError> {
    if path.as_os_str().is_empty()
        || path.is_absolute()
        || path.components().any(|component| match component {
            Component::Normal(value) => value
                .to_str()
                .is_none_or(|segment| !is_safe_portable_segment(segment)),
            _ => true,
        })
    {
        return Err(TransactionError::UnsafeOutputPath(path.to_path_buf()));
    }
    Ok(())
}

fn validate_tree(tree: &GeneratedTree) -> Result<GenerationManifest, TransactionError> {
    if !tree.recursive_aliases.is_empty() {
        let mut aliases = tree.recursive_aliases.clone();
        aliases.sort();
        aliases.dedup();
        return Err(TransactionError::RecursiveAliases(aliases));
    }
    if tree.metadata.schema_version == 0 {
        return Err(TransactionError::InvalidMetadata(
            "schema version must be non-zero",
        ));
    }
    if tree.metadata.cli_version.is_empty()
        || tree.metadata.required_bridge_version.is_empty()
        || tree.metadata.program_identity.is_empty()
    {
        return Err(TransactionError::InvalidMetadata(
            "versions and program identity must be non-empty",
        ));
    }
    if tree.program_bytes.is_empty() {
        return Err(TransactionError::InvalidMetadata(
            "program bytecode must be non-empty",
        ));
    }
    let actual_fingerprint = sha256(&tree.program_bytes);
    if tree.metadata.program_fingerprint != actual_fingerprint {
        return Err(TransactionError::ProgramFingerprintMismatch {
            expected: tree.metadata.program_fingerprint.clone(),
            actual: actual_fingerprint,
        });
    }
    if tree.files.is_empty() {
        return Err(TransactionError::InvalidMetadata(
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
        let comparison = portable.to_lowercase();
        if !seen.insert(comparison) {
            return Err(TransactionError::DuplicateCaseInsensitivePath(
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

fn validate_generated_path(path: &Path) -> Result<(), TransactionError> {
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
        return Err(TransactionError::UnsafeGeneratedPath(path.to_path_buf()));
    }
    Ok(())
}

fn validate_source(file: &GeneratedFile) -> Result<(), TransactionError> {
    let path = file.relative_path.clone();
    let source = std::str::from_utf8(&file.contents).map_err(|_| {
        TransactionError::InvalidGeneratedSource(path.clone(), "source is not UTF-8")
    })?;
    if file.contents.starts_with(&[0xef, 0xbb, 0xbf]) {
        return Err(TransactionError::InvalidGeneratedSource(
            path,
            "UTF-8 BOM is forbidden",
        ));
    }
    if source.contains('\r') {
        return Err(TransactionError::InvalidGeneratedSource(
            path,
            "CRLF/CR line endings are forbidden",
        ));
    }
    if !source.starts_with("// <auto-generated />\n#nullable enable\n") {
        return Err(TransactionError::InvalidGeneratedSource(
            path,
            "standard generated header is missing",
        ));
    }
    Ok(())
}

fn write_staging_tree(
    staging: &Path,
    tree: &GeneratedTree,
    manifest: &GenerationManifest,
) -> Result<(), TransactionError> {
    fs::create_dir(staging)
        .map_err(|source| io_error("create staging directory", staging, source))?;
    let marker = staging.join(STAGING_MARKER);
    fs::write(&marker, STAGING_MARKER_CONTENT)
        .map_err(|source| io_error("write staging marker", &marker, source))?;

    let result = (|| {
        for file in &tree.files {
            let destination = staging.join(&file.relative_path);
            let parent = destination
                .parent()
                .expect("validated generated path has a parent");
            fs::create_dir_all(parent)
                .map_err(|source| io_error("create generated directory", parent, source))?;
            fs::write(&destination, &file.contents)
                .map_err(|source| io_error("write generated source", &destination, source))?;
        }
        let mut json = serde_json::to_string_pretty(manifest)
            .map_err(TransactionError::ManifestSerialization)?;
        json.push('\n');
        let path = staging.join(MANIFEST_FILE);
        fs::write(&path, json)
            .map_err(|source| io_error("write generation manifest", &path, source))
    })();

    if result.is_err() {
        let _ = remove_staging(staging);
    }
    result
}

fn validate_staged_tree(
    staging: &Path,
    expected: &GenerationManifest,
) -> Result<(), TransactionError> {
    let manifest_path = staging.join(MANIFEST_FILE);
    reject_symlink(&manifest_path)?;
    let manifest_bytes = fs::read(&manifest_path)
        .map_err(|source| io_error("read staged manifest", &manifest_path, source))?;
    let staged_manifest: GenerationManifest =
        serde_json::from_slice(&manifest_bytes).map_err(TransactionError::ManifestSerialization)?;
    if &staged_manifest != expected {
        return Err(TransactionError::InvalidMetadata(
            "staged manifest differs from validated manifest",
        ));
    }

    let mut staged_files = Vec::new();
    collect_staged_files(staging, staging, &mut staged_files)?;
    staged_files.sort();
    let expected_paths = expected
        .files
        .iter()
        .map(|entry| entry.relative_path.clone())
        .collect::<Vec<_>>();
    if staged_files != expected_paths {
        return Err(TransactionError::InvalidMetadata(
            "staged file inventory differs from manifest",
        ));
    }

    for entry in &expected.files {
        let path = staging.join(&entry.relative_path);
        let bytes = fs::read(&path)
            .map_err(|source| io_error("read staged generated source", &path, source))?;
        if sha256(&bytes) != entry.sha256 {
            return Err(TransactionError::InvalidGeneratedSource(
                PathBuf::from(&entry.relative_path),
                "staged bytes do not match manifest hash",
            ));
        }
    }
    Ok(())
}

fn collect_staged_files(
    staging: &Path,
    directory: &Path,
    files: &mut Vec<String>,
) -> Result<(), TransactionError> {
    let entries = fs::read_dir(directory)
        .map_err(|source| io_error("read staged directory", directory, source))?;
    for entry in entries {
        let entry =
            entry.map_err(|source| io_error("read staged directory entry", directory, source))?;
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path)
            .map_err(|source| io_error("inspect staged path", &path, source))?;
        if metadata.file_type().is_symlink() {
            return Err(TransactionError::UnownedPath(path));
        }
        if metadata.is_dir() {
            collect_staged_files(staging, &path, files)?;
        } else if metadata.is_file() {
            let relative = path
                .strip_prefix(staging)
                .expect("staged entry is below staging root");
            let portable = portable_path(relative);
            if portable != MANIFEST_FILE && portable != STAGING_MARKER {
                files.push(portable);
            }
        } else {
            return Err(TransactionError::UnownedPath(path));
        }
    }
    Ok(())
}

fn recover_or_clean_backup(
    target: &Path,
    backup: &Path,
    rename: &mut impl FnMut(&Path, &Path) -> io::Result<()>,
) -> Result<(), TransactionError> {
    if !path_lexists(backup) {
        return Ok(());
    }
    require_owned_output(backup)?;
    if path_lexists(target) {
        require_owned_output(target)?;
        remove_owned_output(backup)
    } else {
        rename(backup, target)
            .map_err(|source| io_error("recover prior generated output", backup, source))
    }
}

fn clean_staging(staging: &Path) -> Result<(), TransactionError> {
    if path_lexists(staging) {
        remove_staging(staging)
    } else {
        Ok(())
    }
}

fn remove_staging(staging: &Path) -> Result<(), TransactionError> {
    reject_symlink(staging)?;
    let marker = staging.join(STAGING_MARKER);
    if path_lexists(&marker) {
        reject_symlink(&marker)?;
        if !fs::read(&marker).is_ok_and(|contents| contents == STAGING_MARKER_CONTENT) {
            return Err(TransactionError::UnownedPath(staging.to_path_buf()));
        }
    } else {
        validate_owned_tree(staging)?;
    }
    fs::remove_dir_all(staging)
        .map_err(|source| io_error("remove staging directory", staging, source))
}

fn require_owned_output(path: &Path) -> Result<(), TransactionError> {
    reject_symlink(path)?;
    if !path.is_dir() || !path.join(MANIFEST_FILE).is_file() {
        Err(TransactionError::UnownedPath(path.to_path_buf()))
    } else {
        validate_owned_tree(path)
    }
}

fn validate_owned_tree(path: &Path) -> Result<(), TransactionError> {
    let manifest_path = path.join(MANIFEST_FILE);
    reject_symlink(&manifest_path)?;
    let manifest_bytes = fs::read(&manifest_path)
        .map_err(|source| io_error("read generation manifest", &manifest_path, source))?;
    let manifest: GenerationManifest =
        serde_json::from_slice(&manifest_bytes).map_err(TransactionError::ManifestSerialization)?;
    validate_manifest(&manifest)?;
    validate_staged_tree(path, &manifest)
}

fn validate_manifest(manifest: &GenerationManifest) -> Result<(), TransactionError> {
    if manifest.schema_version == 0 {
        return Err(TransactionError::InvalidMetadata(
            "manifest schema version must be non-zero",
        ));
    }
    if manifest.cli_version.is_empty()
        || manifest.required_bridge_version.is_empty()
        || manifest.program_identity.is_empty()
    {
        return Err(TransactionError::InvalidMetadata(
            "manifest versions and program identity must be non-empty",
        ));
    }
    if !is_lower_hex_sha256(&manifest.program_fingerprint) {
        return Err(TransactionError::InvalidMetadata(
            "manifest program fingerprint must be lowercase SHA-256",
        ));
    }
    if manifest.files.is_empty() {
        return Err(TransactionError::InvalidMetadata(
            "manifest file inventory must be non-empty",
        ));
    }

    let mut seen = BTreeSet::new();
    let mut previous = None::<&str>;
    for entry in &manifest.files {
        let path = PathBuf::from(&entry.relative_path);
        validate_generated_path(&path)?;
        if portable_path(&path) != entry.relative_path {
            return Err(TransactionError::UnsafeGeneratedPath(path));
        }
        if previous.is_some_and(|value| value >= entry.relative_path.as_str()) {
            return Err(TransactionError::InvalidMetadata(
                "manifest file inventory must be strictly sorted",
            ));
        }
        previous = Some(&entry.relative_path);
        if !seen.insert(entry.relative_path.to_lowercase()) {
            return Err(TransactionError::DuplicateCaseInsensitivePath(path));
        }
        if !is_lower_hex_sha256(&entry.sha256) {
            return Err(TransactionError::InvalidMetadata(
                "manifest file hash must be lowercase SHA-256",
            ));
        }
    }
    Ok(())
}

fn is_lower_hex_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn reject_symlink(path: &Path) -> Result<(), TransactionError> {
    if fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
        Err(TransactionError::UnownedPath(path.to_path_buf()))
    } else {
        Ok(())
    }
}

fn path_lexists(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok()
}

#[cfg(not(target_arch = "wasm32"))]
fn try_lock_exclusive(file: &fs::File) -> io::Result<()> {
    fs2::FileExt::try_lock_exclusive(file)
}

#[cfg(target_arch = "wasm32")]
#[expect(
    clippy::unnecessary_wraps,
    reason = "share the fallible native locking interface at the target boundary"
)]
fn try_lock_exclusive(_file: &fs::File) -> io::Result<()> {
    // The C# generator is a native CLI surface. This stub keeps the workspace
    // crate target-compatible without pulling a native file-locking backend
    // into browser builds; the filesystem transaction is never executed there.
    Ok(())
}

fn generation_lock_path(lock_directory: &Path, canonical_parent: &Path, stem: &str) -> PathBuf {
    let target = canonical_parent.join(stem);
    let case_insensitive_identity = target.to_string_lossy().to_lowercase();
    lock_directory.join(format!(
        "{}.lock",
        sha256(case_insensitive_identity.as_bytes())
    ))
}

fn remove_owned_output(path: &Path) -> Result<(), TransactionError> {
    require_owned_output(path)?;
    fs::remove_dir_all(path).map_err(|source| io_error("remove generated output", path, source))
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

fn io_error(operation: &'static str, path: &Path, source: io::Error) -> TransactionError {
    TransactionError::Io {
        operation,
        path: path.to_path_buf(),
        source,
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use tempfile::TempDir;

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
    fn repeat_is_byte_identical_and_stale_files_are_removed() {
        let root = TempDir::new().unwrap();
        let output = Path::new("baml_client");
        fs::create_dir(root.path().join("BamlExtensions")).unwrap();
        let user_file = root.path().join("BamlExtensions/User.cs");
        fs::write(&user_file, b"partial class UserExtension {}\n").unwrap();
        let first = tree(&[
            ("Acme/First.g.cs", "class First {}"),
            ("Stale.g.cs", "class Stale {}"),
        ]);
        let manifest = commit_generated_tree(root.path(), output, &first).unwrap();
        let manifest_bytes = fs::read(root.path().join(output).join(MANIFEST_FILE)).unwrap();
        assert_eq!(manifest.files.len(), 2);

        commit_generated_tree(root.path(), output, &first).unwrap();
        assert_eq!(
            fs::read(root.path().join(output).join(MANIFEST_FILE)).unwrap(),
            manifest_bytes
        );
        assert!(!root.path().join(".baml_client.baml-lock").exists());

        let second = tree(&[(
            "Acme/First.g.cs",
            "class First { public int Value { get; } }",
        )]);
        commit_generated_tree(root.path(), output, &second).unwrap();
        assert!(!root.path().join(output).join("Stale.g.cs").exists());
        assert!(root.path().join(output).join("Acme/First.g.cs").exists());
        assert_eq!(
            fs::read(&user_file).unwrap(),
            b"partial class UserExtension {}\n"
        );
    }

    #[test]
    fn failed_replacement_rolls_back_last_complete_tree() {
        let root = TempDir::new().unwrap();
        let output = Path::new("baml_client");
        let original = tree(&[("Value.g.cs", "class Original {}")]);
        commit_generated_tree(root.path(), output, &original).unwrap();
        let original_bytes = fs::read(root.path().join(output).join("Value.g.cs")).unwrap();

        let calls = Cell::new(0);
        let replacement = tree(&[("Value.g.cs", "class Replacement {}")]);
        let result =
            commit_generated_tree_with_rename(root.path(), output, &replacement, |from, to| {
                let call = calls.get() + 1;
                calls.set(call);
                if call == 2 {
                    Err(io::Error::other("injected replacement failure"))
                } else {
                    fs::rename(from, to)
                }
            });
        assert!(result.is_err());
        assert_eq!(
            fs::read(root.path().join(output).join("Value.g.cs")).unwrap(),
            original_bytes
        );
    }

    #[test]
    fn interrupted_staging_is_cleaned_but_unowned_paths_are_preserved() {
        let root = TempDir::new().unwrap();
        let staging = root.path().join(".baml_client.baml-staging");
        fs::create_dir(&staging).unwrap();
        fs::write(staging.join(STAGING_MARKER), STAGING_MARKER_CONTENT).unwrap();
        fs::write(staging.join("partial"), b"partial").unwrap();
        commit_generated_tree(
            root.path(),
            Path::new("baml_client"),
            &tree(&[("Value.g.cs", "class Value {}")]),
        )
        .unwrap();
        assert!(!staging.exists());

        let user_root = TempDir::new().unwrap();
        let user_output = user_root.path().join("baml_client");
        fs::create_dir(&user_output).unwrap();
        fs::write(user_output.join("User.cs"), b"class User {}").unwrap();
        assert!(matches!(
            commit_generated_tree(user_root.path(), Path::new("baml_client"), &tree(&[("Value.g.cs", "class Value {}")])),
            Err(TransactionError::UnownedPath(path)) if path == user_output
        ));
        assert!(user_output.join("User.cs").exists());
    }

    #[test]
    fn markerless_staging_requires_a_fully_valid_owned_tree() {
        let root = TempDir::new().unwrap();
        let staging = root.path().join(".baml_client.baml-staging");
        fs::create_dir(&staging).unwrap();
        fs::write(staging.join(MANIFEST_FILE), b"not a generator manifest\n").unwrap();
        let user_file = staging.join("User.cs");
        fs::write(&user_file, b"class User {}\n").unwrap();
        assert!(
            commit_generated_tree(
                root.path(),
                Path::new("baml_client"),
                &tree(&[("Value.g.cs", "class Value {}")]),
            )
            .is_err()
        );
        assert_eq!(fs::read(&user_file).unwrap(), b"class User {}\n");

        let valid_root = TempDir::new().unwrap();
        let valid_staging = valid_root.path().join(".baml_client.baml-staging");
        let generated = tree(&[("Value.g.cs", "class Value {}")]);
        let manifest = validate_tree(&generated).unwrap();
        write_staging_tree(&valid_staging, &generated, &manifest).unwrap();
        fs::remove_file(valid_staging.join(STAGING_MARKER)).unwrap();
        commit_generated_tree(valid_root.path(), Path::new("baml_client"), &generated).unwrap();
        assert!(!valid_staging.exists());
        assert!(valid_root.path().join("baml_client/Value.g.cs").is_file());
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn concurrent_writer_is_rejected_without_touching_output() {
        use fs2::FileExt as _;

        let root = TempDir::new().unwrap();
        let canonical_parent = fs::canonicalize(root.path()).unwrap();
        let lock_directory = std::env::temp_dir().join("baml-sdkgen-csharp-locks");
        fs::create_dir_all(&lock_directory).unwrap();
        let lock_path = generation_lock_path(&lock_directory, &canonical_parent, "baml_client");
        let lock = fs::OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(&lock_path)
            .unwrap();
        lock.try_lock_exclusive().unwrap();

        let result = commit_generated_tree(
            root.path(),
            Path::new("baml_client"),
            &tree(&[("Value.g.cs", "class Value {}")]),
        );
        assert!(matches!(
            result,
            Err(TransactionError::Io {
                operation: "acquire generation lock",
                ..
            })
        ));
        assert!(!root.path().join("baml_client").exists());
    }

    #[test]
    fn case_variant_targets_share_one_external_writer_lock() {
        let root = TempDir::new().unwrap();
        let canonical_parent = fs::canonicalize(root.path()).unwrap();
        let lock_directory = std::env::temp_dir().join("baml-sdkgen-csharp-locks");
        assert_eq!(
            generation_lock_path(&lock_directory, &canonical_parent, "baml_client"),
            generation_lock_path(&lock_directory, &canonical_parent, "BAML_CLIENT")
        );
    }

    #[test]
    fn staged_inventory_and_hashes_are_revalidated_from_disk() {
        let root = TempDir::new().unwrap();
        let staging = root.path().join("stage");
        let generated = tree(&[("Value.g.cs", "class Value {}")]);
        let manifest = validate_tree(&generated).unwrap();
        write_staging_tree(&staging, &generated, &manifest).unwrap();
        validate_staged_tree(&staging, &manifest).unwrap();

        fs::write(
            staging.join("Value.g.cs"),
            format!("{HEADER}class Edited {{}}\n"),
        )
        .unwrap();
        assert!(matches!(
            validate_staged_tree(&staging, &manifest),
            Err(TransactionError::InvalidGeneratedSource(_, _))
        ));
        fs::write(
            staging.join("Extra.g.cs"),
            format!("{HEADER}class Extra {{}}\n"),
        )
        .unwrap();
        assert!(matches!(
            validate_staged_tree(&staging, &manifest),
            Err(TransactionError::InvalidMetadata(
                "staged file inventory differs from manifest"
            ))
        ));
    }

    #[test]
    fn invalid_inputs_fail_before_touching_the_last_output() {
        let root = TempDir::new().unwrap();
        let output = Path::new("baml_client");
        commit_generated_tree(
            root.path(),
            output,
            &tree(&[("Value.g.cs", "class Original {}")]),
        )
        .unwrap();
        let before = fs::read(root.path().join(output).join("Value.g.cs")).unwrap();

        let mut recursive = tree(&[("Value.g.cs", "class Replacement {}")]);
        recursive.recursive_aliases = vec!["Node".to_string(), "Node".to_string()];
        assert_eq!(
            commit_generated_tree(root.path(), output, &recursive)
                .unwrap_err()
                .to_string(),
            "recursive aliases are unsupported by C# generation: Node"
        );

        let collision = tree(&[("Foo.g.cs", "class Foo {}"), ("foo.g.cs", "class Foo2 {}")]);
        assert!(matches!(
            commit_generated_tree(root.path(), output, &collision),
            Err(TransactionError::DuplicateCaseInsensitivePath(_))
        ));
        assert_eq!(
            fs::read(root.path().join(output).join("Value.g.cs")).unwrap(),
            before
        );

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
            assert!(commit_generated_tree(root.path(), output, &invalid_tree).is_err());
        }
        assert_eq!(
            fs::read(root.path().join(output).join("Value.g.cs")).unwrap(),
            before
        );

        let mut corrupt_fingerprint = tree(&[("Value.g.cs", "class Replacement {}")]);
        corrupt_fingerprint.metadata.program_fingerprint = "0".repeat(64);
        assert!(matches!(
            commit_generated_tree(root.path(), output, &corrupt_fingerprint),
            Err(TransactionError::ProgramFingerprintMismatch { .. })
        ));
        let mut empty_bytecode = tree(&[("Value.g.cs", "class Replacement {}")]);
        empty_bytecode.program_bytes.clear();
        assert!(matches!(
            commit_generated_tree(root.path(), output, &empty_bytecode),
            Err(TransactionError::InvalidMetadata(
                "program bytecode must be non-empty"
            ))
        ));
        let mut empty_inventory = tree(&[]);
        assert!(matches!(
            commit_generated_tree(root.path(), output, &empty_inventory),
            Err(TransactionError::InvalidMetadata(
                "generated file inventory must be non-empty"
            ))
        ));
        empty_inventory.files = tree(&[("Value.g.cs", "class Replacement {}")]).files;
        empty_inventory.metadata.cli_version.clear();
        assert!(matches!(
            commit_generated_tree(root.path(), output, &empty_inventory),
            Err(TransactionError::InvalidMetadata(
                "versions and program identity must be non-empty"
            ))
        ));
        assert_eq!(
            fs::read(root.path().join(output).join("Value.g.cs")).unwrap(),
            before
        );
    }

    #[test]
    fn manifest_paths_hashes_and_boundary_are_deterministic() {
        let root = TempDir::new().unwrap();
        let generated = tree(&[
            ("Zed.g.cs", "class Zed {}"),
            ("Acme/Alpha.g.cs", "class Alpha {}"),
        ]);
        let manifest =
            commit_generated_tree(root.path(), Path::new("baml_client"), &generated).unwrap();
        assert_eq!(
            manifest
                .files
                .iter()
                .map(|entry| entry.relative_path.as_str())
                .collect::<Vec<_>>(),
            ["Acme/Alpha.g.cs", "Zed.g.cs"]
        );
        assert!(manifest.files.iter().all(|entry| entry.sha256.len() == 64));
        assert!(matches!(
            commit_generated_tree(root.path(), Path::new("../outside"), &generated),
            Err(TransactionError::UnsafeOutputPath(_))
        ));
        assert!(!root.path().parent().unwrap().join("outside").exists());
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_output_boundaries_are_rejected() {
        use std::os::unix::fs::symlink;

        let root = TempDir::new().unwrap();
        let outside = TempDir::new().unwrap();
        symlink(outside.path(), root.path().join("linked")).unwrap();
        let generated = tree(&[("Value.g.cs", "class Value {}")]);
        assert!(matches!(
            commit_generated_tree(root.path(), Path::new("linked/baml_client"), &generated),
            Err(TransactionError::UnsafeOutputPath(_))
        ));
        assert!(!outside.path().join("baml_client").exists());

        let external_output = outside.path().join("owned");
        commit_generated_tree(outside.path(), Path::new("owned"), &generated).unwrap();
        symlink(&external_output, root.path().join("baml_client")).unwrap();
        assert!(matches!(
            commit_generated_tree(root.path(), Path::new("baml_client"), &generated),
            Err(TransactionError::UnownedPath(_))
        ));
    }
}
