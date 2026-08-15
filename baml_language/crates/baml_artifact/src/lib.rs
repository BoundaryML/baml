//! Versioned, sectioned compiled-package artifacts.

use std::collections::BTreeSet;

use baml_base::Name;
use baml_package_interface::PackageInterface;
use bex_vm_types::unit::CompilationUnit;
use borsh::{BorshDeserialize, BorshSerialize};
use sha2::{Digest, Sha256};

const MAGIC: &[u8; 8] = b"BAMLART\0";
const FORMAT_VERSION: u32 = 1;
pub const BYTECODE_ABI: u32 = 1;
const PREFIX_LEN: usize = MAGIC.len() + size_of::<u32>() + size_of::<u64>();

include!(concat!(env!("OUT_DIR"), "/build_id.rs"));

pub type Hash = [u8; 32];

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PackageIdentity {
    pub name: Name,
    pub source_hash: Hash,
    pub interface_hash: Hash,
}

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct DependencyRequirement {
    pub name: Name,
    pub interface_hash: Hash,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, BorshSerialize, BorshDeserialize)]
pub enum SectionKind {
    Interface,
    Code,
    Tooling,
    Sources,
}

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct SectionEntry {
    pub kind: SectionKind,
    pub offset: u64,
    pub len: u64,
    pub hash: Hash,
}

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ArtifactHeader {
    pub compiler_build_id: Hash,
    pub bytecode_abi: u32,
    pub package: PackageIdentity,
    pub dependencies: Vec<DependencyRequirement>,
    pub sections: Vec<SectionEntry>,
}

#[derive(Debug, Clone, BorshSerialize, BorshDeserialize)]
pub struct PackageCode {
    pub units: Vec<CompilationUnit>,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid artifact magic")]
    InvalidMagic,
    #[error("unsupported artifact format version {0}")]
    UnsupportedVersion(u32),
    #[error("artifact is truncated")]
    Truncated,
    #[error("invalid artifact header: {0}")]
    InvalidHeader(String),
    #[error("duplicate artifact section {0:?}")]
    DuplicateSection(SectionKind),
    #[error("artifact sections overlap or are out of order")]
    InvalidSectionOrder,
    #[error("artifact section {0:?} is missing")]
    MissingSection(SectionKind),
    #[error("artifact section {0:?} failed integrity validation")]
    InvalidSectionHash(SectionKind),
    #[error("artifact was built by a different compiler")]
    CompilerBuildMismatch,
    #[error("artifact bytecode ABI {actual} does not match expected ABI {expected}")]
    BytecodeAbiMismatch { expected: u32, actual: u32 },
    #[error("duplicate compiled package {0}")]
    DuplicatePackage(Name),
    #[error("compiled package {package} requires missing package {dependency}")]
    MissingDependency { package: Name, dependency: Name },
    #[error("compiled package {package} requires a different interface for {dependency}")]
    DependencyInterfaceMismatch { package: Name, dependency: Name },
    #[error("compiled package dependency graph contains a cycle at {0}")]
    DependencyCycle(Name),
    #[error("could not link compiled packages: {0}")]
    Link(String),
    #[error("could not encode artifact: {0}")]
    Encode(String),
    #[error("could not decode artifact section {section:?}: {message}")]
    Decode {
        section: SectionKind,
        message: String,
    },
}

#[derive(Debug, Clone)]
#[must_use]
pub struct ArtifactBuilder {
    compiler_build_id: Hash,
    bytecode_abi: u32,
    package_name: Name,
    source_hash: Hash,
    dependencies: Vec<DependencyRequirement>,
    sections: Vec<(SectionKind, Vec<u8>)>,
}

impl ArtifactBuilder {
    pub fn new(
        compiler_build_id: Hash,
        bytecode_abi: u32,
        package_name: Name,
        source_hash: Hash,
    ) -> Self {
        Self {
            compiler_build_id,
            bytecode_abi,
            package_name,
            source_hash,
            dependencies: Vec::new(),
            sections: Vec::new(),
        }
    }

    pub fn dependencies(mut self, dependencies: Vec<DependencyRequirement>) -> Self {
        self.dependencies = dependencies;
        self
    }

    pub fn interface(mut self, interface: &PackageInterface) -> Result<Self, Error> {
        self.push_encoded(SectionKind::Interface, interface)?;
        Ok(self)
    }

    pub fn code(mut self, code: &PackageCode) -> Result<Self, Error> {
        self.push_encoded(SectionKind::Code, code)?;
        Ok(self)
    }

    pub fn tooling(mut self, bytes: Vec<u8>) -> Self {
        self.push_raw(SectionKind::Tooling, bytes);
        self
    }

    pub fn sources(mut self, bytes: Vec<u8>) -> Self {
        self.push_raw(SectionKind::Sources, bytes);
        self
    }

    pub fn finish(mut self) -> Result<Vec<u8>, Error> {
        self.sections.sort_by_key(|(kind, _)| *kind);
        validate_unique_sections(self.sections.iter().map(|(kind, _)| *kind))?;

        let interface = self
            .sections
            .iter()
            .find(|(kind, _)| *kind == SectionKind::Interface)
            .ok_or(Error::MissingSection(SectionKind::Interface))?;

        let mut header = ArtifactHeader {
            compiler_build_id: self.compiler_build_id,
            bytecode_abi: self.bytecode_abi,
            package: PackageIdentity {
                name: self.package_name,
                source_hash: self.source_hash,
                interface_hash: hash(&interface.1),
            },
            dependencies: self.dependencies,
            sections: self
                .sections
                .iter()
                .map(|(kind, bytes)| SectionEntry {
                    kind: *kind,
                    offset: 0,
                    len: bytes.len() as u64,
                    hash: hash(bytes),
                })
                .collect(),
        };

        let placeholder_header = borsh::to_vec(&header).map_err(|error| encode_error(&error))?;
        let mut offset = PREFIX_LEN
            .checked_add(placeholder_header.len())
            .ok_or(Error::Truncated)? as u64;
        for entry in &mut header.sections {
            entry.offset = offset;
            offset = offset.checked_add(entry.len).ok_or(Error::Truncated)?;
        }
        let header_bytes = borsh::to_vec(&header).map_err(|error| encode_error(&error))?;
        if header_bytes.len() != placeholder_header.len() {
            return Err(Error::InvalidHeader(
                "header size changed while assigning offsets".into(),
            ));
        }

        let capacity = usize::try_from(offset).map_err(|_| Error::Truncated)?;
        let mut artifact = Vec::with_capacity(capacity);
        artifact.extend_from_slice(MAGIC);
        artifact.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
        artifact.extend_from_slice(&(header_bytes.len() as u64).to_le_bytes());
        artifact.extend_from_slice(&header_bytes);
        for (_, bytes) in self.sections {
            artifact.extend_from_slice(&bytes);
        }
        Ok(artifact)
    }

    fn push_encoded<T: BorshSerialize>(
        &mut self,
        kind: SectionKind,
        value: &T,
    ) -> Result<(), Error> {
        self.push_raw(
            kind,
            borsh::to_vec(value).map_err(|error| encode_error(&error))?,
        );
        Ok(())
    }

    fn push_raw(&mut self, kind: SectionKind, bytes: Vec<u8>) {
        self.sections.push((kind, bytes));
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ArtifactReader<'a> {
    bytes: &'a [u8],
    header: &'a ArtifactHeader,
}

impl<'a> ArtifactReader<'a> {
    pub fn parse(bytes: &'a [u8]) -> Result<OwnedArtifactReader<'a>, Error> {
        if bytes.len() < PREFIX_LEN {
            return Err(Error::Truncated);
        }
        if &bytes[..MAGIC.len()] != MAGIC {
            return Err(Error::InvalidMagic);
        }
        let version = u32::from_le_bytes(bytes[8..12].try_into().expect("fixed prefix"));
        if version != FORMAT_VERSION {
            return Err(Error::UnsupportedVersion(version));
        }
        let header_len = u64::from_le_bytes(bytes[12..20].try_into().expect("fixed prefix"));
        let header_end = PREFIX_LEN
            .checked_add(usize::try_from(header_len).map_err(|_| Error::Truncated)?)
            .ok_or(Error::Truncated)?;
        let header_bytes = bytes.get(PREFIX_LEN..header_end).ok_or(Error::Truncated)?;
        let header = ArtifactHeader::try_from_slice(header_bytes)
            .map_err(|error| Error::InvalidHeader(error.to_string()))?;
        validate_entries(&header, bytes.len(), header_end)?;
        Ok(OwnedArtifactReader { bytes, header })
    }

    pub fn header(&self) -> &ArtifactHeader {
        self.header
    }

    pub fn require_compatible(
        &self,
        compiler_build_id: Hash,
        bytecode_abi: u32,
    ) -> Result<(), Error> {
        if self.header.compiler_build_id != compiler_build_id {
            return Err(Error::CompilerBuildMismatch);
        }
        if self.header.bytecode_abi != bytecode_abi {
            return Err(Error::BytecodeAbiMismatch {
                expected: bytecode_abi,
                actual: self.header.bytecode_abi,
            });
        }
        Ok(())
    }

    pub fn section(&self, kind: SectionKind) -> Result<&'a [u8], Error> {
        let entry = self
            .header
            .sections
            .iter()
            .find(|entry| entry.kind == kind)
            .ok_or(Error::MissingSection(kind))?;
        let start = usize::try_from(entry.offset).map_err(|_| Error::Truncated)?;
        let len = usize::try_from(entry.len).map_err(|_| Error::Truncated)?;
        let end = start.checked_add(len).ok_or(Error::Truncated)?;
        let bytes = self.bytes.get(start..end).ok_or(Error::Truncated)?;
        if hash(bytes) != entry.hash {
            return Err(Error::InvalidSectionHash(kind));
        }
        Ok(bytes)
    }

    pub fn decode_interface(&self) -> Result<PackageInterface, Error> {
        self.decode(SectionKind::Interface)
    }

    pub fn decode_code(&self) -> Result<PackageCode, Error> {
        self.decode(SectionKind::Code)
    }

    fn decode<T: BorshDeserialize>(&self, kind: SectionKind) -> Result<T, Error> {
        T::try_from_slice(self.section(kind)?).map_err(|error| Error::Decode {
            section: kind,
            message: error.to_string(),
        })
    }
}

#[derive(Debug, Clone)]
pub struct OwnedArtifactReader<'a> {
    bytes: &'a [u8],
    header: ArtifactHeader,
}

impl OwnedArtifactReader<'_> {
    pub fn reader(&self) -> ArtifactReader<'_> {
        ArtifactReader {
            bytes: self.bytes,
            header: &self.header,
        }
    }

    pub fn header(&self) -> &ArtifactHeader {
        &self.header
    }
}

pub fn hash(bytes: &[u8]) -> Hash {
    Sha256::digest(bytes).into()
}

pub fn load_package_interfaces<'a>(
    artifacts: impl IntoIterator<Item = &'a [u8]>,
    compiler_build_id: Hash,
    bytecode_abi: u32,
) -> Result<std::collections::BTreeMap<Name, PackageInterface>, Error> {
    let mut headers = Vec::new();
    let mut interfaces = std::collections::BTreeMap::new();
    for bytes in artifacts {
        let owned = ArtifactReader::parse(bytes)?;
        let reader = owned.reader();
        reader.require_compatible(compiler_build_id, bytecode_abi)?;
        let header = reader.header().clone();
        let name = header.package.name.clone();
        if interfaces
            .insert(name.clone(), reader.decode_interface()?)
            .is_some()
        {
            return Err(Error::DuplicatePackage(name));
        }
        headers.push(header);
    }

    let interface_hashes = headers
        .iter()
        .map(|header| (header.package.name.clone(), header.package.interface_hash))
        .collect::<std::collections::BTreeMap<_, _>>();
    for header in headers {
        for dependency in header.dependencies {
            let Some(actual_hash) = interface_hashes.get(&dependency.name) else {
                return Err(Error::MissingDependency {
                    package: header.package.name,
                    dependency: dependency.name,
                });
            };
            if actual_hash != &dependency.interface_hash {
                return Err(Error::DependencyInterfaceMismatch {
                    package: header.package.name,
                    dependency: dependency.name,
                });
            }
        }
    }
    Ok(interfaces)
}

pub fn load_package_codes<'a>(
    artifacts: impl IntoIterator<Item = &'a [u8]>,
    compiler_build_id: Hash,
    bytecode_abi: u32,
) -> Result<Vec<(Name, PackageCode)>, Error> {
    let mut headers = std::collections::BTreeMap::new();
    let mut codes = std::collections::BTreeMap::new();
    for bytes in artifacts {
        let owned = ArtifactReader::parse(bytes)?;
        let reader = owned.reader();
        reader.require_compatible(compiler_build_id, bytecode_abi)?;
        let header = reader.header().clone();
        let name = header.package.name.clone();
        if headers.insert(name.clone(), header).is_some() {
            return Err(Error::DuplicatePackage(name));
        }
        codes.insert(name, reader.decode_code()?);
    }
    validate_dependencies(headers.values())?;

    let mut visiting = BTreeSet::new();
    let mut visited = BTreeSet::new();
    let mut order = Vec::new();
    for name in headers.keys() {
        visit_package(name, &headers, &mut visiting, &mut visited, &mut order)?;
    }
    Ok(order
        .into_iter()
        .map(|name| {
            let code = codes.remove(&name).expect("validated code package");
            (name, code)
        })
        .collect())
}

fn visit_package(
    name: &Name,
    headers: &std::collections::BTreeMap<Name, ArtifactHeader>,
    visiting: &mut BTreeSet<Name>,
    visited: &mut BTreeSet<Name>,
    order: &mut Vec<Name>,
) -> Result<(), Error> {
    if visited.contains(name) {
        return Ok(());
    }
    if !visiting.insert(name.clone()) {
        return Err(Error::DependencyCycle(name.clone()));
    }
    for dependency in &headers[name].dependencies {
        visit_package(&dependency.name, headers, visiting, visited, order)?;
    }
    visiting.remove(name);
    visited.insert(name.clone());
    order.push(name.clone());
    Ok(())
}

pub fn load_linked_program<'a>(
    artifacts: impl IntoIterator<Item = &'a [u8]>,
    compiler_build_id: Hash,
    bytecode_abi: u32,
) -> Result<bex_vm_types::Program, Error> {
    let packages = load_package_codes(artifacts, compiler_build_id, bytecode_abi)?
        .into_iter()
        .map(|(_, code)| code.units)
        .collect::<Vec<_>>();
    bex_vm_types::link::link_packages(&packages).map_err(|error| Error::Link(error.to_string()))
}

fn validate_dependencies<'a>(
    headers: impl IntoIterator<Item = &'a ArtifactHeader>,
) -> Result<(), Error> {
    let headers = headers.into_iter().collect::<Vec<_>>();
    let interface_hashes = headers
        .iter()
        .map(|header| (header.package.name.clone(), header.package.interface_hash))
        .collect::<std::collections::BTreeMap<_, _>>();
    for header in headers {
        for dependency in &header.dependencies {
            let Some(actual_hash) = interface_hashes.get(&dependency.name) else {
                return Err(Error::MissingDependency {
                    package: header.package.name.clone(),
                    dependency: dependency.name.clone(),
                });
            };
            if actual_hash != &dependency.interface_hash {
                return Err(Error::DependencyInterfaceMismatch {
                    package: header.package.name.clone(),
                    dependency: dependency.name.clone(),
                });
            }
        }
    }
    Ok(())
}

fn validate_unique_sections(kinds: impl IntoIterator<Item = SectionKind>) -> Result<(), Error> {
    let mut seen = BTreeSet::new();
    for kind in kinds {
        if !seen.insert(kind) {
            return Err(Error::DuplicateSection(kind));
        }
    }
    Ok(())
}

fn validate_entries(
    header: &ArtifactHeader,
    artifact_len: usize,
    header_end: usize,
) -> Result<(), Error> {
    validate_unique_sections(header.sections.iter().map(|entry| entry.kind))?;
    let mut previous_end = header_end as u64;
    for entry in &header.sections {
        if entry.offset < previous_end {
            return Err(Error::InvalidSectionOrder);
        }
        previous_end = entry
            .offset
            .checked_add(entry.len)
            .ok_or(Error::Truncated)?;
        if previous_end > artifact_len as u64 {
            return Err(Error::Truncated);
        }
    }
    Ok(())
}

fn encode_error(error: &std::io::Error) -> Error {
    Error::Encode(error.to_string())
}

#[cfg(test)]
mod tests {
    use baml_package_interface::FunctionThrowSets;
    use rustc_hash::FxHashMap;

    use super::*;

    fn interface() -> PackageInterface {
        PackageInterface {
            types: FxHashMap::default(),
            functions: FxHashMap::default(),
            impls: Vec::new(),
            throw_sets: FunctionThrowSets::default(),
        }
    }

    fn artifact_with_code() -> Vec<u8> {
        ArtifactBuilder::new([1; 32], 7, Name::new("example"), [2; 32])
            .interface(&interface())
            .unwrap()
            .code(&PackageCode { units: Vec::new() })
            .unwrap()
            .finish()
            .unwrap()
    }

    #[test]
    fn round_trips_sections_and_identity() {
        let bytes = artifact_with_code();
        let owned = ArtifactReader::parse(&bytes).unwrap();
        let reader = owned.reader();

        reader.require_compatible([1; 32], 7).unwrap();
        assert_eq!(reader.header().package.name, Name::new("example"));
        assert_eq!(reader.decode_interface().unwrap(), interface());
        assert!(reader.decode_code().unwrap().units.is_empty());
    }

    #[test]
    fn validates_sections_lazily() {
        let mut bytes = artifact_with_code();
        let owned = ArtifactReader::parse(&bytes).unwrap();
        let code_offset: usize = owned
            .header()
            .sections
            .iter()
            .find(|entry| entry.kind == SectionKind::Code)
            .unwrap()
            .offset
            .try_into()
            .unwrap();
        bytes[code_offset] ^= 1;

        let owned = ArtifactReader::parse(&bytes).unwrap();
        let reader = owned.reader();
        assert_eq!(reader.decode_interface().unwrap(), interface());
        assert!(matches!(
            reader.decode_code(),
            Err(Error::InvalidSectionHash(SectionKind::Code))
        ));
    }

    #[test]
    fn rejects_incompatible_compilers() {
        let bytes = artifact_with_code();
        let owned = ArtifactReader::parse(&bytes).unwrap();
        let reader = owned.reader();

        assert!(matches!(
            reader.require_compatible([9; 32], 7),
            Err(Error::CompilerBuildMismatch)
        ));
        assert!(matches!(
            reader.require_compatible([1; 32], 8),
            Err(Error::BytecodeAbiMismatch { .. })
        ));
    }

    #[test]
    fn validates_dependency_interfaces() {
        let dependency = ArtifactBuilder::new([1; 32], 7, Name::new("dep"), [0; 32])
            .interface(&interface())
            .unwrap()
            .code(&PackageCode { units: Vec::new() })
            .unwrap()
            .finish()
            .unwrap();
        let dependency_header = ArtifactReader::parse(&dependency).unwrap();
        let package = ArtifactBuilder::new([1; 32], 7, Name::new("package"), [0; 32])
            .dependencies(vec![DependencyRequirement {
                name: Name::new("dep"),
                interface_hash: dependency_header.header().package.interface_hash,
            }])
            .interface(&interface())
            .unwrap()
            .code(&PackageCode { units: Vec::new() })
            .unwrap()
            .finish()
            .unwrap();

        let loaded =
            load_package_interfaces([dependency.as_slice(), package.as_slice()], [1; 32], 7)
                .unwrap();
        assert_eq!(loaded.len(), 2);
        let codes =
            load_package_codes([package.as_slice(), dependency.as_slice()], [1; 32], 7).unwrap();
        assert_eq!(
            codes
                .iter()
                .map(|(name, _)| name.as_str())
                .collect::<Vec<_>>(),
            ["dep", "package"]
        );

        let missing = load_package_interfaces([package.as_slice()], [1; 32], 7);
        assert!(matches!(missing, Err(Error::MissingDependency { .. })));
    }
}
