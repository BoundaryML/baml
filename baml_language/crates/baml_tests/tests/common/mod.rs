//! Shared fixtures and assertions for the link / relink byte-identity oracles.

#![allow(dead_code)] // Shared helpers; individual oracle files use a subset.

use std::path::Path;

use baml_db::ProjectDatabase;
use baml_tests::engine::{TestDbExt, db_with_root};
use bex_vm_types::Program;

/// Three-file fixture with cross-file class + function references: `Point` is
/// defined in A and used in B/C; `make_point` / `scale` / `label` are called
/// across files, exercising import resolution and pass-major placement.
pub const A_BAML: &str = r#"class Point {
  x int
  y int
}

function make_point(x: int, y: int) -> Point {
  Point { x: x, y: y }
}

function origin() -> Point {
  make_point(0, 0)
}
"#;

pub const B_BAML: &str = r#"function scale(p: Point, factor: int) -> Point {
  let mul = (v: int) -> int { v * factor }
  Point { x: mul(p.x), y: mul(p.y) }
}

function magnitude_ish(p: Point) -> int {
  p.x * p.x + p.y * p.y
}

function label(p: Point) -> string {
  "point-label"
}
"#;

pub const C_BAML: &str = r#"function main() -> int {
  let banner = "start";
  baml.io.println(banner);
  let p = make_point(3, 4);
  let doubled = scale(p, 2);
  let tag = label(doubled);
  baml.io.println(tag);
  magnitude_ish(doubled)
}
"#;

/// Build an in-memory `ProjectDatabase` rooted at `root` holding `files`
/// (`(name, content)` pairs written under the root).
pub fn build_db(root: &str, files: &[(&str, &str)]) -> ProjectDatabase {
    let mut db = db_with_root(Path::new(root));
    for (name, content) in files {
        db.file(Path::new(root).join(name), content);
    }
    db
}

/// Assert two `Program`s serialize to identical borsh bytes, panicking with the
/// byte lengths, the first differing offset, and a hex window around it.
/// (`Program` has no `PartialEq`, so equality is proved on the bytes.)
pub fn assert_programs_byte_identical(label: &str, expected: &Program, actual: &Program) {
    let expected_bytes = borsh::to_vec(expected).expect("serialize expected program");
    let actual_bytes = borsh::to_vec(actual).expect("serialize actual program");
    if expected_bytes == actual_bytes {
        return;
    }
    let first_diff = expected_bytes
        .iter()
        .zip(actual_bytes.iter())
        .position(|(a, b)| a != b)
        .unwrap_or_else(|| expected_bytes.len().min(actual_bytes.len()));
    let ctx = |v: &[u8]| {
        let s = first_diff.saturating_sub(8);
        let end = (first_diff + 8).min(v.len());
        format!("{:02x?}", &v[s..end])
    };
    panic!(
        "{label}: programs differ\n\
         lengths: expected={} actual={}\n\
         first diff at byte {first_diff}\n\
         expected ..={}\n\
         actual   ..={}",
        expected_bytes.len(),
        actual_bytes.len(),
        ctx(&expected_bytes),
        ctx(&actual_bytes),
    );
}
