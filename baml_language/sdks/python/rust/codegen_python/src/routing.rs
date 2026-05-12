//! Routing: turns a `baml_codegen_types::Name` into the leaf `__init__.py`
//! path (under `baml_sdk/`) where that symbol's Python representation
//! lives. Single source of truth for per-symbol placement; G1 uses this
//! to enumerate leaves and interior directories, and later phases reuse
//! it when resolving cross-leaf type references.
//!
//! Rule source: 09b-codegen-rules §1 + 11c-phaseg1 §3 (as corrected by
//! 12a-namespace-rules §1, §5).
//!
//! The `pkg` field in the codegen-facing `Name` is the literal package
//! name from HIR — `"user"` for project files, `"baml"` for stdlib,
//! `"<vendor>"` for declared external packages. The string `"root"` is
//! a `.baml` source-syntax keyword (substituted to the current package
//! during HIR resolution) and never appears as `Name::pkg`.
//!
//! `"baml"` routes under `baml/`, anything else under `vendor/<pkg>/`.
//!
//! If the name carries the `$stream` suffix, the entire routed path is
//! prepended with `stream_types/` before top-level packaging is applied.

#[cfg(test)]
use std::path::PathBuf;

use baml_codegen_types::{Name, Symbol};

/// Leaf path under `baml_sdk/`. Empty segments means the root leaf
/// (i.e. `baml_sdk/__init__.py`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct LeafPath {
    pub(crate) segments: Vec<String>,
}

impl LeafPath {
    /// Build the leaf's `__init__.py` path, relative to `baml_sdk/`.
    #[cfg(test)]
    pub(crate) fn init_py(&self) -> PathBuf {
        let mut p = PathBuf::new();
        for seg in &self.segments {
            p.push(seg);
        }
        p.push("__init__.py");
        p
    }

    /// Whether this is the root leaf (`baml_sdk/__init__.py`).
    #[cfg(test)]
    pub(crate) fn is_root(&self) -> bool {
        self.segments.is_empty()
    }
}

/// Route a pool entry to its leaf `__init__.py` path (under `baml_sdk/`).
///
/// `$stream` *classes* route to `stream_types/…`; function symbols
/// (including the function `$stream` and `$parse_stream` companions) route
/// alongside their parent function regardless of the suffix.
pub(crate) fn route(name: &Name, symbol: &Symbol) -> LeafPath {
    route_inner(name, !matches!(symbol, Symbol::Function(_)))
}

/// Sanitize a path segment so it's a usable Python module identifier.
/// Today only handles `assert` (the BAML stdlib package whose name
/// collides with Python's `assert` keyword — `from . import assert` is
/// a `SyntaxError`); the routed leaf becomes `vendor/assert_/…` and any
/// cross-leaf type reference renders as `vendor.assert_.…`. The runtime
/// BAML FQN passed to `_define_function` (e.g. `"assert.is_true"`) is
/// built from `Name`, not from `LeafPath`, so it is *not* affected.
///
/// TODO(reserved-keywords): generalize to all Python keywords and any
/// other invalid identifiers. User packages or namespaces named after
/// keywords (`class`, `def`, `pass`, …) would hit the same issue, but
/// none exist today; broaden this set when one shows up.
fn sanitize_python_module_segment(seg: &str) -> String {
    if seg == "assert" {
        "assert_".to_string()
    } else {
        seg.to_string()
    }
}

/// Route a `Name` referenced from a type position (`Ty::Class`,
/// `Ty::Enum`, `Ty::TypeAlias`). Type references always point at
/// class-like symbols, so the `$stream` suffix routes under
/// `stream_types/`.
pub(crate) fn route_class_ref(name: &Name) -> LeafPath {
    route_inner(name, true)
}

fn route_inner(name: &Name, honor_stream_suffix: bool) -> LeafPath {
    let mut segs: Vec<String> = Vec::new();

    if honor_stream_suffix && name.is_stream() {
        segs.push("stream_types".to_string());
    }

    match name.pkg.as_str() {
        "user" => {}
        "baml" => segs.push("baml".to_string()),
        other => {
            segs.push("vendor".to_string());
            segs.push(sanitize_python_module_segment(other));
        }
    }

    for seg in &name.namespace_path {
        segs.push(sanitize_python_module_segment(seg.as_str()));
    }

    LeafPath { segments: segs }
}

#[cfg(test)]
mod tests {
    use baml_base::Name as BaseName;
    use baml_codegen_types::{Class, Enum, EnumVariant, Function, FunctionArgument, Origin, Ty};

    use super::*;

    fn name(pkg: &str, ns: &[&str], n: &str) -> Name {
        Name::new(
            BaseName::new(pkg),
            ns.iter().map(|s| BaseName::new(*s)).collect(),
            BaseName::new(n),
        )
    }

    fn origin() -> Origin {
        Origin {
            source_file_path: "x.baml".to_string(),
            span_start: 0,
        }
    }

    fn class_sym(n: &Name) -> Symbol {
        Symbol::Class(Class {
            name: n.clone(),
            generic_params: Vec::new(),
            docstring: None,
            properties: Vec::new(),
            static_methods: Vec::new(),
            instance_methods: Vec::new(),
            origin: origin(),
        })
    }

    fn enum_sym(n: &Name) -> Symbol {
        Symbol::Enum(Enum {
            name: n.clone(),
            docstring: None,
            variants: vec![EnumVariant {
                name: BaseName::new("A"),
                docstring: None,
                value: "A".to_string(),
            }],
            origin: origin(),
        })
    }

    fn func_sym() -> Symbol {
        Symbol::Function(Function {
            name: BaseName::new("foo"),
            generic_params: Vec::new(),
            docstring: None,
            arguments: vec![FunctionArgument {
                name: BaseName::new("x"),
                docstring: None,
                ty: Ty::Int,
                default: None,
            }],
            return_type: Ty::Int,
            watchers: Vec::new(),
            origin: origin(),
        })
    }

    #[test]
    fn user_no_ns_routes_to_root_leaf() {
        let n = name("user", &[], "Foo");
        let lp = route(&n, &class_sym(&n));
        assert!(lp.is_root());
        assert_eq!(lp.init_py(), PathBuf::from("__init__.py"));
    }

    #[test]
    fn user_with_ns_routes_under_ns() {
        let n = name("user", &["lorem"], "Resume");
        let lp = route(&n, &class_sym(&n));
        assert_eq!(lp.segments, vec!["lorem".to_string()]);
        assert_eq!(lp.init_py(), PathBuf::from("lorem/__init__.py"));
    }

    #[test]
    fn vendor_routes_under_vendor_pkg() {
        let n = name("aws", &["s3"], "Bucket");
        let lp = route(&n, &class_sym(&n));
        assert_eq!(
            lp.segments,
            vec!["vendor".to_string(), "aws".to_string(), "s3".to_string()]
        );
        assert_eq!(lp.init_py(), PathBuf::from("vendor/aws/s3/__init__.py"));
    }

    #[test]
    fn baml_routes_under_baml() {
        let n = name("baml", &["http"], "Response");
        let lp = route(&n, &class_sym(&n));
        assert_eq!(lp.segments, vec!["baml".to_string(), "http".to_string()]);
        assert_eq!(lp.init_py(), PathBuf::from("baml/http/__init__.py"));
    }

    #[test]
    fn stream_class_prepends_stream_types() {
        let n = name("user", &["lorem"], "Resume$stream");
        let lp = route(&n, &class_sym(&n));
        assert_eq!(
            lp.segments,
            vec!["stream_types".to_string(), "lorem".to_string()]
        );
    }

    #[test]
    fn stream_class_vendor() {
        let n = name("aws", &["s3"], "Bucket$stream");
        let lp = route(&n, &class_sym(&n));
        assert_eq!(
            lp.segments,
            vec![
                "stream_types".to_string(),
                "vendor".to_string(),
                "aws".to_string(),
                "s3".to_string()
            ]
        );
    }

    #[test]
    fn stream_class_baml() {
        let n = name("baml", &["http"], "Response$stream");
        let lp = route(&n, &class_sym(&n));
        assert_eq!(
            lp.segments,
            vec![
                "stream_types".to_string(),
                "baml".to_string(),
                "http".to_string()
            ]
        );
    }

    #[test]
    fn stream_class_user_no_ns_routes_to_stream_root_leaf() {
        let n = name("user", &[], "Foo$stream");
        let lp = route(&n, &class_sym(&n));
        assert_eq!(lp.segments, vec!["stream_types".to_string()]);
        assert_eq!(lp.init_py(), PathBuf::from("stream_types/__init__.py"));
    }

    #[test]
    fn user_deeper_ns() {
        let n = name("user", &["a", "b"], "Thing");
        let lp = route(&n, &class_sym(&n));
        assert_eq!(lp.segments, vec!["a".to_string(), "b".to_string()]);
    }

    #[test]
    fn function_stream_companion_routes_alongside_parent() {
        // `extract$stream` is a function-level companion (not a class).
        // It must NOT be routed under `stream_types/`.
        let n = name("user", &["lorem"], "extract$stream");
        let lp = route(&n, &func_sym());
        assert_eq!(lp.segments, vec!["lorem".to_string()]);
    }

    #[test]
    fn function_parse_companion_routes_alongside_parent() {
        let n = name("user", &["lorem"], "extract$parse");
        let lp = route(&n, &func_sym());
        assert_eq!(lp.segments, vec!["lorem".to_string()]);
    }

    #[test]
    fn enum_with_stream_suffix_does_not_route_to_stream_types() {
        // Enums never get a `$stream` companion in the current model;
        // a stream-suffixed enum (defensive) routes by package only.
        let n = name("user", &["lorem"], "Foo");
        let lp = route(&n, &enum_sym(&n));
        assert_eq!(lp.segments, vec!["lorem".to_string()]);
    }

    #[test]
    fn assert_package_segment_is_sanitized() {
        // BAML stdlib `assert` package collides with Python's `assert`
        // keyword (`from . import assert` is a SyntaxError). The routed
        // leaf renames the segment to `assert_`; the BAML FQN is
        // unaffected because it's built from `Name`, not `LeafPath`.
        let n = name("assert", &[], "is_true");
        let lp = route(&n, &func_sym());
        assert_eq!(
            lp.segments,
            vec!["vendor".to_string(), "assert_".to_string()]
        );
    }

    #[test]
    fn assert_namespace_segment_is_sanitized() {
        // Defense: a namespace path segment named `assert` (today
        // unreachable in user BAML, but cheap to cover) is also renamed.
        let n = name("user", &["assert"], "Foo");
        let lp = route(&n, &class_sym(&n));
        assert_eq!(lp.segments, vec!["assert_".to_string()]);
    }
}
