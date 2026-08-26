//! The prefix-accelerated compile helpers must be a *substitute*, not an
//! approximation.
//!
//! `baml_tests::stdlib_prefix` skips re-deriving the stdlib by splicing in a slice
//! built at build time. That is only legitimate while the spliced result is
//! indistinguishable from an honest compile, so this oracle compiles the same
//! sources both ways and compares the serialized `Program`s byte for byte, at
//! every optimization level the artifact carries.
//!
//! It is the reason `baml_db::testing`'s honest helpers still exist: they
//! are the control arm. Deleting them would leave the fast path unfalsifiable.
//!
//! A failure here means the suite has started testing a different artifact than
//! a real compile produces. Do not re-bless it — find what the prefix is
//! serving that the sources no longer imply.

use std::{collections::HashSet, path::Path};

use baml_compiler_diagnostics::Severity;
use baml_compiler2_emit::{CompileOptions, OptLevel, generate_project_bytecode_with_opt};
use baml_db::{ProjectDatabase, collect_diagnostics, testing};
use baml_tests::{
    engine::TestDbExt,
    stdlib_prefix::{check_user_files, prefix},
};

/// One project exercising the constructs whose lowering could plausibly depend
/// on stdlib derivation: sysop calls (the case a source-less stdlib mount gets
/// wrong), stdlib generics, user interfaces and impls, closures, `throws`
/// inference across files, and auto-derived class methods.
const FILES: &[(&str, &str)] = &[
    (
        "prims.baml",
        r#"
function add(a: int, b: int) -> int { a + b }
function greet(name: string) -> string { "hello " + name }
function pick(flag: bool) -> float { if (flag) { 1.5 } else { 2.5 } }
"#,
    ),
    (
        "sysops.baml",
        r#"
// A direct sysop call: lowers to `sys_op`, but only while the compiler can see
// the stdlib body behind `baml.io.println`.
function shout(line: string) -> null { baml.io.println(line) }
function encoded(xs: int[]) -> string { json.to_string(xs) }
"#,
    ),
    (
        "classes.baml",
        r#"
class Point {
  x int
  y int
  function norm1(self) -> int { self.x + self.y }
}
enum Color { Red, Green, Blue }
type MaybePoint = Point?
function origin() -> Point { Point { x: 0, y: 0 } }
function shade(c: Color) -> string {
  match (c) { Color.Red => "r", Color.Green => "g", Color.Blue => "b" }
}
"#,
    ),
    (
        "ifaces.baml",
        r#"
interface Area { function area(self) -> int throws never }
class Square { side int }
implements Area for Square { function area(self) -> int { self.side * self.side } }
function total_area(sq: Square) -> int { sq.area() }
"#,
    ),
    (
        "closures.baml",
        r#"
function doubled(xs: int[]) -> int[] { xs.map((v: int) -> int { v * 2 }) }
function summed(xs: int[]) -> int {
  let total = 0;
  for (let x in xs) { total += x; }
  total
}
"#,
    ),
    (
        "throws.baml",
        r#"
class Bad { msg string }
function risky(n: int) -> int throws Bad {
  if (n < 0) { throw Bad { msg: "negative" } }
  n
}
function safe(n: int) -> int { risky(n) catch (e) { Bad => 0 } }
"#,
    ),
    (
        "ns_sub/nested.baml",
        r#"
class Inner { tag string }
function tagged(t: string) -> Inner { Inner { tag: t } }
"#,
    ),
];

fn opts() -> CompileOptions {
    CompileOptions {
        emit_test_cases: false,
    }
}

fn honest_bytes(opt: OptLevel) -> Vec<u8> {
    let mut db = ProjectDatabase::new();
    db.workspace(Path::new("."));
    for (path, content) in FILES {
        db.file(*path, content);
    }
    testing::assert_no_diagnostic_errors(&db);
    let program = generate_project_bytecode_with_opt(&db, &opts(), opt).expect("honest compile");
    borsh::to_vec(&program).expect("serialize honest program")
}

fn fast_bytes(opt: OptLevel) -> Vec<u8> {
    let program = testing::compile_multi_file_with_prefix(prefix(opt), FILES, opt);
    borsh::to_vec(&program).expect("serialize prefixed program")
}

#[test]
fn prefixed_compile_is_byte_identical_at_every_opt_level() {
    for opt in [OptLevel::Zero, OptLevel::One, OptLevel::Two] {
        let honest = honest_bytes(opt);
        let fast = fast_bytes(opt);
        assert_eq!(
            honest.len(),
            fast.len(),
            "{opt:?}: prefixed program is {} bytes, honest is {} — the spliced stdlib slice no \
             longer matches what these sources compile to",
            fast.len(),
            honest.len(),
        );
        if honest != fast {
            let at = honest
                .iter()
                .zip(&fast)
                .position(|(a, b)| a != b)
                .expect("lengths are equal and contents differ");
            panic!(
                "{opt:?}: prefixed program diverges from the honest one at byte {at} of {}",
                honest.len()
            );
        }
    }
}

/// `check_user_files` narrows the checked set to user files, which is sound
/// only because a test database is written once and read once. Pin that it
/// reports the same user-file diagnostics as the whole-project pass — including
/// the package-level ones, which belong to no single file and are the easiest
/// to lose when narrowing.
#[test]
fn user_file_check_matches_the_whole_project_pass() {
    const BROKEN: &[(&str, &str)] = &[
        (
            "a.baml",
            "class Dup { v int }\nfunction bad() -> int { \"not an int\" }\n",
        ),
        (
            "b.baml",
            "class Dup { v string }\nfunction worse() -> int { unknown_name() }\n",
        ),
    ];

    let mut db = ProjectDatabase::new();
    db.workspace(Path::new("."));
    db.set_seeded_stdlib_interface(prefix(OptLevel::One).interfaces.clone());
    for (path, content) in BROKEN {
        db.file(*path, content);
    }

    let user_files: HashSet<_> = db
        .workspace_files()
        .iter()
        .map(|f| f.file_id(&db))
        .collect();
    // Compared as ordered lists, not sets: `check_user_files` sorts with the
    // same comparator as the whole-project pass precisely so that snapshot
    // callers see an identical sequence, and a set compare would not catch a
    // regression in that ordering.
    let render = |diagnostics: Vec<baml_compiler_diagnostics::Diagnostic>| -> Vec<String> {
        diagnostics
            .iter()
            .filter(|d| matches!(d.severity, Severity::Error))
            .filter(|d| {
                d.primary_span()
                    .map(|s| user_files.contains(&s.file_id))
                    .unwrap_or(false)
            })
            .map(|d| format!("[{}] {}", d.code(), d.message_with_primary_label()))
            .collect()
    };

    let whole_project = render(collect_diagnostics(&db));
    let user_only = render(check_user_files(&db));

    assert!(
        !whole_project.is_empty(),
        "the fixture must actually produce errors, or this proves nothing"
    );
    assert_eq!(
        whole_project, user_only,
        "narrowing the check to user files changed the reported diagnostics (or their order)"
    );
}
