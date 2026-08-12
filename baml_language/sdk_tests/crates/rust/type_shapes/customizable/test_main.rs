//! Smoke tests for the type_shapes sdk-test crate.
//!
//! The actual type-shape verification is the Rust compiler type-checking
//! the generated crate (python leans on `pyright baml_sdk`, run by the
//! `pyright` test in `tests/sdk_test.rs`, for the same job). These cases
//! just confirm each generated namespace imports cleanly and that the
//! symbols listed in 18a are reachable.
//!
//! ADAPTATION(rust): python's smoke imports are runtime facts; here
//! reachability is a compile-time fact, asserted by the `use … as _;`
//! items naming each path. Those imports are "unused" by design — the
//! assertion is that they resolve.

#![expect(
    unused_imports,
    reason = "reachability is asserted by the imports resolving at compile time"
)]

#[test]
fn test_main_root_imports_cleanly() {
    use baml_sdk as _;
}

#[test]
fn test_main_all_namespaces_reachable() {
    use baml_sdk::a as _;
    use baml_sdk::aliases as _;
    use baml_sdk::aliases_consumer as _;
    use baml_sdk::class_refs as _;
    use baml_sdk::complex_models as _;
    use baml_sdk::enums as _;
    use baml_sdk::forward_refs as _;
    use baml_sdk::generics as _;
    use baml_sdk::lists as _;
    use baml_sdk::literals as _;
    use baml_sdk::lorem as _;
    use baml_sdk::maps as _;
    use baml_sdk::media as _;
    use baml_sdk::optional as _;
    use baml_sdk::primitives as _;
    use baml_sdk::recursion as _;
    use baml_sdk::unions as _;
}

#[test]
fn test_main_root_foo_reachable() {
    use baml_sdk::Foo as _;
}

#[test]
fn test_main_lorem_resume_reachable() {
    use baml_sdk::lorem::Resume as _;
}

#[test]
fn test_main_deep_namespace_thing_reachable() {
    use baml_sdk::a::b::Thing as _;
}
