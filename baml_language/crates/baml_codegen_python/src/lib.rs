//! Phase G1 Python SDK emitter: produces a structurally correct
//! `baml_sdk/` tree from a `SymbolPool`. Leaves are symbol-empty in
//! G1 — content fills in across G2–G5.
//!
//! See `.humanlayer/tasks/clientpython/11c-phaseg1-scaffolding.md`.

mod routing;

use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    fmt::Write as _,
    path::PathBuf,
};

use baml_codegen_types::{Name, Symbol, SymbolPool};

use crate::routing::{LeafPath, route};

const HEADER: &str = "from __future__ import annotations\n";

/// A user BAML source file as it should appear in `_inlinedbaml.py`.
/// `rel_path` is relative to the `baml_src/` root (e.g. `"lorem/foo.baml"`).
pub type UserBamlFile = (PathBuf, String);

/// Build the Python SDK output tree for `pool`. Returned paths are
/// relative to the `baml_sdk/` output root.
pub fn to_source_code(
    pool: &SymbolPool,
    user_baml_files: &[UserBamlFile],
) -> HashMap<PathBuf, String> {
    let mut out: HashMap<PathBuf, String> = HashMap::new();

    // Every symbol in the pool routes to exactly one leaf. Dedup via
    // `BTreeSet` so leaf and directory enumeration below is stable.
    let mut leaves: BTreeSet<LeafPath> = BTreeSet::new();
    for key in pool.keys() {
        leaves.insert(route(key));
    }

    // `baml/` always exists (hosts `_inlinedbaml.py`), even if no
    // stdlib symbols routed there — the root init imports `_inlinedbaml`
    // from it. The root leaf itself is always emitted as well.
    leaves.insert(LeafPath {
        segments: vec!["baml".to_string()],
    });
    leaves.insert(LeafPath {
        segments: Vec::new(),
    });

    // Walk every leaf's ancestor chain to discover all directories that
    // need an `__init__.py` and the set of immediate subdirectory
    // children for each directory. A single directory may be both a
    // routed leaf AND have subdirectory children (e.g. `stream_types/`
    // when there are no-namespace `root..Foo$stream` symbols alongside
    // namespaced stream symbols). Those cases merge into a single
    // `__init__.py` emission below.
    let mut all_dirs: BTreeSet<Vec<String>> = BTreeSet::new();
    let mut children: BTreeMap<Vec<String>, BTreeSet<String>> = BTreeMap::new();

    children.entry(Vec::new()).or_default();
    all_dirs.insert(Vec::new());

    for leaf in &leaves {
        all_dirs.insert(leaf.segments.clone());
        for i in 0..leaf.segments.len() {
            let prefix: Vec<String> = leaf.segments[..i].to_vec();
            children
                .entry(prefix.clone())
                .or_default()
                .insert(leaf.segments[i].clone());
            all_dirs.insert(prefix);
        }
    }

    // Emit every directory's `__init__.py`. Root gets the runtime-init
    // shape; every other dir gets `<header> + optional re-export`.
    // Both leaves and pure interiors end up with the same rendering
    // in G1 (leaves are symbol-empty here).
    for dir in &all_dirs {
        let kids = children.get(dir).cloned().unwrap_or_default();
        let path = init_py_path(dir);

        let content = if dir.is_empty() {
            render_root_init(&kids)
        } else {
            render_package_init(&kids)
        };
        out.insert(path, content);
    }

    // Emit `baml/_inlinedbaml.py`.
    out.insert(
        PathBuf::from("baml/_inlinedbaml.py"),
        render_inlinedbaml(user_baml_files),
    );

    // Emit PEP 561 marker.
    out.insert(PathBuf::from("py.typed"), String::new());

    out
}

fn init_py_path(dir: &[String]) -> PathBuf {
    let mut path = PathBuf::new();
    for seg in dir {
        path.push(seg);
    }
    path.push("__init__.py");
    path
}

/// Render a non-root package `__init__.py`. Contains the uniform
/// `from __future__ …` header and, if the directory has subdirectory
/// children, a single re-export line. G1 never emits symbol content.
fn render_package_init(children: &BTreeSet<String>) -> String {
    let mut s = String::from(HEADER);
    if !children.is_empty() {
        s.push('\n');
        let list: Vec<&str> = children.iter().map(String::as_str).collect();
        writeln!(s, "from . import {}", list.join(", ")).unwrap();
    }
    s
}

fn render_root_init(top_children: &BTreeSet<String>) -> String {
    // Exclude `_inlinedbaml` if somehow it leaked into the top-level
    // children (it's a file under baml/, not a top-level dir — belt and
    // suspenders).
    let mut s = String::new();
    s.push_str(HEADER);
    s.push('\n');
    s.push_str("from baml.baml_core import BamlRuntime\n");
    s.push_str("from .baml import _inlinedbaml\n");
    s.push('\n');
    s.push_str(
        "BamlRuntime.initialize_runtime(\n    \"baml_src\", _inlinedbaml.FILES, sdk_root=__name__\n)\n",
    );

    if !top_children.is_empty() {
        let list: Vec<&str> = top_children.iter().map(String::as_str).collect();
        s.push('\n');
        writeln!(s, "from . import {}", list.join(", ")).unwrap();
    }

    s
}

fn render_inlinedbaml(files: &[UserBamlFile]) -> String {
    let mut entries: Vec<(&PathBuf, &String)> = files.iter().map(|(p, c)| (p, c)).collect();
    entries.sort_by(|a, b| a.0.cmp(b.0));

    let mut s = String::from(HEADER);
    s.push('\n');
    s.push_str("FILES: dict[str, str] = {\n");
    for (rel, contents) in entries {
        let key = rel.to_string_lossy();
        writeln!(s, "    {}: {},", py_string(&key), py_string(contents)).unwrap();
    }
    s.push_str("}\n");
    s
}

/// Render `s` as a Python string literal. Uses a regular double-quoted
/// form with the usual `\\`, `\"`, `\n`, `\r`, `\t` escapes so the result
/// round-trips through `ast.literal_eval` and is byte-identical.
fn py_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                write!(out, "\\x{:02x}", c as u32).unwrap();
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Return the `cg::Name` that keys a given symbol in the pool. Useful
/// for future phases; not load-bearing in G1 (we iterate `pool.keys()`
/// directly), but kept near the emitter for symmetry with `SymbolPool`.
#[allow(dead_code)]
fn symbol_name(sym: &Symbol) -> Option<&Name> {
    match sym {
        Symbol::Class(c) => Some(&c.name),
        Symbol::Enum(e) => Some(&e.name),
        Symbol::TypeAlias(t) => Some(&t.name),
        // `Function.name` is a bare `baml_base::Name`; the pool key is
        // authoritative. Return None so callers keep using keys.
        Symbol::Function(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use baml_base::Name as BaseName;
    use baml_codegen_types::{Class, ClassProperty, Origin, Ty};

    fn cg_name(pkg: &str, ns: &[&str], n: &str) -> Name {
        Name::new(
            BaseName::new(pkg),
            ns.iter().map(|s| BaseName::new(*s)).collect(),
            BaseName::new(n),
        )
    }

    fn class(name: Name) -> Symbol {
        Symbol::Class(Class {
            name: name.clone(),
            docstring: None,
            properties: vec![ClassProperty {
                name: BaseName::new("a"),
                docstring: None,
                ty: Ty::Int,
            }],
            origin: Origin {
                source_file_path: "x.baml".to_string(),
                span_start: 0,
            },
        })
    }

    #[test]
    fn empty_pool_emits_structural_files() {
        let pool: SymbolPool = HashMap::new();
        let out = to_source_code(&pool, &[]);

        // Root init, baml/ interior, baml/_inlinedbaml.py, py.typed.
        assert!(out.contains_key(&PathBuf::from("__init__.py")));
        assert!(out.contains_key(&PathBuf::from("baml/__init__.py")));
        assert!(out.contains_key(&PathBuf::from("baml/_inlinedbaml.py")));
        assert!(out.contains_key(&PathBuf::from("py.typed")));

        let root = &out[&PathBuf::from("__init__.py")];
        assert!(root.contains("from baml.baml_core import BamlRuntime"));
        assert!(root.contains(
            "BamlRuntime.initialize_runtime(\n    \"baml_src\", _inlinedbaml.FILES, sdk_root=__name__\n)"
        ));
        // Root must reference `baml` (always a top-level child).
        assert!(root.contains("from . import baml"));

        // `baml/__init__.py` must NOT re-export `_inlinedbaml`.
        let baml_init = &out[&PathBuf::from("baml/__init__.py")];
        assert!(!baml_init.contains("_inlinedbaml"));

        assert_eq!(out[&PathBuf::from("py.typed")], "");
    }

    #[test]
    fn user_with_ns_emits_leaf() {
        let mut pool: SymbolPool = HashMap::new();
        let n = cg_name("user", &["lorem"], "Resume");
        pool.insert(n.clone(), class(n));

        let out = to_source_code(&pool, &[]);

        let leaf = out.get(&PathBuf::from("lorem/__init__.py")).unwrap();
        assert_eq!(leaf, HEADER);

        // Root init should re-export `lorem` and `baml`.
        let root = &out[&PathBuf::from("__init__.py")];
        assert!(root.contains("from . import baml, lorem"));
    }

    #[test]
    fn vendor_creates_interior_dirs() {
        let mut pool: SymbolPool = HashMap::new();
        let n = cg_name("aws", &["s3"], "Bucket");
        pool.insert(n.clone(), class(n));

        let out = to_source_code(&pool, &[]);

        assert!(out.contains_key(&PathBuf::from("vendor/__init__.py")));
        assert!(out.contains_key(&PathBuf::from("vendor/aws/__init__.py")));
        assert!(out.contains_key(&PathBuf::from("vendor/aws/s3/__init__.py")));

        let vendor_init = &out[&PathBuf::from("vendor/__init__.py")];
        assert!(vendor_init.contains("from . import aws"));
        let aws_init = &out[&PathBuf::from("vendor/aws/__init__.py")];
        assert!(aws_init.contains("from . import s3"));

        // s3 is a leaf, not an interior → symbol-empty body.
        let s3_leaf = &out[&PathBuf::from("vendor/aws/s3/__init__.py")];
        assert_eq!(s3_leaf, HEADER);
    }

    #[test]
    fn stream_variant_under_stream_types() {
        let mut pool: SymbolPool = HashMap::new();
        let n = cg_name("user", &["lorem"], "Resume$stream");
        pool.insert(n.clone(), class(n));

        let out = to_source_code(&pool, &[]);

        assert!(out.contains_key(&PathBuf::from("stream_types/__init__.py")));
        assert!(out.contains_key(&PathBuf::from("stream_types/lorem/__init__.py")));

        let root = &out[&PathBuf::from("__init__.py")];
        assert!(root.contains("stream_types"));
    }

    #[test]
    fn dir_that_is_both_leaf_and_interior_reexports_children() {
        // `user..Foo$stream` routes to the root stream leaf; also a
        // `user.lorem.Resume$stream` gives `stream_types/` a subdirectory
        // child `lorem`. `stream_types/__init__.py` must re-export
        // `lorem` even though it's also a leaf itself.
        let mut pool: SymbolPool = HashMap::new();
        let no_ns = cg_name("user", &[], "Foo$stream");
        let with_ns = cg_name("user", &["lorem"], "Resume$stream");
        pool.insert(no_ns.clone(), class(no_ns));
        pool.insert(with_ns.clone(), class(with_ns));

        let out = to_source_code(&pool, &[]);

        let stream_root = &out[&PathBuf::from("stream_types/__init__.py")];
        assert!(stream_root.starts_with(HEADER));
        assert!(stream_root.contains("from . import lorem"));
    }

    #[test]
    fn inlinedbaml_round_trips() {
        let pool: SymbolPool = HashMap::new();
        let files = vec![
            (PathBuf::from("main.baml"), "class Foo {}\n".to_string()),
            (
                PathBuf::from("lorem/bar.baml"),
                "function foo() -> int { 1 }\n".to_string(),
            ),
        ];
        let out = to_source_code(&pool, &files);

        let inl = &out[&PathBuf::from("baml/_inlinedbaml.py")];
        assert!(inl.starts_with("from __future__ import annotations\n"));
        assert!(inl.contains("FILES: dict[str, str] = {"));
        // Alphabetical: lorem/bar.baml comes before main.baml.
        let lo = inl.find("lorem/bar.baml").unwrap();
        let mo = inl.find("main.baml").unwrap();
        assert!(lo < mo);
        // Quoted values.
        assert!(inl.contains("\"class Foo {}\\n\""));
    }

    #[test]
    fn py_string_escapes() {
        assert_eq!(py_string("hello"), "\"hello\"");
        assert_eq!(py_string("a\\b"), "\"a\\\\b\"");
        assert_eq!(py_string("a\"b"), "\"a\\\"b\"");
        assert_eq!(py_string("a\nb"), "\"a\\nb\"");
    }

    #[test]
    fn no_legacy_output_paths() {
        let pool: SymbolPool = HashMap::new();
        let out = to_source_code(&pool, &[]);
        for path in out.keys() {
            let s = path.to_string_lossy();
            assert!(!s.starts_with("baml_types/"));
            assert!(!s.starts_with("baml_stream_types/"));
            assert!(!s.starts_with("baml_sync/"));
            assert!(!s.starts_with("baml_async/"));
            assert!(!s.contains("inlinedbaml.py") || s == "baml/_inlinedbaml.py");
            assert_ne!(s, "runtime.py");
            assert_ne!(s, "config.py");
            assert_ne!(s, "globals.py");
            assert_ne!(s, "tracing.py");
        }
    }
}
