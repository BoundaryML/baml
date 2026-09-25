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

/// The reflection module keeps exactly one name boundary: the test
/// collector's constructor is looked up by its fully qualified name until the
/// executable's rendered views are derived at load.
#[test]
fn the_reflection_module_keeps_one_name_boundary() {
    for needle in [
        "split_once",
        "\"user.",
        "global_by_name",
        "package_name(",
        "dependency_named",
        "link_dynamic",
    ] {
        assert!(!REFLECT.contains(needle), "reflect.rs mentions `{needle}`");
    }
    assert_eq!(
        REFLECT.matches("object_by_name").count(),
        1,
        "reflect.rs resolves by rendered name somewhere new"
    );
}
