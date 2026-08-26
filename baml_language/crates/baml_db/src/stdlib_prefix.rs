//! The compiler-built stdlib slice, and how to derive one.
//!
//! Every BAML compile begins with the same embedded stdlib. Deriving it is the
//! dominant fixed cost of a compile, and it depends only on the compiler build
//! and the optimization level — no user file contributes to a stdlib package —
//! so it can be computed once per toolchain by a build script and spliced into
//! every compile afterwards.
//!
//! This module is the single producer. Its consumers each embed their own
//! artifact, because their requirements genuinely differ: `bex_project` ships
//! one optimization level in a production binary where size matters, while
//! `baml_tests` carries every level for tests, where it does not.
//! Sharing the *derivation* is what matters — an artifact that drifted from
//! what a real compile produces would be silently wrong in both.

use std::collections::BTreeMap;

pub use baml_compiler2_emit::OptLevel;
use baml_compiler2_emit::generate_stdlib_program;
use bex_vm_types::Program;

use crate::ProjectDatabase;

/// The compiler-built stdlib slice: every stdlib package's typed interface
/// alongside the bytecode prefix compiled from those same sources.
///
/// Both halves depend only on the compiler build and the optimization level —
/// no user file contributes to a stdlib package — so one instance is valid for
/// every compile at that `opt`. Produce it with [`build_stdlib_prefix`].
///
/// The two halves must come from the same [`build_stdlib_prefix`] call: the
/// interfaces short-circuit type derivation while the program short-circuits
/// lowering, and splicing a program built at a different `opt` than the
/// interfaces were derived with would emit against the wrong index layout.
pub struct StdlibPrefix {
    /// Stdlib package name -> `borsh(PackageInterface)`.
    pub interfaces: BTreeMap<String, Vec<u8>>,
    /// The stdlib-only bytecode slice, from [`generate_stdlib_program`].
    pub program: Program,
    /// The level `program` was lowered at. The `testing` module's
    /// prefix-taking compile helpers assert the caller asked for the same one:
    /// user code emitted on top of a spliced prefix must be lowered the same
    /// way the prefix was.
    pub opt: OptLevel,
}

/// Derive a [`StdlibPrefix`] honestly, by compiling the embedded stdlib
/// sources. Costs a full stdlib compile, so call it once per process (a build
/// script) and reuse the result.
pub fn build_stdlib_prefix(opt: OptLevel) -> StdlibPrefix {
    use baml_compiler2_hir::package::PackageId;
    use baml_compiler2_hir_ty::package_interface::package_interface;

    // Only the stdlib roots matter here: no user file contributes to a
    // stdlib package, so the database carries the stdlib sources and nothing
    // else.
    let mut db = ProjectDatabase::new();
    db.ensure_stdlib_sources();
    let interfaces = baml_builtins2::stdlib_package_names()
        .iter()
        .map(|name| {
            let package = PackageId::new(&db, baml_base::Name::new(*name));
            let bytes = borsh::to_vec(package_interface(&db, package))
                .expect("a stdlib PackageInterface always serializes");
            ((*name).to_string(), bytes)
        })
        .collect();
    let program =
        generate_stdlib_program(&db, opt).expect("the embedded stdlib always compiles cleanly");
    StdlibPrefix {
        interfaces,
        program,
        opt,
    }
}

/// The embedded artifact's wire shape: a header key, the interface map shared
/// by every level, and one bytecode slice per level.
///
/// Interfaces are type-level and identical across optimization levels, so they
/// are stored once rather than per slice.
type Artifact = (String, BTreeMap<String, Vec<u8>>, Vec<(u8, Program)>);

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
    let mut programs: Vec<(u8, Program)> = Vec::with_capacity(prefixes.len());
    for prefix in prefixes {
        let raw = raw_level(prefix.opt);
        assert!(
            !programs.iter().any(|(seen, _)| *seen == raw),
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
        programs.push((raw, prefix.program));
    }
    let artifact: Artifact = (
        key.to_string(),
        interfaces.expect("encode_artifact needs at least one prefix"),
        programs,
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
    let (found, interfaces, programs): Artifact =
        borsh::from_slice(bytes).expect("decode the embedded stdlib prefix artifact");
    assert_eq!(
        found, key,
        "the embedded stdlib prefix was produced by a different build than the one consuming it"
    );
    programs
        .into_iter()
        .map(|(raw, program)| {
            let opt = opt_level(raw);
            (
                opt,
                StdlibPrefix {
                    interfaces: interfaces.clone(),
                    program,
                    opt,
                },
            )
        })
        .collect()
}
