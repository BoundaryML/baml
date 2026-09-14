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

use baml_codegen_types::Name;

#[cfg(test)]
use crate::names::PYTHON_KEYWORDS;
use crate::names::is_python_keyword;

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

/// Python's hard keywords. A keyword cannot be used in a dotted reference or
/// relative import, so every routed occurrence receives a trailing `_`.
/// Sanitize a path segment so it's a usable Python module identifier.
///
/// In addition to the hard keywords above, a user namespace named `type`
/// receives a trailing underscore so it cannot shadow the Python builtin in
/// sibling annotations. Runtime BAML FQNs are built from `Name`, not
/// `LeafPath`, so routing does not alter them.
pub(crate) fn sanitize_python_module_segment(seg: &str) -> String {
    let mut projected = String::new();
    for (index, ch) in seg.chars().enumerate() {
        if ch == '_' || ch.is_alphanumeric() && (index > 0 || ch.is_alphabetic()) {
            projected.push(ch);
        } else {
            projected.push('_');
        }
    }
    if projected.is_empty() {
        projected.push('_');
    }
    if projected == "type" || is_python_keyword(&projected) {
        projected.push('_');
    }
    projected
}

/// Route a `Name` referenced from a type position (`Ty::Class`,
/// `Ty::Enum`, `Ty::TypeAlias`). Type references always point at
/// class-like symbols, so the `$stream` suffix routes under
/// `stream_types/`.
pub(crate) fn route_class_ref(name: &Name) -> LeafPath {
    route_inner(name, true)
}

fn route_inner(name: &Name, honor_stream_suffix: bool) -> LeafPath {
    LeafPath {
        segments: raw_route_segments(name, honor_stream_suffix)
            .into_iter()
            .map(|segment| sanitize_python_module_segment(&segment))
            .collect(),
    }
}

pub(crate) fn raw_route_segments(name: &Name, honor_stream_suffix: bool) -> Vec<String> {
    let mut segs: Vec<String> = Vec::new();

    if honor_stream_suffix && name.is_stream() {
        segs.push("stream_types".to_string());
    }

    match name.package().as_str() {
        "user" => {}
        "baml" => segs.push("baml".to_string()),
        "ai" => segs.push("ai".to_string()),
        "reflect" => segs.push("reflect".to_string()),
        other => {
            segs.push("vendor".to_string());
            segs.push(other.to_string());
        }
    }

    for seg in name.namespace() {
        segs.push(seg.as_str().to_string());
    }
    segs
}

#[cfg(test)]
mod tests {
    use baml_base::Name as BaseName;
    use pretty_assertions::assert_eq;

    use super::*;

    fn name(pkg: &str, ns: &[&str], n: &str) -> Name {
        Name::new(
            BaseName::new(pkg),
            ns.iter().map(|s| BaseName::new(*s)).collect(),
            BaseName::new(n),
        )
    }

    #[test]
    fn user_no_ns_routes_to_root_leaf() {
        let n = name("user", &[], "Foo");
        let lp = route_inner(&n, true);
        assert!(lp.is_root());
        assert_eq!(lp.init_py(), PathBuf::from("__init__.py"));
    }

    #[test]
    fn user_with_ns_routes_under_ns() {
        let n = name("user", &["lorem"], "Resume");
        let lp = route_inner(&n, true);
        assert_eq!(lp.segments, vec!["lorem".to_string()]);
        assert_eq!(lp.init_py(), PathBuf::from("lorem/__init__.py"));
    }

    #[test]
    fn vendor_routes_under_vendor_pkg() {
        let n = name("aws", &["s3"], "Bucket");
        let lp = route_inner(&n, true);
        assert_eq!(
            lp.segments,
            vec!["vendor".to_string(), "aws".to_string(), "s3".to_string()]
        );
        assert_eq!(lp.init_py(), PathBuf::from("vendor/aws/s3/__init__.py"));
    }

    #[test]
    fn baml_routes_under_baml() {
        let n = name("baml", &["http"], "Response");
        let lp = route_inner(&n, true);
        assert_eq!(lp.segments, vec!["baml".to_string(), "http".to_string()]);
        assert_eq!(lp.init_py(), PathBuf::from("baml/http/__init__.py"));
    }

    #[test]
    fn ai_routes_under_ai() {
        let n = name("ai", &["stream"], "Stream");
        let lp = route_inner(&n, true);
        assert_eq!(lp.segments, vec!["ai".to_string(), "stream".to_string()]);
        assert_eq!(lp.init_py(), PathBuf::from("ai/stream/__init__.py"));
    }

    #[test]
    fn stream_class_prepends_stream_types() {
        let n = name("user", &["lorem"], "Resume$stream");
        let lp = route_inner(&n, true);
        assert_eq!(
            lp.segments,
            vec!["stream_types".to_string(), "lorem".to_string()]
        );
    }

    #[test]
    fn stream_class_vendor() {
        let n = name("aws", &["s3"], "Bucket$stream");
        let lp = route_inner(&n, true);
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
        let lp = route_inner(&n, true);
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
        let lp = route_inner(&n, true);
        assert_eq!(lp.segments, vec!["stream_types".to_string()]);
        assert_eq!(lp.init_py(), PathBuf::from("stream_types/__init__.py"));
    }

    #[test]
    fn user_deeper_ns() {
        let n = name("user", &["a", "b"], "Thing");
        let lp = route_inner(&n, true);
        assert_eq!(lp.segments, vec!["a".to_string(), "b".to_string()]);
    }

    #[test]
    fn enum_with_stream_suffix_does_not_route_to_stream_types() {
        // Enums never get a `$stream` companion in the current model;
        // a stream-suffixed enum (defensive) routes by package only.
        let n = name("user", &["lorem"], "Foo$stream");
        let lp = route_inner(&n, false);
        assert_eq!(lp.segments, vec!["lorem".to_string()]);
    }

    #[test]
    fn assert_package_segment_is_sanitized() {
        // BAML stdlib `assert` package collides with Python's `assert`
        // keyword (`from . import assert` is a SyntaxError). The routed
        // leaf renames the segment to `assert_`; the BAML FQN is
        // unaffected because it's built from `Name`, not `LeafPath`.
        let n = name("assert", &[], "is_true");
        let lp = route_inner(&n, false);
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
        let lp = route_inner(&n, true);
        assert_eq!(lp.segments, vec!["assert_".to_string()]);
    }

    #[test]
    fn every_python_keyword_segment_is_sanitized_in_packages_and_namespaces() {
        for &keyword in PYTHON_KEYWORDS {
            let expected = format!("{keyword}_");

            let package_name = name(keyword, &[], "Thing");
            let package_leaf = route_inner(&package_name, true);
            assert_eq!(
                package_leaf.segments,
                vec!["vendor".to_string(), expected.clone()],
                "package segment {keyword:?}",
            );

            let namespace_name = name("user", &[keyword], "Thing");
            let namespace_leaf = route_inner(&namespace_name, true);
            assert_eq!(
                namespace_leaf.segments,
                vec![expected],
                "namespace segment {keyword:?}",
            );
        }
    }

    #[test]
    fn type_namespace_segment_is_sanitized() {
        // A user submodule literally named `type` shadows the builtin in
        // sibling annotations (pyright reportInvalidTypeForm). The module
        // segment is mangled while the runtime BAML FQN is unaffected.
        let n = name("user", &["type"], "of_value");
        let lp = route_inner(&n, false);
        assert_eq!(lp.segments, vec!["type_".to_string()]);
    }
}
