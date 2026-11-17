//! Program and package content identity.
//!
//! Lives in the pure data layer beside [`crate::Program`]'s
//! `source_content_hash` field: the compiler stamps it at compile time
//! and hosts restamp it when materializing a program from stored bytes
//! (the field is `borsh(skip)` — in-memory metadata, not compiled
//! content).
//!
//! A package has no persistent identity (BEP-085): at run time it is a heap
//! object, and a name reaches it only along an edge. What persists is what
//! the hashes here say about its content, and the claim its manifest makes.
//! Nothing in the compiler or runtime keys a package by either; they are
//! carried for tooling and telemetry to correlate builds by.

use baml_base::ClaimedIdentity;
use borsh::{BorshDeserialize, BorshSerialize};

macro_rules! content_hash {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, BorshSerialize, BorshDeserialize)]
        pub struct $name(pub [u8; 32]);

        impl std::fmt::Debug for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, concat!(stringify!($name), "({})"), self)
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                self.0.iter().try_for_each(|byte| write!(f, "{byte:02x}"))
            }
        }
    };
}

content_hash! {
    /// Hash of a package's public API: the payload of the interface its
    /// dependents compile against. Covers the package's claimed identity, the
    /// compiler version, and the interface hash of every direct dependency.
    PackageInterfaceHash
}

content_hash! {
    /// Hash of a full compiled package, function bodies included
    /// ([`CompilationUnitHasher`]).
    CompilationUnitHash
}

content_hash! {
    /// Hash of a package's formatted, normalized source text
    /// ([`package_source_hash`]).
    SourceHash
}

/// What a compile records about the package it produced (BEP-085): the
/// content hashes, what it was compiled against, and what it claims to be.
/// A function of the package's sources, its claim, the compiler, and its
/// dependencies' interfaces, so equal builds record equal metadata.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PackageMetadata {
    pub compilation_unit_hash: CompilationUnitHash,
    pub package_interface: PackageInterfaceHash,
    /// The interface of every package this one reaches directly, the
    /// language packages included, in edge order, each once.
    pub dependencies: Vec<PackageInterfaceHash>,
    pub claimed: ClaimedIdentity,
    /// The compiler that built the package: `baml_version::CANONICAL_VERSION`.
    pub compiler_version: String,
}

/// Hashes a compiled package into its [`CompilationUnitHash`]:
///
/// ```text
/// SHA-256( "baml-compilation-unit-v1"
///        ‖ framed(baml_version::CANONICAL_VERSION (utf8))
///        ‖ package interface hash
///        ‖ everything written: the package's compiled output, Borsh )
/// ```
///
/// The interface hash carries the claimed identity and the dependencies'
/// interfaces in, so the unit hash moves with either. The compiled output is
/// hashed in a machine-independent form ([`normalize_for_unit_hash`]),
/// without its source positions: they are numbered by the database that
/// compiled it, and they move with whitespace and comments, neither of
/// which is the package's content.
pub struct CompilationUnitHasher(sha2::Sha256);

impl CompilationUnitHasher {
    #[must_use]
    pub fn new(interface: PackageInterfaceHash) -> Self {
        use sha2::Digest;
        let mut hasher = sha2::Sha256::new();
        hasher.update(b"baml-compilation-unit-v1");
        update_framed(&mut hasher, baml_version::CANONICAL_VERSION.as_bytes());
        hasher.update(interface.0);
        Self(hasher)
    }

    #[must_use]
    pub fn finish(self) -> CompilationUnitHash {
        use sha2::Digest;
        CompilationUnitHash(self.0.finalize().into())
    }
}

impl std::io::Write for CompilationUnitHasher {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        use sha2::Digest;
        self.0.update(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Put each function of `objects` in the form its package is hashed in.
///
/// Where in the source it came from is removed: its span, its line tables,
/// and the spans its debug locals are in scope for. A file is numbered by
/// the database that compiled it, and a position moves with every edit above
/// it, so neither belongs in a hash of the compiled package. The path of its
/// source file is spelled with `/`, whatever separator the compiling
/// platform displays paths with.
pub fn normalize_for_unit_hash(objects: &mut [crate::Object]) {
    let nowhere = baml_base::Span::default();
    for object in objects {
        let crate::Object::Function(function) = object else {
            continue;
        };
        if function.source_file.contains('\\') {
            function.source_file = function.source_file.replace('\\', "/");
        }
        function.span = nowhere;
        function.bytecode.line_table.clear();
        if let Some(compact) = &mut function.bytecode.compact {
            compact.line_table.clear();
        }
        for local in &mut function.debug_locals {
            local.scope_span = nowhere;
        }
    }
}

/// The hash of a package's source text (BEP-088):
///
/// ```text
/// SHA-256( "baml-package-source-v1"
///        ‖ for each file, sorted by path bytes ascending:
///            framed(path (utf8)) ‖ framed(file bytes) )
/// ```
///
/// The caller passes paths and text already normalized (package-relative
/// `/`-separated paths, formatted text with `\n` line endings), so equal
/// code hashes equally on every machine. The compiler version is not an
/// input: the same text under two compilers is the same source.
#[must_use]
pub fn package_source_hash<'a>(files: impl IntoIterator<Item = (&'a str, &'a [u8])>) -> SourceHash {
    use sha2::Digest;
    let mut sorted: Vec<(&str, &[u8])> = files.into_iter().collect();
    sorted.sort_by(|left, right| left.0.as_bytes().cmp(right.0.as_bytes()));
    let mut hasher = sha2::Sha256::new();
    hasher.update(b"baml-package-source-v1");
    for (path, bytes) in sorted {
        update_framed(&mut hasher, path.as_bytes());
        update_framed(&mut hasher, bytes);
    }
    SourceHash(hasher.finalize().into())
}

/// One package of a linked program, as [`linked_program_id`] reads it: what
/// was compiled, and which packages its edges were bound to.
#[derive(Clone, Copy, Debug)]
pub struct LinkedPackage<'a> {
    pub unit: CompilationUnitHash,
    pub edges: &'a [crate::types::ProgramEdge],
}

/// The identity of a linked program: a function of what was linked, so one
/// link set is one program however many times it is linked, cached, or run.
///
/// ```text
/// first 16 bytes of SHA-256( "baml-linked-program-v1"
///                          ‖ root ordinal as u32 big-endian
///                          ‖ package count as u64 big-endian
///                          ‖ for each package, in program order:
///                              compilation unit hash
///                            ‖ edge count as u64 big-endian
///                            ‖ for each edge, in table order:
///                                framed(name (utf8))
///                              ‖ target ordinal as u32 big-endian
///                              ‖ kind (0 dependency, 1 re-exported) )
/// ```
///
/// A unit hash covers the interfaces a package was compiled against, not
/// which packages its edges were bound to: two packages with one interface
/// are interchangeable to it. The edges say which was linked.
///
/// with the version and variant bits of a `UUIDv8` (RFC 9562's custom
/// layout), so it never collides with the random `UUIDv7` a program gets
/// when it has no content identity.
#[must_use]
pub fn linked_program_id(root: u32, packages: &[LinkedPackage<'_>]) -> [u8; 16] {
    use sha2::Digest;
    let count = |len: usize| u64::try_from(len).unwrap_or(u64::MAX).to_be_bytes();
    let mut hasher = sha2::Sha256::new();
    hasher.update(b"baml-linked-program-v1");
    hasher.update(root.to_be_bytes());
    hasher.update(count(packages.len()));
    for package in packages {
        hasher.update(package.unit.0);
        hasher.update(count(package.edges.len()));
        for edge in package.edges {
            update_framed(&mut hasher, edge.name.as_str().as_bytes());
            hasher.update(edge.target.to_be_bytes());
            hasher.update([match edge.kind {
                crate::types::EdgeKind::Dependency => 0u8,
                crate::types::EdgeKind::ReExported => 1,
            }]);
        }
    }
    let digest: [u8; 32] = hasher.finalize().into();
    let mut id = [0u8; 16];
    id.copy_from_slice(&digest[..16]);
    id[6] = (id[6] & 0x0f) | 0x80;
    id[8] = (id[8] & 0x3f) | 0x80;
    id
}

/// `len(x)` as u64 big-endian, then `x`.
fn update_framed(hasher: &mut sha2::Sha256, bytes: &[u8]) {
    use sha2::Digest;
    hasher.update(u64::try_from(bytes.len()).unwrap_or(u64::MAX).to_be_bytes());
    hasher.update(bytes);
}

/// The conservative source-content program hash (profiling streams spec
/// §2.3):
///
/// ```text
/// SHA-256( "baml-program-v1"
///        ‖ framed(baml_version::CANONICAL_VERSION (utf8))
///        ‖ for each file, sorted by path bytes ascending:
///            framed(path (utf8)) ‖ framed(file bytes) )
///
/// where framed(x) = len(x) as u64 big-endian ‖ x
/// ```
///
/// Every variable-length field carries its own length, so no two distinct
/// inputs share a byte stream: a delimiter alone would not do, since path
/// and file bytes may contain any byte value.
///
/// Any byte change in any compiled file — comments and whitespace included —
/// or a compiler version change yields a new hash. Two byte-identical builds
/// in different processes produce identical hashes, so profiling
/// `ContextKey`s are comparable across executions of one build. A later
/// semantic hash must use a NEW domain string, never `"baml-program-v1"`.
#[must_use]
pub fn program_content_hash<'a>(files: impl IntoIterator<Item = (&'a str, &'a [u8])>) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let mut sorted: Vec<(&str, &[u8])> = files.into_iter().collect();
    sorted.sort_by(|left, right| left.0.as_bytes().cmp(right.0.as_bytes()));
    let mut hasher = Sha256::new();
    hasher.update(b"baml-program-v1");
    let mut framed = |bytes: &[u8]| {
        hasher.update(u64::try_from(bytes.len()).unwrap_or(u64::MAX).to_be_bytes());
        hasher.update(bytes);
    };
    framed(baml_version::CANONICAL_VERSION.as_bytes());
    for (path, bytes) in sorted {
        framed(path.as_bytes());
        framed(bytes);
    }
    hasher.finalize().into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn program_content_hash_is_order_independent_and_byte_sensitive() {
        let base = program_content_hash([
            ("b.baml", b"beta".as_slice()),
            ("a.baml", b"alpha".as_slice()),
        ]);
        let reordered = program_content_hash([
            ("a.baml", b"alpha".as_slice()),
            ("b.baml", b"beta".as_slice()),
        ]);
        assert_eq!(base, reordered, "input order must not matter");
        let comment_flip = program_content_hash([
            ("a.baml", b"alpha ".as_slice()),
            ("b.baml", b"beta".as_slice()),
        ]);
        assert_ne!(base, comment_flip, "any byte change splits the identity");
        let path_flip = program_content_hash([
            ("a2.baml", b"alpha".as_slice()),
            ("b.baml", b"beta".as_slice()),
        ]);
        assert_ne!(base, path_flip, "path bytes are part of the identity");
        // Cross-platform golden for the formula (pinned to CANONICAL_VERSION
        // "0.17.0"; regenerate deliberately on a version bump).
        if baml_version::CANONICAL_VERSION == "0.17.0" {
            assert_eq!(
                hex::encode(base),
                "e98260b82b3b024bcc7d3b56fee3632a840cc8b05abf9ebcbd28882b16ef3049"
            );
        }
    }

    #[test]
    fn version_and_first_path_cannot_be_confused() {
        use sha2::{Digest, Sha256};

        // Recomputes the documented formula for one file, with the version
        // as a parameter (`CANONICAL_VERSION` is a constant, so the real
        // entry point cannot vary it).
        fn formula(version: &str, path: &str, bytes: &[u8]) -> [u8; 32] {
            let mut hasher = Sha256::new();
            hasher.update(b"baml-program-v1");
            for field in [version.as_bytes(), path.as_bytes(), bytes] {
                hasher.update((field.len() as u64).to_be_bytes());
                hasher.update(field);
            }
            hasher.finalize().into()
        }

        // Ties the formula above to the shipping one: if `program_content_hash`
        // stops framing the version, this equality breaks.
        assert_eq!(
            program_content_hash([("a.baml", b"alpha".as_slice())]),
            formula(baml_version::CANONICAL_VERSION, "a.baml", b"alpha"),
            "the entry point must hash the documented framing"
        );

        // Unframed, "0.17.1" + "0a.baml" and "0.17.10" + "a.baml" concatenate
        // to identical bytes.
        assert_ne!(
            formula("0.17.1", "0a.baml", b"same"),
            formula("0.17.10", "a.baml", b"same"),
            "a version boundary must not be absorbed into the first path"
        );
    }

    #[test]
    fn package_source_hash_is_order_independent_and_version_free() {
        use sha2::{Digest, Sha256};

        let files = [
            ("b.baml", b"beta".as_slice()),
            ("a.baml", b"alpha".as_slice()),
        ];
        let base = package_source_hash(files);
        assert_eq!(
            base,
            package_source_hash([files[1], files[0]]),
            "input order must not matter"
        );
        assert_ne!(
            base,
            package_source_hash([files[0], ("a.baml", b"alpha ".as_slice())])
        );
        assert_ne!(
            package_source_hash([("a\0b.baml", b"c".as_slice())]),
            package_source_hash([("a.baml", b"\0bc".as_slice())]),
            "field boundaries must not depend on the content bytes"
        );

        // The documented formula, which names no compiler version.
        let mut hasher = Sha256::new();
        hasher.update(b"baml-package-source-v1");
        for field in [b"a.baml".as_slice(), b"alpha", b"b.baml", b"beta"] {
            hasher.update((field.len() as u64).to_be_bytes());
            hasher.update(field);
        }
        assert_eq!(base.0, <[u8; 32]>::from(hasher.finalize()));
    }

    #[test]
    fn linked_program_id_is_a_uuid_v8_of_the_link_set() {
        use crate::types::{EdgeKind, ProgramEdge};

        let a = CompilationUnitHash([1; 32]);
        let b = CompilationUnitHash([2; 32]);
        let edge = |name: &str, target, kind| ProgramEdge {
            name: baml_base::Name::new(name),
            target,
            kind,
        };
        let to_first = [edge("lib", 0, EdgeKind::Dependency)];
        let id = |root, packages: &[(CompilationUnitHash, &[ProgramEdge])]| {
            let packages: Vec<_> = packages
                .iter()
                .map(|&(unit, edges)| LinkedPackage { unit, edges })
                .collect();
            linked_program_id(root, &packages)
        };
        let base = id(1, &[(a, &[]), (b, &to_first)]);
        assert_eq!(base, id(1, &[(a, &[]), (b, &to_first)]));
        assert_ne!(base, id(0, &[(a, &[]), (b, &to_first)]), "the root");
        assert_ne!(base, id(1, &[(b, &to_first), (a, &[])]), "the order");
        assert_ne!(base, id(1, &[(a, &[])]));
        // The same units bound differently are another program.
        for other in [
            edge("lib", 1, EdgeKind::Dependency),
            edge("other", 0, EdgeKind::Dependency),
            edge("lib", 0, EdgeKind::ReExported),
        ] {
            let rebound = std::slice::from_ref(&other);
            assert_ne!(base, id(1, &[(a, &[]), (b, rebound)]), "{other:?}");
        }
        assert_ne!(base, id(1, &[(a, &to_first), (b, &[])]), "the edge's owner");
        assert_eq!(base[6] >> 4, 8, "version 8");
        assert_eq!(base[8] >> 6, 0b10, "RFC 9562 variant");
    }

    #[test]
    fn unit_hash_form_drops_positions_and_spells_paths_with_slashes() {
        let function = |source_file: &str, file: u32| {
            let mut function = crate::relink::tests::test_function();
            function.source_file = source_file.to_owned();
            function.span.file_id = baml_base::FileId::new(file);
            let mut objects = [crate::Object::Function(Box::new(function))];
            normalize_for_unit_hash(&mut objects);
            borsh::to_vec(&objects).unwrap()
        };
        assert_eq!(
            function("ns_a\\main.baml", 3),
            function("ns_a/main.baml", 9)
        );
        assert_ne!(function("ns_a/main.baml", 3), function("ns_b/main.baml", 3));
    }

    #[test]
    fn compilation_unit_hash_covers_interface_and_content() {
        use std::io::Write as _;

        let hash = |interface: [u8; 32], content: &[u8]| {
            let mut hasher = CompilationUnitHasher::new(PackageInterfaceHash(interface));
            hasher.write_all(content).unwrap();
            hasher.finish()
        };
        let base = hash([1; 32], b"unit");
        assert_eq!(base, hash([1; 32], b"unit"));
        assert_ne!(base, hash([2; 32], b"unit"), "the interface is an input");
        assert_ne!(base, hash([1; 32], b"unit2"), "the content is an input");
        assert_eq!(
            format!("{:?}", PackageInterfaceHash([0xab; 32])),
            format!("PackageInterfaceHash({})", "ab".repeat(32))
        );
    }

    #[test]
    fn path_and_content_boundaries_cannot_be_confused() {
        // Length framing (unlike a NUL delimiter) also survives paths and
        // file bytes that themselves contain the delimiter byte.
        assert_ne!(
            program_content_hash([("a\0b.baml", b"c".as_slice())]),
            program_content_hash([("a.baml", b"\0bc".as_slice())]),
            "field boundaries must not depend on the content bytes"
        );
    }
}
