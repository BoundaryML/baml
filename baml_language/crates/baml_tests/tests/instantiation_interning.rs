//! Identity/interning coverage for generic instantiation expressions
//! (`let f = foo<int>`).
//!
//! The runtime fixtures in `baml_src/ns_instantiation_expr/` assert *value*
//! equality via `==`, which (by design) also holds for distinct
//! but structurally-equal copies — so it can't prove the pooled, pointer-stable
//! identity that `foo<int> === foo<int>` is supposed to guarantee. BAML has no
//! in-language identity operator, so the guarantee is verified here instead, by
//! inspecting the compiled object pool directly: two references to the same
//! concrete specialization must resolve to a *single* pooled
//! `Object::GenericFunction`, and a different specialization must be a different
//! object.

use baml_tests::engine::compile_source;
use bex_vm_types::Object;

/// Collect the pool indices of every `Object::GenericFunction` whose type
/// arguments reach a head with `fq_name`'s tag. An emitted program's heads
/// are tag-only until the loader binds them, so mentions are found by
/// identity rather than by rendered name.
fn generic_function_indices_mentioning(
    program: &bex_vm_types::Program,
    fq_name: &str,
) -> Vec<usize> {
    let needle = baml_type::typetag::TypeTag::of_head(fq_name);
    let mut indices = Vec::new();
    for i in 0..program.objects.len() {
        if let Some(Object::GenericFunction(gf)) = program.objects.get(i) {
            let mut mentions = false;
            for arg in &gf.type_args {
                arg.visit_heads(&mut |head| {
                    if head.tag() == needle {
                        mentions = true;
                    }
                });
            }
            if mentions {
                indices.push(i);
            }
        }
    }
    indices
}

#[test]
fn concrete_instantiation_values_are_interned_and_distinct() {
    // `identity<Marker>` is referenced twice and `identity<Other>` once. User
    // classes are used as type args so nothing in the standard library can
    // produce a colliding `GenericFunction`.
    let program = compile_source(
        r#"
class Marker { id int }
class Other { id int }

function identity<T>(x: T) -> T { x }

// Two references to the SAME specialization. Compared with `==` so the
// bindings stay live through optimization.
function marker_pair() -> bool {
    let a = identity<Marker>;
    let b = identity<Marker>;
    a == b
}

// A DIFFERENT specialization.
function other_one() -> bool {
    let c = identity<Other>;
    c == c
}
"#,
    );

    let marker = generic_function_indices_mentioning(&program, "user.Marker");
    let other = generic_function_indices_mentioning(&program, "user.Other");

    // Interning: two `identity<Marker>` references share ONE pooled object.
    assert_eq!(
        marker.len(),
        1,
        "two `identity<Marker>` references must intern to one pooled \
         Object::GenericFunction (pointer-stable identity), found indices {marker:?}"
    );
    // A different specialization is a different pooled object.
    assert_eq!(
        other.len(),
        1,
        "`identity<Other>` must be one pooled Object::GenericFunction, found indices {other:?}"
    );
    assert_ne!(
        marker[0], other[0],
        "distinct specializations (`identity<Marker>` vs `identity<Other>`) must be \
         distinct pooled objects"
    );
}
