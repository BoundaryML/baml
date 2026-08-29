//! Routing: turns a `baml_codegen_types::Name` into the module path
//! (under `src/`) where that symbol's Rust representation lives.
//!
//! The package of the codegen-facing `Name` is `Local` for project files
//! (`is_local()`), `"baml"` for stdlib, `"<vendor>"` for declared external
//! packages. Local symbols route to the crate root's namespace tree,
//! `"baml"` under `baml/`, anything else under `vendor/<pkg>/` — the same
//! placement rules as the python and typescript emitters.

use baml_codegen_types::Name;

use crate::idents;

/// Module path under `src/`, one segment per directory. Empty segments
/// means the crate root (`src/lib.rs`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct LeafPath {
    /// On-disk directory segments (see [`idents::dir_segment`]).
    pub(crate) segments: Vec<String>,
}

impl LeafPath {
    #[cfg(test)]
    pub(crate) fn is_root(&self) -> bool {
        self.segments.is_empty()
    }
}

/// Route a pool entry to the module its Rust items are emitted in.
pub(crate) fn route(name: &Name) -> LeafPath {
    let mut segments = Vec::new();
    if name.is_stream() {
        segments.push("stream_types".to_string());
    }
    if !name.is_local() {
        match name.package().as_str() {
            "baml" => segments.push("baml".to_string()),
            vendor => {
                segments.push("vendor".to_string());
                segments.push(idents::dir_segment(vendor));
            }
        }
    }
    for seg in name.namespace() {
        segments.push(idents::dir_segment(seg.as_str()));
    }
    LeafPath { segments }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn name(pkg: &str, ns: &[&str], leaf: &str) -> Name {
        Name::new(
            baml_base::Name::new(pkg),
            ns.iter().map(|s| baml_base::Name::new(*s)).collect(),
            baml_base::Name::new(leaf),
        )
    }

    #[test]
    fn user_symbols_route_to_the_namespace_tree() {
        assert!(route(&name("user", &[], "hello_world")).is_root());
        assert_eq!(route(&name("user", &["a", "b"], "X")).segments, ["a", "b"]);
    }

    #[test]
    fn stdlib_routes_under_baml() {
        assert_eq!(
            route(&name("baml", &["http"], "Response")).segments,
            ["baml", "http"]
        );
    }

    #[test]
    fn vendor_routes_under_vendor_pkg() {
        assert_eq!(
            route(&name("aws", &["s3"], "Bucket")).segments,
            ["vendor", "aws", "s3"]
        );
    }

    #[test]
    fn stream_types_route_under_the_stream_types_tree() {
        assert_eq!(
            route(&name("user", &["lorem"], "Doc$stream")).segments,
            ["stream_types", "lorem"]
        );
    }
}
