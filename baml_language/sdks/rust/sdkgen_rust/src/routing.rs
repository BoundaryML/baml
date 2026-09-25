//! Routing: turns a `baml_codegen_types::Name` into the module path
//! (under `src/`) where that symbol's Rust representation lives.
//!
//! The package of the codegen-facing `Name` is `Local` for project files
//! (`is_local()`), `"baml"` for stdlib, `"<vendor>"` for declared external
//! packages. Local symbols route to the crate root's namespace tree,
//! core language packages under their own names, other dependencies under `vendor/<pkg>/` — the same
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
    LeafPath {
        segments: baml_codegen_types::namespace_segments(name)
            .iter()
            .map(|segment| idents::dir_segment(segment))
            .collect(),
    }
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
    fn ai_and_reflect_use_builtin_roots_with_target_escaping() {
        for package in ["ai", "reflect"] {
            assert_eq!(
                route(&name(package, &["crate"], "Thing")).segments,
                [package, "crate_"]
            );
        }
    }

    #[test]
    fn vendor_routes_under_vendor_pkg() {
        assert_eq!(
            route(&name("aws", &["s3"], "Bucket")).segments,
            ["vendor", "aws", "s3"]
        );
    }
}
