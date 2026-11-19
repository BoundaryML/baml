//! A compile described as a build (BEP-088): the row BCS keeps for it, the
//! package it produced, the program it linked and the source it read.

use baml_db::{ProjectDatabase, SourceRoot};
use baml_fmt::package_source::{PackageSourceError, package_source};
use bex_vm_types::{Program, identity::PackageMetadata};
use btel_types::{BuildOrigin, BuildRecord, BuildRegistration, GitInfo, ProgramRecord};

/// What a build is beyond the package it compiled.
pub struct BuildFacts {
    pub build_id: [u8; 16],
    pub timestamp_unix_ns: i64,
    pub origin: BuildOrigin,
    /// `None` when the package is not in a git repository.
    pub git: Option<GitInfo>,
    /// `None` when the build linked no program.
    pub program: Option<ProgramRecord>,
}

/// A fresh build ID: a random UUID.
#[must_use]
pub fn new_build_id() -> [u8; 16] {
    *uuid::Uuid::new_v4().as_bytes()
}

#[must_use]
pub fn unix_now_ns() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .and_then(|elapsed| i64::try_from(elapsed.as_nanos()).ok())
        .unwrap_or(0)
}

/// The build of `root`, whose compile recorded `metadata`: formats and
/// hashes every source file of the package.
///
/// # Errors
///
/// The package's source has no normalized form ([`PackageSourceError`]).
pub fn registration(
    db: &ProjectDatabase,
    root: SourceRoot,
    metadata: &PackageMetadata,
    facts: BuildFacts,
) -> Result<BuildRegistration, PackageSourceError> {
    let source = package_source(db, root)?;
    let source_hash = bex_vm_types::identity::package_source_hash(
        source
            .files
            .iter()
            .map(|(path, text)| (path.as_str(), text.as_bytes())),
    );
    Ok(BuildRegistration {
        build: BuildRecord {
            build_id: facts.build_id,
            timestamp_unix_ns: facts.timestamp_unix_ns,
            origin: facts.origin,
            git: facts.git,
            source_hash: source_hash.0,
            compilation_unit_hash: metadata.compilation_unit_hash.0,
        },
        package: metadata.record(),
        program: facts.program,
        source: Some(source.files),
    })
}

/// `program` as BCS records it: `None` when a package in it carries no
/// metadata, which leaves the program without an identity.
#[must_use]
pub fn program_record(program: &Program) -> Option<ProgramRecord> {
    Some(ProgramRecord {
        program_id: program.linked_id()?,
        static_packages: program
            .packages
            .iter()
            .map(|package| Some(package.metadata.as_ref()?.compilation_unit_hash.0))
            .collect::<Option<_>>()?,
    })
}
