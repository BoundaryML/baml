//! Shared routing: turns a `baml_codegen_types::Name` into the leaf `index.ts`
//! directory path (under `baml_sdk/`) where that symbol's TypeScript
//! representation lives. Single source of truth for per-symbol placement;
//! `to_source_code` uses this to enumerate leaves and interior
//! directories, and later phases reuse it when resolving cross-leaf type
//! references.
//!
//! The namespace logic is identical to the Python emitter
//! (`sdkgen_python_pydantic2/src/routing.rs`); only the in-directory filename
//! differs (`__init__.py` → `index.ts`), and that mapping lives in
//! `lib.rs`, not here.
//!
//! The `pkg` field in the codegen-facing `Name` is the literal package
//! name from HIR — `"user"` for project files, `"baml"` for stdlib,
//! `"<vendor>"` for declared external packages.
//!
//! `"baml"` and `"ai"` route under their public package names, anything else
//! under `vendor/<pkg>/`.
//!
//! PPIR-generated `$stream` partial-output types do **not** get a separate
//! leaf. Because `$` is a valid TypeScript identifier character, the partial
//! type keeps its suffix and is emitted beside its final type.

#[cfg(test)]
use std::path::PathBuf;

use baml_codegen_types::Name;

/// Leaf directory path under `baml_sdk/`. Empty segments means the root
/// leaf (i.e. `baml_sdk/index.ts`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct LeafPath {
    pub(crate) segments: Vec<String>,
}

impl LeafPath {
    /// Build the leaf's `index.ts` path, relative to `baml_sdk/`.
    #[cfg(test)]
    pub(crate) fn init_ts(&self) -> PathBuf {
        let mut p = PathBuf::new();
        for seg in &self.segments {
            p.push(seg);
        }
        p.push("index.ts");
        p
    }

    /// Whether this is the root leaf (`baml_sdk/index.ts`).
    #[cfg(test)]
    pub(crate) fn is_root(&self) -> bool {
        self.segments.is_empty()
    }
}

/// Route a pool entry to its leaf `index.ts` directory (under `baml_sdk/`).
///
/// Routing depends only on the symbol's package + namespace path. The
/// `$stream` suffix on a PPIR partial-output type does not influence placement.
pub(crate) fn route(name: &Name) -> LeafPath {
    route_inner(name)
}

/// Sanitize a path segment so it's a usable module/file identifier.
/// JavaScript has many reserved words, but unlike Python's `assert` the
/// segments we emit today (`import * as <seg> from "./<seg>/index.js"`,
/// directory names) tolerate contextual keywords like `assert`, so this
/// is a no-op pass-through for now. The function exists so Phase 4 / 6
/// can extend the sanitization set cheaply if a vendor package or
/// namespace named after a hard JS keyword shows up.
fn sanitize_module_segment(seg: &str) -> String {
    seg.to_string()
}

/// Route a `Name` referenced from a type position (`Ty::Class`,
/// `Ty::Enum`, `Ty::TypeAlias`). Identical to [`route`]: a `$stream` class
/// reference resolves to the base type's leaf (the suffix lives on the
/// emitted identifier, not the path).
pub(crate) fn route_class_ref(name: &Name) -> LeafPath {
    route_inner(name)
}

fn route_inner(name: &Name) -> LeafPath {
    let mut segs: Vec<String> = Vec::new();

    match name.package().as_str() {
        "user" => {}
        "baml" => segs.push("baml".to_string()),
        "ai" => segs.push("ai".to_string()),
        "reflect" => segs.push("reflect".to_string()),
        other => {
            segs.push("vendor".to_string());
            segs.push(sanitize_module_segment(other));
        }
    }

    for seg in name.namespace() {
        segs.push(sanitize_module_segment(seg.as_str()));
    }

    LeafPath { segments: segs }
}

#[cfg(test)]
mod tests {
    use baml_base::Name as BaseName;

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
        let lp = route(&name("user", &[], "Foo"));
        assert!(lp.is_root());
        assert_eq!(lp.init_ts(), PathBuf::from("index.ts"));
    }

    #[test]
    fn user_with_ns_routes_under_ns() {
        let lp = route(&name("user", &["lorem"], "Resume"));
        assert_eq!(lp.segments, vec!["lorem".to_string()]);
        assert_eq!(lp.init_ts(), PathBuf::from("lorem/index.ts"));
    }

    #[test]
    fn vendor_routes_under_vendor_pkg() {
        let lp = route(&name("aws", &["s3"], "Bucket"));
        assert_eq!(
            lp.segments,
            vec!["vendor".to_string(), "aws".to_string(), "s3".to_string()]
        );
        assert_eq!(lp.init_ts(), PathBuf::from("vendor/aws/s3/index.ts"));
    }

    #[test]
    fn baml_routes_under_baml() {
        let lp = route(&name("baml", &["http"], "Response"));
        assert_eq!(lp.segments, vec!["baml".to_string(), "http".to_string()]);
        assert_eq!(lp.init_ts(), PathBuf::from("baml/http/index.ts"));
    }

    #[test]
    fn ai_routes_under_ai() {
        let lp = route(&name("ai", &["stream"], "Stream"));
        assert_eq!(lp.segments, vec!["ai".to_string(), "stream".to_string()]);
        assert_eq!(lp.init_ts(), PathBuf::from("ai/stream/index.ts"));
    }

    #[test]
    fn stream_class_routes_to_base_leaf() {
        // spec2: a `$stream` companion class lives beside its base type;
        // the suffix does not add a `stream_types/` path segment.
        let lp = route(&name("user", &["lorem"], "Resume$stream"));
        assert_eq!(lp.segments, vec!["lorem".to_string()]);
    }

    #[test]
    fn stream_class_vendor_routes_to_base_leaf() {
        let lp = route(&name("aws", &["s3"], "Bucket$stream"));
        assert_eq!(
            lp.segments,
            vec!["vendor".to_string(), "aws".to_string(), "s3".to_string()]
        );
    }

    #[test]
    fn stream_class_baml_routes_to_base_leaf() {
        let lp = route(&name("baml", &["http"], "Response$stream"));
        assert_eq!(lp.segments, vec!["baml".to_string(), "http".to_string()]);
    }

    #[test]
    fn stream_class_user_no_ns_routes_to_root_leaf() {
        let lp = route(&name("user", &[], "Foo$stream"));
        assert!(lp.is_root());
        assert_eq!(lp.init_ts(), PathBuf::from("index.ts"));
    }

    #[test]
    fn user_deeper_ns() {
        let lp = route(&name("user", &["a", "b"], "Thing"));
        assert_eq!(lp.segments, vec!["a".to_string(), "b".to_string()]);
    }

    #[test]
    fn route_class_ref_matches_route_for_stream_suffix() {
        // The type-position router agrees with `route`: a `$stream`
        // reference resolves to the base type's leaf.
        let n = name("user", &["lorem"], "Resume$stream");
        assert_eq!(route_class_ref(&n), route(&n));
        assert_eq!(route_class_ref(&n).segments, vec!["lorem".to_string()]);
    }

    #[test]
    fn assert_package_segment_passes_through() {
        // Unlike Python (where `assert` collides with a keyword and is
        // renamed to `assert_`), `import * as assert from "./assert/index.js"` is
        // legal TypeScript because `assert` is only a contextual keyword.
        // The segment passes through unchanged.
        let lp = route(&name("assert", &[], "is_true"));
        assert_eq!(
            lp.segments,
            vec!["vendor".to_string(), "assert".to_string()]
        );
    }

    #[test]
    fn keyword_does_not_collide() {
        // Pins the no-op identity behavior of `sanitize_module_segment`
        // for a non-keyword segment (the TypeScript analog of Python's
        // `assert_namespace_segment_is_sanitized`, which has no JS
        // equivalent today).
        let lp = route(&name("user", &["helpers"], "Foo"));
        assert_eq!(lp.segments, vec!["helpers".to_string()]);
    }
}
