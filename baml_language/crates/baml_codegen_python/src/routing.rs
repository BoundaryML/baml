//! Routing: turns a `baml_codegen_types::Name` into the leaf `__init__.py`
//! path (under `baml_sdk/`) where that symbol's Python representation
//! lives. Single source of truth for per-symbol placement; G1 uses this
//! to enumerate leaves and interior directories, and later phases reuse
//! it when resolving cross-leaf type references.
//!
//! Rule source: 09b-codegen-rules §1 + 11c-phaseg1 §3.
//!
//! The `pkg` field in the codegen-facing `Name` is the external BAML
//! package name. Today the compiler frontend populates user-code symbols
//! with `pkg == "user"`, while the external/documentation name for the
//! same thing is `"root"` (matches `__define_function("root.lorem.foo", …)`
//! strings rendered into generated code). Routing accepts both.
//!
//! `"baml"` routes under `baml/`, anything else under `vendor/<pkg>/`.
//!
//! If the name carries the `$stream` suffix, the entire routed path is
//! prepended with `stream_types/` before top-level packaging is applied.

#[cfg(test)]
use std::path::PathBuf;

use baml_codegen_types::Name;

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

/// Route a `Name` to its leaf `__init__.py` path (under `baml_sdk/`).
pub(crate) fn route(name: &Name) -> LeafPath {
    let mut segs: Vec<String> = Vec::new();

    if name.is_stream() {
        segs.push("stream_types".to_string());
    }

    match name.pkg.as_str() {
        // `user` is the internal compiler name; `root` is the external
        // documentation name. Both land at the SDK root (no prefix).
        "user" | "root" => {}
        "baml" => segs.push("baml".to_string()),
        other => {
            segs.push("vendor".to_string());
            segs.push(other.to_string());
        }
    }

    for seg in &name.namespace_path {
        segs.push(seg.as_str().to_string());
    }

    LeafPath { segments: segs }
}

#[cfg(test)]
mod tests {
    use super::*;
    use baml_base::Name as BaseName;

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
        let lp = route(&n);
        assert!(lp.is_root());
        assert_eq!(lp.init_py(), PathBuf::from("__init__.py"));
    }

    #[test]
    fn user_with_ns_routes_under_ns() {
        let n = name("user", &["lorem"], "Resume");
        let lp = route(&n);
        assert_eq!(lp.segments, vec!["lorem".to_string()]);
        assert_eq!(lp.init_py(), PathBuf::from("lorem/__init__.py"));
    }

    #[test]
    fn root_alias_matches_user() {
        let u = route(&name("user", &["lorem"], "Resume"));
        let r = route(&name("root", &["lorem"], "Resume"));
        assert_eq!(u, r);
    }

    #[test]
    fn vendor_routes_under_vendor_pkg() {
        let n = name("aws", &["s3"], "Bucket");
        let lp = route(&n);
        assert_eq!(
            lp.segments,
            vec!["vendor".to_string(), "aws".to_string(), "s3".to_string()]
        );
        assert_eq!(lp.init_py(), PathBuf::from("vendor/aws/s3/__init__.py"));
    }

    #[test]
    fn baml_routes_under_baml() {
        let n = name("baml", &["http"], "Response");
        let lp = route(&n);
        assert_eq!(lp.segments, vec!["baml".to_string(), "http".to_string()]);
        assert_eq!(lp.init_py(), PathBuf::from("baml/http/__init__.py"));
    }

    #[test]
    fn stream_prepends_stream_types() {
        let n = name("user", &["lorem"], "Resume$stream");
        let lp = route(&n);
        assert_eq!(
            lp.segments,
            vec!["stream_types".to_string(), "lorem".to_string()]
        );
    }

    #[test]
    fn stream_vendor() {
        let n = name("aws", &["s3"], "Bucket$stream");
        let lp = route(&n);
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
    fn stream_baml() {
        let n = name("baml", &["http"], "Response$stream");
        let lp = route(&n);
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
    fn stream_user_no_ns_routes_to_stream_root_leaf() {
        let n = name("user", &[], "Foo$stream");
        let lp = route(&n);
        assert_eq!(lp.segments, vec!["stream_types".to_string()]);
        assert_eq!(lp.init_py(), PathBuf::from("stream_types/__init__.py"));
    }

    #[test]
    fn user_deeper_ns() {
        let n = name("user", &["a", "b"], "Thing");
        let lp = route(&n);
        assert_eq!(lp.segments, vec!["a".to_string(), "b".to_string()]);
    }
}
