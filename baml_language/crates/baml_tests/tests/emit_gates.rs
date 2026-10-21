//! The per-package emitter and the static linker reach every declaration by
//! identity. These gates keep the rendered-name lane from growing back: no
//! spelling is split or prefixed, no name-keyed table is consulted, and the
//! legacy string-keyed unit format has no reader.

const EMIT: &[(&str, &str)] = &[
    (
        "lib.rs",
        include_str!("../../baml_compiler2_emit/src/lib.rs"),
    ),
    (
        "refs.rs",
        include_str!("../../baml_compiler2_emit/src/refs.rs"),
    ),
    (
        "emit.rs",
        include_str!("../../baml_compiler2_emit/src/emit.rs"),
    ),
    (
        "items.rs",
        include_str!("../../baml_compiler2_emit/src/items.rs"),
    ),
    (
        "package/mod.rs",
        include_str!("../../baml_compiler2_emit/src/package/mod.rs"),
    ),
    (
        "package/bodies.rs",
        include_str!("../../baml_compiler2_emit/src/package/bodies.rs"),
    ),
    (
        "package/finish.rs",
        include_str!("../../baml_compiler2_emit/src/package/finish.rs"),
    ),
];

const LINKER: &[(&str, &str)] = &[
    (
        "assemble.rs",
        include_str!("../../baml_linker/src/assemble.rs"),
    ),
    ("bind.rs", include_str!("../../baml_linker/src/bind.rs")),
    ("layout.rs", include_str!("../../baml_linker/src/layout.rs")),
    ("order.rs", include_str!("../../baml_linker/src/order.rs")),
    (
        "resolve.rs",
        include_str!("../../baml_linker/src/resolve.rs"),
    ),
    ("space.rs", include_str!("../../baml_linker/src/space.rs")),
    ("lib.rs", include_str!("../../baml_linker/src/lib.rs")),
];

const NEEDLES: &[&str] = &[
    "split_once",
    "\"user.",
    "class_object_indices",
    "seed_served_rows",
    "from_stdlib_program",
    "legacy_unit",
    "legacy_link",
    "SymbolKind",
    "obj_by_name",
];

/// A source file's production text: everything before its first
/// `#[cfg(test)]` — a test module is not a production path.
fn production(source: &str) -> &str {
    source
        .find("#[cfg(test)]")
        .map_or(source, |at| &source[..at])
}

#[test]
fn the_emitter_resolves_nothing_by_rendered_name() {
    for (file, source) in EMIT {
        for needle in NEEDLES {
            assert!(
                !production(source).contains(needle),
                "emit {file} mentions `{needle}`"
            );
        }
    }
}

#[test]
fn the_linker_resolves_nothing_by_rendered_name() {
    for (file, source) in LINKER {
        for needle in NEEDLES {
            assert!(
                !production(source).contains(needle),
                "linker {file} mentions `{needle}`"
            );
        }
    }
}
