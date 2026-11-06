//! Shared fixtures for the package emit oracle.

use std::path::Path;

use baml_db::ProjectDatabase;
use baml_tests::engine::{TestDbExt, db_with_root};

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
