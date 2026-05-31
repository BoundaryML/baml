//! Deterministic hash of the codegen *source of truth* — the `.baml` files
//! under `crates/baml_builtins2/baml_std/`.
//!
//! Shared by two callers so the value is computed identically on both sides:
//!   * `tools_rustgen` embeds a per-file hash in the header of each generated file.
//!   * the `build.rs` of every generated-code crate (`sys_types`, `sys_ops`,
//!     `bex_vm`, `bex_vm_types`) recomputes the hash and **fails the build** if
//!     the embedded one is stale — so a `baml_std` change that wasn't
//!     regenerated is caught at compile time, in the package that owns the file.
//!
//! This crate has **zero dependencies** on purpose: it is a build-dependency of
//! those crates, and must not pull in `baml_compiler2_ast` (which would
//! reintroduce the host/target double-compile that motivated checking the
//! generated code into the tree in the first place).
//!
//! NOTE: to avoid forcing a regen on every unrelated stdlib edit, this hashes
//! ONLY the `$rust_function` / `$rust_io_function` / `$compiler_intrinsic`
//! function signatures (see [`extract_relevant`]) — the brittle subset the
//! codegen actually consumes. Changes to the codegen *logic*
//! (`baml_builtins2_codegen` / ast lowering) or to class/struct definitions are
//! caught by `tools_rustgen`'s `up_to_date` test, which regenerates and diffs
//! in CI.

use std::path::{Path, PathBuf};

/// The comment label written into each generated file's header.
pub const GENERATED_FROM: &str = "crates/baml_builtins2/baml_std/** via `baml_builtins2_codegen`";

/// Marker prefix for the embedded hash line, e.g. `// codegen-hash: a1b2c3...`.
pub const HASH_PREFIX: &str = "codegen-hash: ";

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

fn fnv1a(bytes: &[u8], mut h: u64) -> u64 {
    for &b in bytes {
        h ^= u64::from(b);
        h = h.wrapping_mul(FNV_PRIME);
    }
    h
}

/// `crates/` directory of the workspace, resolved at compile time (this crate
/// lives at `<workspace>/crates/baml_rustgen_check`).
pub fn crates_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("baml_rustgen_check lives in <workspace>/crates/baml_rustgen_check")
        .to_path_buf()
}

/// The source-of-truth `.baml` stdlib directory.
pub fn baml_std_dir() -> PathBuf {
    crates_dir().join("baml_builtins2").join("baml_std")
}

/// Every file under `baml_std`, as `(forward-slash relative path, bytes)`,
/// sorted by path for a stable, platform-independent ordering.
fn collect_inputs() -> Vec<(String, Vec<u8>)> {
    let root = baml_std_dir();
    let mut out = Vec::new();
    walk(&root, &root, &mut out);
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

fn walk(root: &Path, dir: &Path, out: &mut Vec<(String, Vec<u8>)>) {
    // Fail CLOSED: an unreadable source-of-truth must abort the build/regen, not
    // silently hash a partial tree (which could match stale generated output).
    // This only runs at build/codegen time, so a panic is the right loud failure.
    let entries = std::fs::read_dir(dir).unwrap_or_else(|e| {
        panic!(
            "baml_rustgen_check: cannot read baml_std dir {}: {e}",
            dir.display()
        )
    });
    for entry in entries {
        let entry = entry.unwrap_or_else(|e| {
            panic!(
                "baml_rustgen_check: cannot read dir entry in {}: {e}",
                dir.display()
            )
        });
        let path = entry.path();
        if path.is_dir() {
            walk(root, &path, out);
        } else {
            let bytes = std::fs::read(&path).unwrap_or_else(|e| {
                panic!(
                    "baml_rustgen_check: cannot read baml_std file {}: {e}",
                    path.display()
                )
            });
            let rel = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            out.push((rel, bytes));
        }
    }
}

/// The only declarations the codegen actually consumes. Hashing the whole
/// stdlib would force a regen on every doc-comment edit, so we scope to these.
const MARKERS: [&str; 3] = ["$rust_io_function", "$rust_function", "$compiler_intrinsic"];

/// How far above a marker we'll look for its enclosing `function` signature.
/// Generous: real signatures are a handful of lines; this only bounds pathology.
const MAX_SIG_LOOKBACK: usize = 60;

/// Collapse runs of whitespace to single spaces, so reindentation/reflow doesn't
/// change the hash but token changes do.
///
/// Known limitation: this also collapses whitespace *inside* string literals
/// (e.g. a `"a  b"` default → `"a b"`), so that specific drift wouldn't change
/// the hash. A quote-aware normalizer would close it, but it's not worth the
/// complexity: such changes are exceedingly rare in builtin signatures, and the
/// `tools_rustgen` `up_to_date` test (real regen + diff) catches ANY drift,
/// including this, as the absolute backstop.
fn normalize(line: &str) -> String {
    line.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Portion of a line before a `//` comment — used ONLY for marker detection, so a
/// marker named in prose (`/// calls $rust_function`) is ignored and a `//` inside
/// a string can't truncate a captured signature (capture uses `normalize`).
fn code_before_comment(line: &str) -> &str {
    match line.find("//") {
        Some(i) => &line[..i],
        None => line,
    }
}

fn line_carries_marker(line: &str) -> bool {
    let code = code_before_comment(line);
    MARKERS.iter().any(|m| code.contains(*m))
}

/// A `//baml:…` codegen directive (`mut_self`/`vm`/`mut_vm`/`may_yield`/`fallible`).
/// These are comments, but they change generated code, so — unlike doc comments —
/// they ARE part of the relevant surface.
fn is_directive(trimmed: &str) -> bool {
    trimmed.starts_with("//baml:")
}

/// Extract the codegen-relevant surface from ONE file's source.
///
/// Generated code is a function of exactly two things in each `.baml` file:
///   1. **builtin functions** — those whose body is a `$rust_function` /
///      `$rust_io_function` / `$compiler_intrinsic` marker: their full signature
///      (name/params/return/`throws`/generics/`self`), the marker, and the
///      `//baml:` directives directly above them.
///   2. **class definitions** — the `class` declaration plus its fields.
///
/// Everything else is dropped: doc comments, blanks, plain comments, and —
/// crucially — pure-BAML functions/methods (no marker), whose signatures *and*
/// bodies generate nothing. Pure-BAML method bodies live inside class bodies, so
/// fields are separated from them by tracking brace depth.
///
/// This is a lexical heuristic, not a parser; it is exhaustively unit-tested
/// below, and the `tools_rustgen` `up_to_date` test (real regen + diff in CI)
/// is the absolute backstop covering codegen-logic changes a lexer can't see.
pub fn extract_relevant(source: &str) -> Vec<String> {
    let lines: Vec<&str> = source.lines().collect();
    let mut units = Vec::new();

    // ---- Pass 1: builtin functions (directives + signature + marker) ----
    // Anchored on the marker, so pure-BAML functions (no marker) never appear.
    for marker_idx in 0..lines.len() {
        if !line_carries_marker(lines[marker_idx]) {
            continue;
        }
        // Walk up to the enclosing `function` line.
        let mut start = marker_idx;
        let mut anchored = false;
        while marker_idx - start <= MAX_SIG_LOOKBACK {
            if normalize(lines[start]).starts_with("function ") {
                anchored = true;
                break;
            }
            if start == 0 {
                break;
            }
            start -= 1;
        }
        // `//baml:` directives sit on the line(s) directly above the `function`.
        let mut directives: Vec<String> = Vec::new();
        if anchored {
            let mut d = start;
            while d > 0 && is_directive(lines[d - 1].trim()) {
                directives.push(normalize(lines[d - 1]));
                d -= 1;
            }
            directives.reverse();
        }
        // Signature block: the `function …` line(s) through the marker, minus
        // any doc/plain comments (but the marker line itself is kept).
        let sig: String = lines[start..=marker_idx]
            .iter()
            .map(|l| normalize(l))
            .filter(|l| !l.is_empty() && (is_directive(l) || !l.starts_with("//")))
            .collect::<Vec<_>>()
            .join(" ");

        units.push(if anchored {
            format!("fn|{}|{sig}", directives.join(" "))
        } else {
            // Stray marker with no `function` above — never drop it silently.
            format!("fn|<unanchored>|{}", normalize(lines[marker_idx]))
        });
    }

    // ---- Pass 2: class declarations + fields ----
    // Brace-depth tracking captures fields (direct children of a class body) but
    // NOT method signatures (Pass 1 owns those) or method bodies — so pure-BAML
    // methods inside classes drop out entirely, just like free pure-BAML fns.
    let mut depth: i32 = 0;
    let mut class_body_depths: Vec<i32> = Vec::new();
    for raw in &lines {
        let t = raw.trim();

        // Leave any class scopes we've closed out of.
        while class_body_depths.last().is_some_and(|&bd| depth < bd) {
            class_body_depths.pop();
        }
        let in_class_body = class_body_depths.last() == Some(&depth);

        let is_class_decl = t.starts_with("class ") || t.starts_with("enum ");
        if is_class_decl {
            units.push(format!("decl|{}", normalize(t)));
        } else if in_class_body
            && !t.is_empty()
            && !t.starts_with("//")
            && !t.starts_with("function ")
            && t != "{"
            && t != "}"
        {
            // A direct child of a class body that isn't a method → a field.
            units.push(format!("field|{}", normalize(t)));
        }

        // Count braces without `as` casts (a line never holds i32::MAX braces).
        let depth_before = depth;
        for b in raw.bytes() {
            match b {
                b'{' => depth += 1,
                b'}' => depth -= 1,
                _ => {}
            }
        }
        if is_class_decl && depth > depth_before {
            class_body_depths.push(depth_before + 1);
        }
    }

    units.sort();
    units.dedup();
    units
}

/// Relevant surface across the whole stdlib, each unit prefixed by its file.
fn relevant_units() -> Vec<String> {
    let mut units = Vec::new();
    for (rel, bytes) in collect_inputs() {
        let text = String::from_utf8_lossy(&bytes);
        for unit in extract_relevant(&text) {
            units.push(format!("{rel}\u{1}{unit}"));
        }
    }
    units.sort();
    units
}

/// FNV-1a over the codegen-relevant signatures (see [`extract_relevant`]).
fn relevant_hash() -> u64 {
    let mut h = FNV_OFFSET;
    for unit in relevant_units() {
        h = fnv1a(unit.as_bytes(), h);
        h = fnv1a(&[0x00], h);
    }
    h
}

/// Per-file hash: the relevant-surface hash mixed with the generated file's own
/// crates-relative path, so each generated file carries a distinct value.
pub fn file_hash(rel_path: &str) -> String {
    let h = fnv1a(rel_path.as_bytes(), relevant_hash());
    format!("{h:016x}")
}

/// The full comment header `tools_rustgen` prepends to each generated file.
pub fn header(rel_path: &str) -> String {
    let owning_crate = rel_path.split('/').next().unwrap_or("");
    format!(
        "// @generated by `cargo run -p tools_rustgen` (or `mise run codegen`) — DO NOT EDIT.\n\
         // Generated from {GENERATED_FROM}.\n\
         // {HASH_PREFIX}{hash}\n\
         // `{owning_crate}/build.rs` recomputes this hash on every build and fails the\n\
         // compile if baml_std changed without regenerating — run the command above\n\
         // and commit the result.\n",
        hash = file_hash(rel_path),
    )
}

/// Extract the embedded hash from a generated file's header, if present.
pub fn embedded_hash(content: &str) -> Option<String> {
    content.lines().find_map(|line| {
        line.trim_start_matches('/')
            .trim()
            .strip_prefix(HASH_PREFIX)
            .map(|h| h.trim().to_string())
    })
}

/// For `build.rs`: tell Cargo to re-run the staleness check whenever the
/// `baml_std` source of truth changes (otherwise an edited `.baml` file would
/// only be caught on a clean build). Emits `rerun-if-changed` for the
/// `baml_std` tree and every file under it.
// Emitting `cargo:` directives on stdout is exactly this helper's job.
#[allow(clippy::print_stdout)]
pub fn rerun_if_baml_std_changed() {
    let root = baml_std_dir();
    println!("cargo:rerun-if-changed={}", root.display());
    for (rel, _) in collect_inputs() {
        println!("cargo:rerun-if-changed={}", root.join(rel).display());
    }
}

/// For `build.rs`: assert the checked-in generated file at `rel_path` (relative
/// to `crates/`) is still in sync with the current `baml_std` builtins, by
/// comparing its embedded hash against a freshly-computed one. Panics — failing
/// the build — if the file is missing its header or is stale, pointing the dev
/// at `cargo run -p tools_rustgen`. Also emits `rerun-if-changed` for the file.
// Emitting the `cargo:rerun-if-changed` directive on stdout is intended here.
#[allow(clippy::print_stdout)]
pub fn assert_generated_matches_baml_std(rel_path: &str) {
    let abs = crates_dir().join(rel_path);
    println!("cargo:rerun-if-changed={}", abs.display());

    let content = std::fs::read_to_string(&abs).unwrap_or_else(|e| {
        panic!("baml_rustgen_check: cannot read generated file {rel_path}: {e}")
    });
    let found = embedded_hash(&content).unwrap_or_else(|| {
        panic!(
            "baml_rustgen_check: no `{HASH_PREFIX}` header in {rel_path} — \
             run `cargo run -p tools_rustgen` and commit the result"
        )
    });
    let expected = file_hash(rel_path);
    assert!(
        found == expected,
        "baml_rustgen_check: STALE generated file {rel_path}\n  \
         baml_std hash is now {expected}, but the file was generated for {found}.\n  \
         Run `cargo run -p tools_rustgen` (or `mise run codegen`) and commit the result."
    );
}

#[cfg(test)]
mod tests {
    use super::extract_relevant;

    // ---- builtin functions DO trigger (signature + marker + directives) ----

    #[test]
    fn each_marker_is_captured() {
        for marker in super::MARKERS {
            let src = format!("function foo(a: int) -> int {{\n  {marker}\n}}\n");
            let units = extract_relevant(&src);
            assert_eq!(units.len(), 1, "marker {marker} not captured: {units:?}");
            assert!(
                units[0].contains("function foo(a: int) -> int"),
                "signature missing for {marker}: {units:?}"
            );
            assert!(units[0].contains(marker), "marker token missing: {units:?}");
        }
    }

    #[test]
    fn marker_on_same_line_as_signature() {
        let src = "function now() -> Instant throws never { $rust_io_function }\n";
        let units = extract_relevant(src);
        assert_eq!(units.len(), 1, "{units:?}");
        assert!(units[0].contains("function now() -> Instant throws never"));
    }

    #[test]
    fn multi_line_signature_is_fully_captured() {
        let a = "function f(\n  a: int,\n  b: int,\n) -> int {\n  $rust_io_function\n}\n";
        let b = "function f(\n  a: int,\n  b: string,\n) -> int {\n  $rust_io_function\n}\n";
        assert_ne!(
            extract_relevant(a),
            extract_relevant(b),
            "multi-line sig change missed"
        );
    }

    #[test]
    fn editing_a_marked_signature_changes_units() {
        let a = "function f(a: int) -> int {\n  $rust_function\n}\n";
        let b = "function f(a: string) -> int {\n  $rust_function\n}\n";
        assert_ne!(extract_relevant(a), extract_relevant(b));
    }

    #[test]
    fn changing_the_throws_clause_changes_units() {
        let a = "function f() -> int throws root.errors.Io {\n  $rust_io_function\n}\n";
        let b = "function f() -> int throws never {\n  $rust_io_function\n}\n";
        assert_ne!(extract_relevant(a), extract_relevant(b));
    }

    #[test]
    fn switching_marker_kind_changes_units() {
        let a = "function f() -> int {\n  $rust_function\n}\n";
        let b = "function f() -> int {\n  $rust_io_function\n}\n";
        assert_ne!(extract_relevant(a), extract_relevant(b));
    }

    #[test]
    fn adding_or_removing_a_marked_function_changes_units() {
        let a = "function f() -> int {\n  $rust_function\n}\n";
        let b = "function f() -> int {\n  $rust_function\n}\n\
                 function g() -> string {\n  $rust_function\n}\n";
        let (ua, ub) = (extract_relevant(a), extract_relevant(b));
        assert_eq!(ub.len(), ua.len() + 1);
    }

    #[test]
    fn renaming_a_marked_function_changes_units() {
        let a = "function old_name() -> int {\n  $rust_function\n}\n";
        let b = "function new_name() -> int {\n  $rust_function\n}\n";
        assert_ne!(extract_relevant(a), extract_relevant(b));
    }

    // ---- //baml: directives ARE part of the surface (they change codegen) ----

    #[test]
    fn changing_a_directive_changes_units() {
        let a = "//baml:vm\nfunction f(self) -> int {\n  $rust_function\n}\n";
        let b = "//baml:mut_vm\nfunction f(self) -> int {\n  $rust_function\n}\n";
        assert_ne!(
            extract_relevant(a),
            extract_relevant(b),
            "directive change missed"
        );
    }

    #[test]
    fn adding_a_directive_changes_units() {
        let a = "function f(self) -> int {\n  $rust_function\n}\n";
        let b = "//baml:may_yield\n//baml:mut_vm\nfunction f(self) -> int {\n  $rust_function\n}\n";
        assert_ne!(
            extract_relevant(a),
            extract_relevant(b),
            "added directive missed"
        );
    }

    // ---- classes + fields ARE part of the surface ----

    #[test]
    fn class_declaration_and_fields_are_captured() {
        // BAML fields are `name Type` (no colon), e.g. error classes.
        let src = "class Io {\n  message string\n  code int?\n}\n";
        let units = extract_relevant(src);
        assert!(units.iter().any(|u| u.contains("class Io")), "{units:?}");
        assert!(
            units.iter().any(|u| u.contains("message string")),
            "{units:?}"
        );
        assert!(units.iter().any(|u| u.contains("code int?")), "{units:?}");
    }

    #[test]
    fn changing_a_class_field_changes_units() {
        let a = "class Timeout {\n  message string\n}\n";
        let b = "class Timeout {\n  message int\n}\n";
        assert_ne!(
            extract_relevant(a),
            extract_relevant(b),
            "class field change missed"
        );
    }

    #[test]
    fn adding_a_class_field_changes_units() {
        let a = "class Timeout {\n  message string\n}\n";
        let b = "class Timeout {\n  message string\n  duration_ms int?\n}\n";
        assert_ne!(
            extract_relevant(a),
            extract_relevant(b),
            "added field missed"
        );
    }

    #[test]
    fn renaming_a_class_changes_units() {
        let a = "class Old {\n  message string\n}\n";
        let b = "class New {\n  message string\n}\n";
        assert_ne!(extract_relevant(a), extract_relevant(b));
    }

    #[test]
    fn class_with_builtin_method_captures_field_and_method_but_not_body() {
        // Models `class File { _handle $rust_type; function text(self) {...} }`.
        let src = "\
class File {\n\
  _handle $rust_type\n\
  function text(self) -> string throws root.errors.Io {\n\
    $rust_io_function\n\
  }\n\
}\n";
        let units = extract_relevant(src);
        assert!(
            units
                .iter()
                .any(|u| u.starts_with("decl|") && u.contains("class File"))
        );
        assert!(
            units
                .iter()
                .any(|u| u.starts_with("field|") && u.contains("_handle $rust_type"))
        );
        assert!(
            units
                .iter()
                .any(|u| u.starts_with("fn|") && u.contains("function text(self)"))
        );
        // The marker body line is part of the fn unit, never a standalone field.
        assert!(
            !units
                .iter()
                .any(|u| u.starts_with("field|") && u.contains("$rust_io_function"))
        );
    }

    // ---- pure-BAML functions/methods generate NOTHING and must be dropped ----

    #[test]
    fn pure_baml_free_function_is_dropped() {
        let src = "function pure(a: int) -> int {\n  return a + 1\n}\n";
        assert!(
            extract_relevant(src).is_empty(),
            "{:?}",
            extract_relevant(src)
        );
    }

    #[test]
    fn editing_a_pure_baml_free_function_is_a_noop() {
        let a = "function marked() -> int {\n  $rust_function\n}\n\
                 function pure(a: int) -> int {\n  return a\n}\n";
        let b = "function marked() -> int {\n  $rust_function\n}\n\
                 function pure(a: string, b: bool) -> string {\n  return b + a\n}\n";
        assert_eq!(extract_relevant(a), extract_relevant(b));
    }

    #[test]
    fn editing_a_pure_baml_method_body_inside_a_class_is_a_noop() {
        // A pure-BAML method (no marker) inside a class: its signature and body
        // generate nothing, so editing the body must not change the surface —
        // while the class field is still captured.
        let a = "\
class Wrapper {\n\
  inner int\n\
  function helper(self) -> int {\n\
    return self.inner + 1\n\
  }\n\
}\n";
        let b = "\
class Wrapper {\n\
  inner int\n\
  function helper(self) -> int {\n\
    let x = self.inner\n\
    return x * 2 - 1\n\
  }\n\
}\n";
        assert_eq!(
            extract_relevant(a),
            extract_relevant(b),
            "pure method body leaked into surface"
        );
        // ...but the field IS captured.
        assert!(extract_relevant(a).iter().any(|u| u.contains("inner int")));
    }

    // ---- noise is ignored ----

    #[test]
    fn editing_doc_comments_does_not_change_units() {
        let a = "/// docs v1\nfunction f() -> int {\n  $rust_function\n}\n";
        let b = "/// rewritten\n/// with more lines\nfunction f() -> int {\n  $rust_function\n}\n";
        assert_eq!(extract_relevant(a), extract_relevant(b));
    }

    #[test]
    fn marker_inside_a_comment_is_ignored() {
        let src = "\
/// Implemented via $rust_io_function under the hood.\n\
// TODO: make $rust_function faster\n\
function documented(a: int) -> int {\n\
  return a // not a real $compiler_intrinsic\n\
}\n";
        assert!(
            extract_relevant(src).is_empty(),
            "comment markers must be ignored: {:?}",
            extract_relevant(src)
        );
    }

    #[test]
    fn reindenting_does_not_change_units() {
        let a = "function f(a: int) -> int {\n  $rust_function\n}\n";
        let b = "function    f(a: int)    ->    int {\n      $rust_function\n}\n";
        assert_eq!(extract_relevant(a), extract_relevant(b));
    }

    #[test]
    fn url_in_signature_default_is_not_truncated() {
        let a = "function f(base: string = \"http://a\") -> int {\n  $rust_io_function\n}\n";
        let b = "function f(base: string = \"http://b\") -> int {\n  $rust_io_function\n}\n";
        assert_ne!(extract_relevant(a), extract_relevant(b));
        assert!(extract_relevant(a)[0].contains("http://a"));
    }

    #[test]
    fn word_starting_with_function_is_not_mistaken_for_a_signature() {
        let src = "let functions_count = 3\nfunction real() -> int {\n  $rust_function\n}\n";
        let units = extract_relevant(src);
        assert_eq!(units.len(), 1);
        assert!(units[0].contains("function real()"), "{units:?}");
    }
}
