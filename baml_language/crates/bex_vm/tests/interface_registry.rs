//! White-box checks of the baked interface registry (`Program::interface_impls`).
//!
//! These assert properties of the data the runtime resolver consumes for a case
//! the resolver's only live caller (reflection) can't observe: that an impl
//! rule's method table is *complete* — it carries the interface's inherited
//! default methods, not just the methods the impl overrides, with an override
//! winning over the default.

use baml_project::testing::compile_source;
use bex_vm_types::types::Program;

/// The `(method name, fn FQN)` pairs recorded for `<for_type> implements <iface>`.
/// `for_type` is matched against the rule's `for_ty_pattern` rendering, excluding
/// `$stream` companions. Panics if no such interface / rule was baked.
fn impl_methods(program: &Program, iface: &str, for_type: &str) -> Vec<(String, String)> {
    program
        .interface_impls
        .values()
        .find_map(|pkg| {
            pkg.iter()
                .find(|(tn, _)| tn.name().as_str() == iface)
                .map(|(_, rules)| rules)
        })
        .unwrap_or_else(|| panic!("no impls baked for interface {iface:?}"))
        .iter()
        .find(|rule| {
            let pat = rule.for_ty_pattern.to_string();
            pat == for_type || (pat.contains(for_type) && !pat.contains("$stream"))
        })
        .unwrap_or_else(|| panic!("no `{for_type} implements {iface}` rule baked"))
        .methods
        .iter()
        .map(|(name, method)| (name.as_str().to_string(), method.fqn.clone()))
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
            function greet(self) -> string
            function greet_loud(self) -> string { return self.greet() }
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
            function greet(self) -> string
            function greet_loud(self) -> string { return self.greet() }
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
