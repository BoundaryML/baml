//! Compile-cache Salsa inputs read through [`crate::Db`] accessors.
//!
//! These are owner-set inputs that let a fresh database skip work a previous
//! compile already did (bytecode cache seeds) or resolve packages that have
//! no source in this database (mounted interface blobs). They live in this
//! crate — the bottom of the compiler2 `Db` trait chain — because their
//! payloads name `baml_type` types, which sit above `baml_base`.

/// Input: per-file `FunctionThrowFacts` from a previous compile, keyed by
/// the full source-file path string (`SourceFile::path` display form).
#[salsa::input]
pub struct SeededThrowFacts {
    #[returns(ref)]
    pub by_path:
        std::collections::BTreeMap<String, Vec<baml_type::throw_facts::FunctionThrowFacts>>,
}

/// Input: exact per-function `callable_throws` results from a previous compile,
/// keyed by source-file path string (`SourceFile::path` display form) then by
/// item-tree `LocalItemId::as_u32`.
///
/// Holds a typed `baml_type::Ty` — not opaque bytes like [`SeededStdlibInterface`]
/// — because `Ty` is a type this crate can already name (same as
/// [`SeededThrowFacts`]). The `LocalItemId` key is a content-derived,
/// process-independent item-tree index, so a byte-identical file's functions map
/// to the same keys across compiles. `callable_throws` reads it through a
/// *tracked* dependency (present-from-construction, empty until seeded), so a
/// later seed on a reused database invalidates the memo.
#[salsa::input]
pub struct SeededCallableThrows {
    #[returns(ref)]
    pub by_path: std::collections::BTreeMap<String, std::collections::BTreeMap<u32, baml_type::Ty>>,
}

/// Input: the stdlib packages' resolved `PackageInterface`s from a previous
/// compile, keyed by package name; each value is `borsh(PackageInterface)`.
///
/// The value is opaque bytes rather than the typed interface because
/// `PackageInterface` lives in `baml_compiler2_hir_ty`, which depends on this
/// crate — naming it here would be a dependency cycle. `baml_compiler2_hir_ty`
/// deserializes the relevant package's bytes on a seed hit. Per-package (not
/// whole-map) bytes keep the short-circuit's deserialize cost to one package
/// per query call. This mirrors [`SeededThrowFacts`], which holds a type this
/// crate can name; `PackageInterface` has no such low-crate home.
#[salsa::input]
pub struct SeededStdlibInterface {
    #[returns(ref)]
    pub by_package: std::collections::BTreeMap<String, Vec<u8>>,
}

/// Input: source-less dependency packages mounted as serialized
/// `PackageInterface` blobs, keyed by package name (the mount alias); each
/// value is `borsh(PackageInterface)`.
///
/// Opaque bytes for the same reason as [`SeededStdlibInterface`]:
/// `PackageInterface` lives in `baml_compiler2_hir_ty`, which depends on this
/// crate. It deserializes a package's bytes when its `package_interface` is
/// queried. A Salsa input, so mounting/unmounting a package invalidates
/// dependents for free (the B-694 delivery mechanism generalized to any
/// alias, per BEP-066 mounted-package linking).
#[salsa::input]
pub struct MountedPackages {
    #[returns(ref)]
    pub by_package: std::collections::BTreeMap<String, Vec<u8>>,

    /// Names whose blobs are compiler-built, image-immutable dependencies.
    ///
    /// This is deliberately metadata on the mounted-package transport rather
    /// than a second interface store: both ordinary runtime mounts and the
    /// precompiled stdlib use the same `PackageInterface` bytes, while the
    /// compiler can still keep immutable rows on its fact-free fast path.
    #[returns(ref)]
    pub immutable_precompiled: std::collections::BTreeSet<String>,
}
