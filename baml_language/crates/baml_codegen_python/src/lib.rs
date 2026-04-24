//! Phase G2 Python SDK emitter.
//!
//! Produces a structurally correct `baml_sdk/` tree from a
//! `SymbolPool`, with placeholder bodies at every routed leaf. The
//! tree shape is unchanged from G1; each leaf that routes at least one
//! symbol now carries stub Python definitions plus an `__all__`
//! trailer.
//!
//! See `.humanlayer/tasks/clientpython/11d-phaseg2-stub-types.md`.

mod emit;
mod leaf;
mod routing;
#[allow(dead_code)]
mod translate_ty;

use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    fmt::Write as _,
    path::PathBuf,
};

use baml_codegen_types::{Name, Symbol, SymbolPool};

use crate::{
    emit::build_emitted,
    leaf::{LeafBody, group_and_sort, render_leaf_body},
    routing::{LeafPath, route},
};

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

    // Build the populated-leaf bodies. Every directory that gets
    // at least one routed symbol ends up with a `LeafBody` here; all
    // others render with G1-identical content.
    let triples = build_emitted(pool);
    let bodies: BTreeMap<LeafPath, LeafBody> = group_and_sort(triples);

    let empty_body = LeafBody {
        symbols: Vec::new(),
    };

    // Emit every directory's `__init__.py`.
    for dir in &all_dirs {
        let kids = children.get(dir).cloned().unwrap_or_default();
        let path = init_py_path(dir);
        let leaf_path = LeafPath {
            segments: dir.clone(),
        };
        let body = bodies.get(&leaf_path).unwrap_or(&empty_body);

        let mut content = if dir.is_empty() {
            render_root_init(&kids)
        } else {
            render_package_init(&kids)
        };
        content.push_str(&render_leaf_body(body));
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
/// children, a single re-export line. Symbol content is appended
/// separately by the caller via `render_leaf_body`.
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
pub(crate) fn py_string(s: &str) -> String {
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
/// for future phases; not load-bearing in G2 (we iterate `pool.keys()`
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
    use baml_base::Name as BaseName;
    use baml_codegen_types::{
        Class, ClassProperty, Enum, EnumVariant, Function, FunctionArgument, Origin, Ty, TypeAlias,
    };

    use super::*;

    fn cg_name(pkg: &str, ns: &[&str], n: &str) -> Name {
        Name::new(
            BaseName::new(pkg),
            ns.iter().map(|s| BaseName::new(*s)).collect(),
            BaseName::new(n),
        )
    }

    fn origin(file: &str, span: u32) -> Origin {
        Origin {
            source_file_path: file.to_string(),
            span_start: span,
        }
    }

    fn class(name: Name) -> Symbol {
        class_at(name, "x.baml", 0)
    }

    fn class_at(name: Name, file: &str, span: u32) -> Symbol {
        Symbol::Class(Class {
            name: name.clone(),
            docstring: None,
            properties: vec![ClassProperty {
                name: BaseName::new("a"),
                docstring: None,
                ty: Ty::Int,
            }],
            origin: origin(file, span),
        })
    }

    fn enum_(name: Name, file: &str, span: u32) -> Symbol {
        Symbol::Enum(Enum {
            name,
            docstring: None,
            variants: vec![EnumVariant {
                name: BaseName::new("A"),
                docstring: None,
                value: "A".to_string(),
            }],
            origin: origin(file, span),
        })
    }

    fn alias(name: Name, file: &str, span: u32) -> Symbol {
        Symbol::TypeAlias(TypeAlias {
            name,
            resolves_to: Ty::Int,
            recursive: false,
            origin: origin(file, span),
        })
    }

    fn bare_func(bare: &str, file: &str, span: u32) -> Function {
        Function {
            name: BaseName::new(bare),
            docstring: None,
            arguments: vec![FunctionArgument {
                name: BaseName::new("x"),
                docstring: None,
                ty: Ty::Int,
            }],
            return_type: Ty::Int,
            stream_return_type: None,
            watchers: vec![],
            companions: vec![],
            origin: origin(file, span),
        }
    }

    fn func_sym(bare: &str, file: &str, span: u32, companions: Vec<(&str, Function)>) -> Symbol {
        let mut f = bare_func(bare, file, span);
        f.companions = companions
            .into_iter()
            .map(|(s, f)| (s.to_string(), f))
            .collect();
        Symbol::Function(f)
    }

    #[test]
    fn empty_pool_emits_structural_files() {
        let pool: SymbolPool = HashMap::new();
        let out = to_source_code(&pool, &[]);

        assert!(out.contains_key(&PathBuf::from("__init__.py")));
        assert!(out.contains_key(&PathBuf::from("baml/__init__.py")));
        assert!(out.contains_key(&PathBuf::from("baml/_inlinedbaml.py")));
        assert!(out.contains_key(&PathBuf::from("py.typed")));

        let root = &out[&PathBuf::from("__init__.py")];
        assert!(root.contains("from baml.baml_core import BamlRuntime"));
        assert!(root.contains("from . import baml"));
        // No symbols → no __all__ emitted (preserves G1 byte shape).
        assert!(!root.contains("__all__"));

        // `baml/__init__.py` must NOT re-export `_inlinedbaml`.
        let baml_init = &out[&PathBuf::from("baml/__init__.py")];
        assert!(!baml_init.contains("_inlinedbaml"));
        assert_eq!(baml_init, HEADER);

        assert_eq!(out[&PathBuf::from("py.typed")], "");
    }

    #[test]
    fn class_placeholder_renders() {
        let mut pool: SymbolPool = HashMap::new();
        let n = cg_name("user", &["lorem"], "Resume");
        pool.insert(n.clone(), class(n));

        let out = to_source_code(&pool, &[]);

        let leaf = &out[&PathBuf::from("lorem/__init__.py")];
        assert!(leaf.starts_with(HEADER));
        assert!(leaf.contains("class Resume: pass\n"));
        assert!(leaf.contains("__all__ = [\n    \"Resume\",\n]\n"));
        assert!(!leaf.contains("import enum"));
        assert!(!leaf.contains("import typing"));
    }

    #[test]
    fn enum_placeholder_adds_enum_import() {
        let mut pool: SymbolPool = HashMap::new();
        let n = cg_name("user", &["lorem"], "Sentiment");
        pool.insert(n.clone(), enum_(n, "x.baml", 0));

        let out = to_source_code(&pool, &[]);
        let leaf = &out[&PathBuf::from("lorem/__init__.py")];
        assert!(leaf.contains("import enum"));
        assert!(leaf.contains("class Sentiment(str, enum.Enum): pass"));
    }

    #[test]
    fn type_alias_placeholder_adds_typing_import() {
        let mut pool: SymbolPool = HashMap::new();
        let n = cg_name("user", &["lorem"], "Foo");
        pool.insert(n.clone(), alias(n, "x.baml", 0));

        let out = to_source_code(&pool, &[]);
        let leaf = &out[&PathBuf::from("lorem/__init__.py")];
        assert!(leaf.contains("import typing"));
        assert!(leaf.contains("Foo: typing.TypeAlias = typing.Any"));
    }

    #[test]
    fn function_fans_out_sync_and_async() {
        let mut pool: SymbolPool = HashMap::new();
        let n = cg_name("user", &["lorem"], "extract_resume");
        pool.insert(n, func_sym("extract_resume", "x.baml", 0, vec![]));

        let out = to_source_code(&pool, &[]);
        let leaf = &out[&PathBuf::from("lorem/__init__.py")];
        assert!(leaf.contains("extract_resume = None"));
        assert!(leaf.contains("extract_resume_async = None"));
        assert!(!leaf.contains("extract_resume_stream"));

        // Fan-out siblings should be adjacent (no blank between).
        let idx_sync = leaf.find("extract_resume = None\n").unwrap();
        let idx_async = leaf.find("extract_resume_async = None\n").unwrap();
        let between = &leaf[idx_sync + "extract_resume = None\n".len()..idx_async];
        assert_eq!(between, "");
    }

    #[test]
    fn function_with_stream_companion() {
        let mut pool: SymbolPool = HashMap::new();
        let n = cg_name("user", &["lorem"], "extract_resume");
        let companion = bare_func("extract_resume", "x.baml", 0);
        pool.insert(
            n,
            func_sym("extract_resume", "x.baml", 0, vec![("stream", companion)]),
        );

        let out = to_source_code(&pool, &[]);
        let leaf = &out[&PathBuf::from("lorem/__init__.py")];
        assert!(leaf.contains("extract_resume_stream = None"));
        assert!(leaf.contains("extract_resume_stream_async = None"));
    }

    #[test]
    fn function_with_build_request_companion_uses_double_underscore() {
        let mut pool: SymbolPool = HashMap::new();
        let n = cg_name("user", &["lorem"], "extract_resume");
        let companion = bare_func("extract_resume", "x.baml", 0);
        pool.insert(
            n,
            func_sym(
                "extract_resume",
                "x.baml",
                0,
                vec![("build_request", companion)],
            ),
        );

        let out = to_source_code(&pool, &[]);
        let leaf = &out[&PathBuf::from("lorem/__init__.py")];
        assert!(leaf.contains("extract_resume__build_request = None"));
        assert!(leaf.contains("extract_resume__build_request_async = None"));
    }

    #[test]
    fn stream_class_routes_to_stream_types() {
        let mut pool: SymbolPool = HashMap::new();
        let n = cg_name("user", &["lorem"], "Resume$stream");
        pool.insert(n.clone(), class(n));

        let out = to_source_code(&pool, &[]);
        let leaf = &out[&PathBuf::from("stream_types/lorem/__init__.py")];
        assert!(leaf.contains("class Resume: pass\n"));
        assert!(!leaf.contains("Resume$stream"));

        // The non-stream `lorem/` dir isn't emitted — no non-stream
        // user.lorem symbols routed here.
        assert!(!out.contains_key(&PathBuf::from("lorem/__init__.py")));
    }

    #[test]
    fn source_order_sorting() {
        // Two classes in the same file at different spans should render
        // in span order, regardless of insertion order into the pool.
        let mut pool: SymbolPool = HashMap::new();
        let late = cg_name("user", &["lorem"], "Bar");
        let early = cg_name("user", &["lorem"], "Foo");
        pool.insert(late.clone(), class_at(late, "x.baml", 200));
        pool.insert(early.clone(), class_at(early, "x.baml", 100));

        let out = to_source_code(&pool, &[]);
        let leaf = &out[&PathBuf::from("lorem/__init__.py")];
        let idx_foo = leaf.find("class Foo: pass").unwrap();
        let idx_bar = leaf.find("class Bar: pass").unwrap();
        assert!(idx_foo < idx_bar);
    }

    #[test]
    fn multi_file_interleave() {
        // Two classes from different files land in the same leaf and
        // interleave lexicographically by file path.
        let mut pool: SymbolPool = HashMap::new();
        let a = cg_name("user", &["lorem"], "A");
        let b = cg_name("user", &["lorem"], "B");
        pool.insert(a.clone(), class_at(a, "b.baml", 0));
        pool.insert(b.clone(), class_at(b, "a.baml", 0));

        let out = to_source_code(&pool, &[]);
        let leaf = &out[&PathBuf::from("lorem/__init__.py")];
        // B (a.baml) sorts before A (b.baml).
        let idx_a = leaf.find("class A: pass").unwrap();
        let idx_b = leaf.find("class B: pass").unwrap();
        assert!(idx_b < idx_a);
    }

    #[test]
    fn all_lists_public_names_only() {
        let mut pool: SymbolPool = HashMap::new();
        let c = cg_name("user", &["lorem"], "Resume");
        let e = cg_name("user", &["lorem"], "Sentiment");
        pool.insert(c.clone(), class_at(c, "x.baml", 0));
        pool.insert(e.clone(), enum_(e, "x.baml", 50));

        let out = to_source_code(&pool, &[]);
        let leaf = &out[&PathBuf::from("lorem/__init__.py")];
        assert!(leaf.contains("__all__ = [\n    \"Resume\",\n    \"Sentiment\",\n]"));
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

        // Pure interior dirs: byte-identical to G1 (no body).
        let vendor_init = &out[&PathBuf::from("vendor/__init__.py")];
        assert_eq!(
            vendor_init,
            "from __future__ import annotations\n\nfrom . import aws\n"
        );

        // Leaf carries the symbol.
        let s3_leaf = &out[&PathBuf::from("vendor/aws/s3/__init__.py")];
        assert!(s3_leaf.contains("class Bucket: pass"));
    }

    #[test]
    fn root_stub_populates_root_init() {
        let mut pool: SymbolPool = HashMap::new();
        let n = cg_name("user", &[], "Foo");
        pool.insert(n.clone(), class(n));

        let out = to_source_code(&pool, &[]);
        let root = &out[&PathBuf::from("__init__.py")];
        assert!(root.contains("BamlRuntime.initialize_runtime("));
        // Stub appended after the runtime init + re-exports.
        assert!(root.contains("class Foo: pass\n"));
        assert!(root.contains("__all__ = [\n    \"Foo\",\n]"));
    }

    #[test]
    fn no_pydantic_or_factory_imports_in_leaves() {
        let mut pool: SymbolPool = HashMap::new();
        let c = cg_name("user", &["lorem"], "Resume");
        pool.insert(c.clone(), class(c));
        let f = cg_name("user", &["lorem"], "extract_resume");
        pool.insert(f, func_sym("extract_resume", "x.baml", 100, vec![]));

        let out = to_source_code(&pool, &[]);
        for (path, content) in &out {
            let s = path.to_string_lossy();
            if !s.ends_with(".py") {
                continue;
            }
            // The root init's `from baml.baml_core import BamlRuntime`
            // is G1 scaffolding, not a G5 factory import — skip root +
            // inlinedbaml file when checking for G5-era leakage.
            let is_scaffold = s == "__init__.py" || s.ends_with("_inlinedbaml.py");
            assert!(
                !content.contains("pydantic"),
                "leaf {} contains pydantic: {}",
                path.display(),
                content
            );
            if !is_scaffold {
                assert!(
                    !content.contains("baml_core"),
                    "leaf {} contains baml_core: {}",
                    path.display(),
                    content
                );
            }
            assert!(
                !content.contains("__define_function"),
                "leaf {} contains __define_function",
                path.display()
            );
        }
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
        let mut pool: SymbolPool = HashMap::new();
        let no_ns = cg_name("user", &[], "Foo$stream");
        let with_ns = cg_name("user", &["lorem"], "Resume$stream");
        pool.insert(no_ns.clone(), class(no_ns));
        pool.insert(with_ns.clone(), class(with_ns));

        let out = to_source_code(&pool, &[]);

        let stream_root = &out[&PathBuf::from("stream_types/__init__.py")];
        assert!(stream_root.starts_with(HEADER));
        assert!(stream_root.contains("from . import lorem"));
        // And the Foo$stream stub at the top-level stream leaf.
        assert!(stream_root.contains("class Foo: pass\n"));
        assert!(stream_root.contains("__all__ = [\n    \"Foo\",\n]"));
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
        let lo = inl.find("lorem/bar.baml").unwrap();
        let mo = inl.find("main.baml").unwrap();
        assert!(lo < mo);
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
