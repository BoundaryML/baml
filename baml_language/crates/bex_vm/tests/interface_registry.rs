//! White-box checks of the baked interface registry (`Program::packages`).
//!
//! These assert properties of the data the runtime resolver consumes for a case
//! the resolver's only live caller (reflection) can't observe: that an impl
//! rule's method table is *complete* — it carries the interface's inherited
//! default methods, not just the methods the impl overrides, with an override
//! winning over the default.

use baml_db::testing::compile_source;
use bex_vm_types::{Object, TyTemplate, types::Program};

/// The head type name of a for-type pattern (`Dog` for `Dog`, `Wrap` for
/// `Wrap<T>`). Matching on this — rather than a substring of the rendered
/// pattern — keeps distinct names like `Dog` and `HotDog` (and `$stream`
/// companions, which have a distinct head name) from colliding.
///
/// A head in a not-yet-loaded `Program` is a tag with no pointer, so the name
/// comes from the pooled declaration carrying that tag — the same association
/// the loader's bind pass makes.
fn for_ty_head_name<'a>(program: &'a Program, pat: &TyTemplate) -> Option<&'a str> {
    let (TyTemplate::Class(head, ..) | TyTemplate::Enum(head, ..)) = pat else {
        return None;
    };
    program.objects.iter().find_map(|object| match object {
        Object::Class(class) if class.type_tag == head.tag() => {
            Some(class.name.item_name().as_str())
        }
        Object::Enum(enm) if enm.type_tag == head.tag() => Some(enm.name.item_name().as_str()),
        _ => None,
    })
}

/// The `(method name, fn FQN)` pairs recorded for `<for_type> implements <iface>`.
/// `iface` is matched against the interface's (short) name and `for_type` against
/// the for-type pattern's head name — both exact, so overlapping names don't
/// alias. The interface and method callees are carried as global object indices
/// (`Program::packages` is `HeapPtr`-free), so they are resolved here through
/// `Program::objects`. Panics if no such interface / rule was baked.
fn impl_methods(program: &Program, iface: &str, for_type: &str) -> Vec<(String, String)> {
    let rule = program
        .packages
        .values()
        .flat_map(|pkg| pkg.impl_rules.values().flatten())
        .filter(|rule| {
            program.objects[rule.interface_head]
                .as_interface()
                .is_some_and(|def| def.name.name().as_str() == iface)
        })
        .find(|rule| for_ty_head_name(program, &rule.for_ty_pattern) == Some(for_type))
        .unwrap_or_else(|| panic!("no `{for_type} implements {iface}` rule baked"));
    rule.methods
        .iter()
        .map(|(name, method)| {
            let fqn = match &program.objects[method.fqn] {
                Object::Function(f) => f.name.clone(),
                other => panic!("method callee is not a Function: {other:?}"),
            };
            (name.as_str().to_string(), fqn)
        })
        .collect()
}

#[test]
fn impl_rule_methods_include_inherited_interface_defaults() {
    // `Greeter` has a required method (`greet`) and a default method
    // (`greet_loud`). `Dog` overrides only `greet`, inheriting the default — both
    // must appear in the baked method table so the resolver can dispatch either.
    let program = compile_source(
        r#"
        interface Greeter {
            function greet(self) -> string throws never
            function greet_loud(self) -> string throws never { return self.greet() }
        }
        class Dog {
            implements Greeter {
                function greet(self) -> string { return "woof" }
            }
        }
        "#,
    );

    let methods = impl_methods(&program, "Greeter", "Dog");
    assert!(
        methods.iter().any(|(m, _)| m == "greet"),
        "overridden method missing: {methods:?}"
    );
    assert!(
        methods.iter().any(|(m, _)| m == "greet_loud"),
        "inherited default method missing: {methods:?}"
    );
}

#[test]
fn impl_rule_override_wins_over_inherited_default() {
    // `Cat` overrides BOTH methods, including the defaulted one. The recorded
    // `greet_loud` must be Cat's override (its FQN names `Cat`), not the
    // interface default — the merge must not clobber an override.
    let program = compile_source(
        r#"
        interface Greeter {
            function greet(self) -> string throws never
            function greet_loud(self) -> string throws never { return self.greet() }
        }
        class Cat {
            implements Greeter {
                function greet(self) -> string { return "meow" }
                function greet_loud(self) -> string { return "MEOW" }
            }
        }
        "#,
    );

    let methods = impl_methods(&program, "Greeter", "Cat");
    let (_, greet_loud_fqn) = methods
        .iter()
        .find(|(m, _)| m == "greet_loud")
        .expect("greet_loud recorded");
    assert!(
        greet_loud_fqn.contains("Cat"),
        "override should win: greet_loud FQN should be Cat's, got {greet_loud_fqn:?}"
    );
}
