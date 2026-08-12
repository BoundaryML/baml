//! Differential coherence tests (I7): hir_ty's
//! `package_coherence_violations` / `package_orphan_violations` against
//! TIR's `package_coherence_diagnostics` / orphan diagnostics over the
//! same sources. Violations are compared as sets of
//! `(primary range, secondary range, indeterminate)` - the two engines
//! must agree pair-for-pair, and each case pins the expected set so the
//! agreement is never vacuous.

use baml_compiler2_hir::package::PackageId;
use baml_project::ProjectDatabase;
use text_size::TextRange;

fn user_package<'db>(db: &'db ProjectDatabase, file: baml_base::SourceFile) -> PackageId<'db> {
    let package = baml_compiler2_hir::file_package::file_package(db, file).package;
    PackageId::new(db, package)
}

fn hir_ty_pairs(source: &str) -> Vec<(TextRange, TextRange, bool)> {
    let mut db = crate::compiler2_tir::support::make_db();
    let file = db.add_file("test.baml", source);
    let pkg = user_package(&db, file);
    let mut pairs: Vec<(TextRange, TextRange, bool)> =
        baml_compiler2_hir_ty::coherence::package_coherence_violations(&db, pkg)
            .0
            .iter()
            .map(|violation| {
                let span_of = |loc: baml_compiler2_hir::loc::ImplLoc<'_>| {
                    baml_compiler2_ppir::item_data::impl_block_source_map(&db, loc).span
                };
                (
                    span_of(violation.primary),
                    span_of(violation.secondary),
                    violation.indeterminate,
                )
            })
            .collect();
    pairs.sort_by_key(|(p, s, _)| (p.start(), p.end(), s.start(), s.end()));
    pairs
}

fn tir_pairs(source: &str) -> Vec<(TextRange, TextRange, bool)> {
    let mut db = crate::compiler2_tir::support::make_db();
    let file = db.add_file("test.baml", source);
    let pkg = user_package(&db, file);
    let mut pairs: Vec<(TextRange, TextRange, bool)> =
        baml_compiler2_hir_ty::interfaces::package_coherence_diagnostics(&db, pkg)
            .iter()
            .map(|violation| {
                (
                    violation.primary.range,
                    violation.secondary.range,
                    violation.indeterminate,
                )
            })
            .collect();
    pairs.sort_by_key(|(p, s, _)| (p.start(), p.end(), s.start(), s.end()));
    pairs
}

/// Both engines over `source`; assert they agree AND match the expected
/// (definite, indeterminate) violation counts.
///
/// Span comparison is by CONTAINMENT, not equality: hir_ty is
/// location-keyed (this test maps a violation to its whole impl-block
/// span) while TIR anchors some diagnostics on a sub-span of the block
/// (an in-body violation points at the interface-target name). The
/// semantic identity is the block; S17 picks the rendering anchor.
#[track_caller]
fn check_coherence(source: &str, definite: usize, indeterminate: usize) {
    let hir = hir_ty_pairs(source);
    let tir = tir_pairs(source);
    let agree = hir.len() == tir.len()
        && hir.iter().zip(tir.iter()).all(|(h, t)| {
            let contains = |outer: TextRange, inner: TextRange| {
                outer.start() <= inner.start() && inner.end() <= outer.end()
            };
            contains(h.0, t.0) && contains(h.1, t.1) && h.2 == t.2
        });
    assert!(
        agree,
        "engines disagree on coherence violations\n  hir_ty: {hir:?}\n  tir: {tir:?}"
    );
    let got_definite = hir.iter().filter(|(_, _, ind)| !ind).count();
    let got_indeterminate = hir.iter().filter(|(_, _, ind)| *ind).count();
    assert_eq!(
        (got_definite, got_indeterminate),
        (definite, indeterminate),
        "violation counts (definite, indeterminate) mismatch: {hir:?}"
    );
}

/// hir_ty orphan outcomes: `(uncovered_param?)` per violation, source order.
#[track_caller]
fn check_orphans(source: &str, expected: &[Option<&str>]) {
    let mut db = crate::compiler2_tir::support::make_db();
    let file = db.add_file("test.baml", source);
    let pkg = user_package(&db, file);
    let got: Vec<Option<String>> =
        baml_compiler2_hir_ty::coherence::package_orphan_violations(&db, pkg)
            .0
            .iter()
            .map(|violation| {
                violation
                    .uncovered_param
                    .as_ref()
                    .map(|name| name.as_str().to_string())
            })
            .collect();
    let expected: Vec<Option<String>> = expected
        .iter()
        .map(|param| param.map(str::to_string))
        .collect();
    assert_eq!(got, expected, "orphan outcomes mismatch");
}

#[test]
fn duplicate_in_body_impls_overlap() {
    // A duplicate block is a degenerate overlap (rustc's
    // conflicting-implementations error for exact duplicates).
    check_coherence(
        r#"
interface Marker {}

class C {
    x int
    implements Marker {}
    implements Marker {}
}
"#,
        1,
        0,
    );
}

#[test]
fn complementary_blankets_share_common_instance() {
    // `Pair<T, int>` vs `Pair<string, U>`: common instance
    // `Pair<string, int>` - found only by SYMMETRIC unification (either
    // side may bind), the case a one-directional matcher misses.
    check_coherence(
        r#"
interface Marker {}

class Pair<A, B> {
    a A
    b B
}

implement<T> Marker for Pair<T, int> {}
implement<U> Marker for Pair<string, U> {}
"#,
        1,
        0,
    );
}

#[test]
fn distinct_subjects_are_disjoint() {
    check_coherence(
        r#"
interface Marker {}

implement Marker for int {}
implement Marker for string {}
"#,
        0,
        0,
    );
}

#[test]
fn alias_obscured_duplicate_overlaps() {
    // `type AI = int`: the alias-expanded heads collide; missing the
    // expansion would admit two impls for one concrete type (fails open).
    check_coherence(
        r#"
interface Marker {}

type AI = int

implement Marker for int {}
implement Marker for AI {}
"#,
        1,
        0,
    );
}

#[test]
fn bound_refutes_common_instance() {
    // `implement<T extends Other> Marker for Box<T>` vs
    // `implement Marker for Box<C>`: the unifier pins `T := C`, and `C`
    // provably does not implement `Other` (registry-checked at the
    // realized witness), so the pair is disjoint.
    check_coherence(
        r#"
interface Marker {}
interface Other {}

class Box<T> {
    value T
}

class C {
    x int
}

implement<T extends Other> Marker for Box<T> {}
implement Marker for Box<C> {}
"#,
        0,
        0,
    );
}

#[test]
fn satisfied_bound_keeps_overlap() {
    // Same shape, but `C` DOES implement `Other`: the bound holds at the
    // common instance, so the overlap is real.
    check_coherence(
        r#"
interface Marker {}
interface Other {}

class Box<T> {
    value T
}

class C {
    x int
    implements Other {}
}

implement<T extends Other> Marker for Box<T> {}
implement Marker for Box<C> {}
"#,
        1,
        0,
    );
}

#[test]
fn orphan_local_type_covers_foreign_interface() {
    // A local class implementing a stdlib interface: covered, no
    // violation (and locally-declared interfaces are always fine).
    check_orphans(
        r#"
class C {
    x int
    implements baml.ops.Equals {
        function eq(self, other: C) -> bool throws never {
            true
        }
    }
}
"#,
        &[],
    );
}

#[test]
fn orphan_foreign_for_foreign_is_violation() {
    // A foreign interface for a foreign type: no local type anywhere in
    // the impl's inputs (E0139).
    check_orphans(
        r#"
implement baml.ops.Equals for baml.iter.Done {}
"#,
        &[None],
    );
}

#[test]
fn orphan_uncovered_param_is_violation() {
    // `implement<T> baml.ops.Equals for T`: the bare param precedes any
    // local type - RFC-2451's covered rule.
    check_orphans(
        r#"
implement<T> baml.ops.Equals for T {}
"#,
        &[Some("T")],
    );
}
