//! What a compile records about a package (BEP-085): the hash of its
//! interface, the hash of its compiled output, and the identity its manifest
//! claims, which both hashes cover.
//!
//! None of this is visible from BAML: the hashes are metadata for tooling
//! and telemetry, so the scenarios drive the emitter directly.

use baml_base::{ClaimedIdentity, Dependency, Name, SourceRoot};
use baml_compiler2_emit::emit_package;
use baml_compiler2_hir_ty::package_interface::{COMPILER_VERSION, interface_digest};
use baml_db::{CheckedWorld, OptLevel, ProjectDatabase, SourceRootKind, SourceRootSpec};
use bex_vm_types::identity::{PackageInterfaceHash, PackageMetadata};

const LIB: &str = "function answer() -> int {\n    42\n}\n";
const APP: &str = "function main() -> int {\n    lib.answer()\n}\n";

fn claim(name: &str, version: &str) -> ClaimedIdentity {
    ClaimedIdentity {
        name: Some(name.to_owned()),
        version: Some(version.to_owned()),
        ..ClaimedIdentity::default()
    }
}

/// A database with the stdlib, a `lib` dependency root, and an `app`
/// workspace root that reaches it as `lib`.
fn world(lib: &str, lib_claim: ClaimedIdentity) -> (ProjectDatabase, SourceRoot, SourceRoot) {
    let mut db = ProjectDatabase::new();
    db.ensure_stdlib_sources();
    let lib_root = db
        .add_source_root(
            SourceRootSpec::new("/package-metadata/lib", SourceRootKind::Dependency)
                .claiming(lib_claim),
        )
        .expect("a dependency root");
    db.add_or_update_file_in(
        lib_root,
        std::path::Path::new("/package-metadata/lib/lib.baml"),
        lib,
    );
    let app_root = db
        .add_source_root(
            SourceRootSpec::new("/package-metadata/app", SourceRootKind::Workspace)
                .claiming(claim("app", "0.1.0"))
                .depending_on(vec![Dependency {
                    name: Name::new("lib"),
                    root: lib_root,
                }]),
        )
        .expect("a workspace root");
    db.add_or_update_file_in(
        app_root,
        std::path::Path::new("/package-metadata/app/main.baml"),
        APP,
    );
    (db, lib_root, app_root)
}

fn metadata(db: &ProjectDatabase, root: SourceRoot) -> PackageMetadata {
    emit_package(db, root, OptLevel::One)
        .expect("the package emits")
        .record
        .metadata
        .expect("the emitter records every package's metadata")
}

#[test]
fn metadata_states_the_hashes_the_claim_and_what_was_compiled_against() {
    let (db, lib, app) = world(LIB, claim("lib", "1.0.0"));
    let lib_meta = metadata(&db, lib);
    let app_meta = metadata(&db, app);

    assert_eq!(lib_meta.claimed, claim("lib", "1.0.0"));
    assert_eq!(app_meta.claimed, claim("app", "0.1.0"));
    assert_eq!(lib_meta.compiler_version, COMPILER_VERSION);
    assert_eq!(
        lib_meta.package_interface,
        PackageInterfaceHash(interface_digest(&db, lib)),
        "the recorded interface hash is the digest dependents bind"
    );
    assert!(
        app_meta.dependencies.contains(&lib_meta.package_interface),
        "a package names the interface of every package it reaches"
    );
    assert!(
        app_meta.dependencies.len() > 1,
        "the language packages are reached too"
    );
    assert_ne!(
        lib_meta.compilation_unit_hash,
        app_meta.compilation_unit_hash
    );
}

#[test]
fn equal_builds_record_equal_metadata() {
    let (first, first_lib, first_app) = world(LIB, claim("lib", "1.0.0"));
    let (second, second_lib, second_app) = world(LIB, claim("lib", "1.0.0"));
    assert_eq!(metadata(&first, first_lib), metadata(&second, second_lib));
    assert_eq!(metadata(&first, first_app), metadata(&second, second_app));
}

#[test]
fn a_body_change_moves_the_unit_hash_and_spares_dependents() {
    let (base, base_lib, base_app) = world(LIB, claim("lib", "1.0.0"));
    let (edited, edited_lib, edited_app) = world(
        "function answer() -> int {\n    43\n}\n",
        claim("lib", "1.0.0"),
    );
    let (before, after) = (metadata(&base, base_lib), metadata(&edited, edited_lib));
    assert_eq!(
        before.package_interface, after.package_interface,
        "a body is not part of the interface"
    );
    assert_ne!(before.compilation_unit_hash, after.compilation_unit_hash);
    assert_eq!(
        metadata(&base, base_app),
        metadata(&edited, edited_app),
        "a dependent compiles against the interface alone"
    );
}

#[test]
fn whitespace_and_comments_are_not_content() {
    let (base, base_lib, _) = world(LIB, claim("lib", "1.0.0"));
    for restyled in [
        "function answer() -> int { 42 }\n",
        "\n\n// The answer.\nfunction answer()\n    -> int\n{\n\n    42 // forty-two\n}\n",
    ] {
        let (edited, edited_lib, _) = world(restyled, claim("lib", "1.0.0"));
        assert_eq!(
            metadata(&base, base_lib),
            metadata(&edited, edited_lib),
            "{restyled:?}"
        );
    }
}

#[test]
fn a_signature_change_moves_both_hashes_and_the_dependents() {
    let (base, base_lib, base_app) = world(LIB, claim("lib", "1.0.0"));
    let (edited, edited_lib, edited_app) = world(
        "function answer() -> int {\n    42\n}\n\nfunction other() -> int {\n    1\n}\n",
        claim("lib", "1.0.0"),
    );
    let (before, after) = (metadata(&base, base_lib), metadata(&edited, edited_lib));
    assert_ne!(before.package_interface, after.package_interface);
    assert_ne!(before.compilation_unit_hash, after.compilation_unit_hash);
    let (app_before, app_after) = (metadata(&base, base_app), metadata(&edited, edited_app));
    assert_ne!(
        app_before.package_interface, app_after.package_interface,
        "an interface covers the interfaces it was exported against"
    );
    assert_ne!(
        app_before.compilation_unit_hash, app_after.compilation_unit_hash,
        "the unit binds the dependency's interface digest"
    );
}

#[test]
fn the_claim_is_part_of_both_hashes() {
    let (base, base_lib, base_app) = world(LIB, claim("lib", "1.0.0"));
    for other in [
        claim("lib", "1.0.1"),
        claim("renamed", "1.0.0"),
        ClaimedIdentity {
            organization: Some("acme".to_owned()),
            ..claim("lib", "1.0.0")
        },
        ClaimedIdentity {
            registry: Some("https://packages.example.com".to_owned()),
            ..claim("lib", "1.0.0")
        },
        ClaimedIdentity::default(),
    ] {
        let (edited, edited_lib, edited_app) = world(LIB, other.clone());
        let (before, after) = (metadata(&base, base_lib), metadata(&edited, edited_lib));
        assert_ne!(
            before.package_interface, after.package_interface,
            "{other:?}"
        );
        assert_ne!(
            before.compilation_unit_hash, after.compilation_unit_hash,
            "{other:?}"
        );
        assert_ne!(
            metadata(&base, base_app).compilation_unit_hash,
            metadata(&edited, edited_app).compilation_unit_hash,
            "a dependent binds the interface it compiled against: {other:?}"
        );
    }
}

#[test]
fn restating_the_claim_invalidates_the_interface() {
    let (mut db, lib, _) = world(LIB, claim("lib", "1.0.0"));
    let before = interface_digest(&db, lib);
    db.set_claimed_identity(lib, claim("lib", "1.0.0"));
    assert_eq!(before, interface_digest(&db, lib));
    db.set_claimed_identity(lib, claim("lib", "2.0.0"));
    assert_ne!(before, interface_digest(&db, lib));
}

#[test]
fn the_linked_program_carries_every_package_and_a_content_identity() {
    let (db, lib, app) = world(LIB, claim("lib", "1.0.0"));
    let program = CheckedWorld::collect(&db, app)
        .expect("the world checks")
        .compile(OptLevel::One)
        .expect("the world links");
    let units: Vec<_> = program
        .packages
        .iter()
        .map(|package| {
            package
                .metadata
                .as_ref()
                .expect("every linked package carries its metadata")
                .compilation_unit_hash
        })
        .collect();
    assert!(units.contains(&metadata(&db, lib).compilation_unit_hash));
    assert!(units.contains(&metadata(&db, app).compilation_unit_hash));
    assert_eq!(
        program.linked_id(),
        Some(bex_vm_types::identity::linked_program_id(
            program.root,
            &units
        ))
    );
}
