//! The runtime loader binds by identity. These gates keep every rendered-name
//! surface out of it: a dependency slot is a pinned package object or a
//! prelude package, a path resolves through the bound package's own tables,
//! and nothing is spelled, split, or prefixed on the way.

const GRAFT: &str = include_str!("../../bex_vm/src/package_reflect/graft.rs");
const REFLECT: &str = include_str!("../../bex_vm/src/package_reflect/reflect.rs");

#[test]
fn the_loader_never_resolves_by_rendered_name() {
    for needle in [
        "split_once",
        "\"user.",
        "object_by_name",
        "global_by_name",
        "package_name(",
        "dependency_named",
        "link_dynamic",
        "object_names",
        "global_names",
        "of_head(",
        "of_name(",
    ] {
        assert!(!GRAFT.contains(needle), "graft.rs mentions `{needle}`");
    }
}

/// The reflection module resolves nothing by rendered name either: the test
/// collector's constructor, its last name boundary, now comes through the
/// prelude package's tables.
#[test]
fn the_reflection_module_resolves_nothing_by_rendered_name() {
    for needle in [
        "split_once",
        "\"user.",
        "object_by_name",
        "global_by_name",
        "package_name(",
        "dependency_named",
        "link_dynamic",
        "object_names",
        "global_names",
    ] {
        assert!(!REFLECT.contains(needle), "reflect.rs mentions `{needle}`");
    }
}
