//! The compiler-built stdlib slice, and how to derive one.
//!
//! Every BAML compile begins with the same embedded stdlib. Deriving it is the
//! dominant fixed cost of a compile, and it depends only on the compiler build
//! and the optimization level — no user file contributes to a stdlib package —
//! so it can be computed once per toolchain by a build script and served into
//! every compile afterwards.
//!
//! This module is the single producer. Its consumers each embed their own
//! artifact, because their requirements genuinely differ: `bex_project` ships
//! one optimization level in a production binary where size matters, while
//! `baml_tests` carries every level for tests, where it does not.
//! Sharing the *derivation* is what matters — an artifact that drifted from
//! what a real compile produces would be silently wrong in both.

use std::collections::BTreeMap;

use baml_base::{SourceRoot, SourceRootKind};
pub use baml_compiler2_emit::OptLevel;
use baml_compiler2_emit::emit_package;
use baml_linker_types::EmittedPackage;

use crate::{ProjectDatabase, program::PackageCache};

/// The compiler-built stdlib slice: every stdlib package's typed interface
/// alongside its compiled output, from those same sources.
///
/// Both halves depend only on the compiler build and the optimization level —
/// no user file contributes to a stdlib package — so one instance is valid for
/// every compile at that `opt`. Produce it with [`build_stdlib_prefix`].
///
/// The two halves must come from the same [`build_stdlib_prefix`] call: the
/// interfaces short-circuit type derivation while the outputs short-circuit
/// emit, and a user package emitted against interfaces the served outputs
/// were not compiled from would not link against them.
pub struct StdlibPrefix {
    /// Stdlib package name -> `borsh(PackageInterface)`.
    pub interfaces: BTreeMap<String, Vec<u8>>,
    /// Stdlib package name -> the package's output at `opt`.
    pub packages: BTreeMap<String, EmittedPackage>,
    /// The level `packages` were lowered at. The `testing` module's
    /// prefix-taking compile helpers assert the caller asked for the same one:
    /// the user package linked with served stdlib outputs must be lowered the
    /// same way they were.
    pub opt: OptLevel,
}

/// Each stdlib package's serialized typed interface, by package name.
pub fn stdlib_interfaces(db: &ProjectDatabase) -> BTreeMap<String, Vec<u8>> {
    use baml_compiler2_hir_ty::package_interface::export_interface;

    stdlib_roots(db)
        .map(|(name, root)| {
            let bytes = borsh::to_vec(&export_interface(db, root))
                .expect("a stdlib PackageInterface always serializes");
            (name, bytes)
        })
        .collect()
}

/// The stdlib roots of `db`, each by its fixed package name.
fn stdlib_roots(db: &ProjectDatabase) -> impl Iterator<Item = (String, SourceRoot)> + '_ {
    db.source_roots()
        .into_iter()
        .filter(|root| root.kind(db) == SourceRootKind::Stdlib)
        .map(|root| {
            let name = root
                .self_name(db)
                .unwrap_or_else(|| unreachable!("stdlib roots are named"));
            (name.to_string(), root)
        })
}

/// Derive a [`StdlibPrefix`] honestly, by compiling the embedded stdlib
/// sources. Costs a full stdlib compile, so call it once per process (a build
/// script) and reuse the result.
pub fn build_stdlib_prefix(opt: OptLevel) -> StdlibPrefix {
    // Only the stdlib roots matter here: no user file contributes to a
    // stdlib package, so the database carries the stdlib sources and nothing
    // else.
    let mut db = ProjectDatabase::new();
    db.ensure_stdlib_sources();
    let interfaces = stdlib_interfaces(&db);
    let packages = stdlib_roots(&db)
        .map(|(name, root)| {
            let emitted =
                emit_package(&db, root, opt).expect("the embedded stdlib always compiles cleanly");
            (name, emitted)
        })
        .collect();
    StdlibPrefix {
        interfaces,
        packages,
        opt,
    }
}

/// A prefix serves its stdlib packages' outputs, by the fixed name the
/// consuming database installs each stdlib root under, at the level it was
/// built for — and nothing else: a build-time constant is never written to.
impl PackageCache for StdlibPrefix {
    fn load(
        &self,
        db: &dyn baml_compiler2_hir::Db,
        root: SourceRoot,
        opt: OptLevel,
    ) -> Option<EmittedPackage> {
        if opt != self.opt || root.kind(db) != SourceRootKind::Stdlib {
            return None;
        }
        let name = root.self_name(db)?;
        self.packages.get(name.as_str()).cloned()
    }

    fn store(
        &self,
        _db: &dyn baml_compiler2_hir::Db,
        _root: SourceRoot,
        _opt: OptLevel,
        _emitted: &EmittedPackage,
    ) {
    }
}

/// The embedded artifact's wire shape: a header key, the interface map shared
/// by every level, and one set of package outputs per level.
///
/// Interfaces are type-level and identical across optimization levels, so they
/// are stored once rather than per level.
type Artifact = (
    String,
    BTreeMap<String, Vec<u8>>,
    Vec<(u8, BTreeMap<String, EmittedPackage>)>,
);

fn raw_level(opt: OptLevel) -> u8 {
    match opt {
        OptLevel::Zero => 0,
        OptLevel::One => 1,
        OptLevel::Two => 2,
    }
}

fn opt_level(raw: u8) -> OptLevel {
    match raw {
        0 => OptLevel::Zero,
        1 => OptLevel::One,
        2 => OptLevel::Two,
        other => panic!("{other} is not an OptLevel"),
    }
}

/// Serialize `prefixes` under `key` for a build script to embed.
///
/// `key` is the consumer's own compatibility header — it decides what counts as
/// a mismatched artifact — and is checked verbatim by [`decode_artifact`].
///
/// # Panics
///
/// If two prefixes share an optimization level, or if their interface maps
/// differ (they are derived before lowering, so they cannot legitimately).
pub fn encode_artifact(key: &str, prefixes: Vec<StdlibPrefix>) -> Vec<u8> {
    let mut interfaces: Option<BTreeMap<String, Vec<u8>>> = None;
    let mut levels: Vec<(u8, BTreeMap<String, EmittedPackage>)> =
        Vec::with_capacity(prefixes.len());
    for prefix in prefixes {
        let raw = raw_level(prefix.opt);
        assert!(
            !levels.iter().any(|(seen, _)| *seen == raw),
            "two stdlib prefixes were built at {:?}",
            prefix.opt
        );
        match &interfaces {
            None => interfaces = Some(prefix.interfaces),
            Some(first) => assert_eq!(
                first, &prefix.interfaces,
                "stdlib package interfaces differ between optimization levels, but they are \
                 derived before lowering and must not"
            ),
        }
        levels.push((raw, prefix.packages));
    }
    let artifact: Artifact = (
        key.to_string(),
        interfaces.expect("encode_artifact needs at least one prefix"),
        levels,
    );
    borsh::to_vec(&artifact).expect("serialize the stdlib prefix artifact")
}

/// Inverse of [`encode_artifact`], keyed by optimization level.
///
/// # Panics
///
/// If `bytes` does not decode, or carries a `key` other than the one supplied —
/// a producer/consumer mismatch that would otherwise surface as a confusing
/// downstream compile failure.
pub fn decode_artifact(key: &str, bytes: &[u8]) -> BTreeMap<OptLevel, StdlibPrefix> {
    let (found, interfaces, levels): Artifact =
        borsh::from_slice(bytes).expect("decode the embedded stdlib prefix artifact");
    assert_eq!(
        found, key,
        "the embedded stdlib prefix was produced by a different build than the one consuming it"
    );
    levels
        .into_iter()
        .map(|(raw, packages)| {
            let opt = opt_level(raw);
            (
                opt,
                StdlibPrefix {
                    interfaces: interfaces.clone(),
                    packages,
                    opt,
                },
            )
        })
        .collect()
}
