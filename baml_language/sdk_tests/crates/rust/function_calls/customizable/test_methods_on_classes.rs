//! Static + instance method coverage (ns_methods_on_classes.Greeter).
//!
//! `raises_test.DocLoader` pins the *shape* of method bindings, but its
//! bodies always throw, so they are never invoked. `Greeter` has non-throwing
//! bodies, so this exercises the full host->engine round-trip for both
//! flavors:
//!
//!   - static   -> `Greeter::create(name)`      (associated function, no `self`)
//!   - instance -> `g.who()` / `g.greet(arg)`   (`&self` receiver)
//!
//! each with its `_async` sibling; the async cases run under
//! `#[tokio::test]`.
//!
//! Node counterpart: `crates/typescript_node/function_calls/customizable/methods_on_classes.test.ts`.

use baml_sdk::methods_on_classes::Greeter;

#[test]
fn test_methods_on_classes_method_bindings_exist() {
    // Static bindings hang off the class; instance bindings carry `self`.
    // python asserts `callable(...)`; in Rust, naming each binding as a
    // function item is the (compile-time) assertion.
    let _ = (
        Greeter::create,
        Greeter::create_async,
        Greeter::who,
        Greeter::who_async,
        Greeter::greet,
        Greeter::greet_async,
    );
}

#[test]
fn test_methods_on_classes_static_create_round_trips() {
    // `isinstance(g, Greeter)` is the ascribed type.
    let g: Greeter = Greeter::create("ada".to_string()).unwrap();
    assert_eq!(g.name, "ada");
}

#[tokio::test]
async fn test_methods_on_classes_static_create_async_round_trips() {
    let g: Greeter = Greeter::create_async("grace".to_string()).await.unwrap();
    assert_eq!(g.name, "grace");
}

#[test]
fn test_methods_on_classes_instance_who_round_trips() {
    let g = Greeter::create("hopper".to_string()).unwrap();
    assert_eq!(g.who().unwrap(), "hopper");
}

#[tokio::test]
async fn test_methods_on_classes_instance_who_async_round_trips() {
    let g = Greeter::create_async("hopper".to_string()).await.unwrap();
    assert_eq!(g.who_async().await.unwrap(), "hopper");
}

#[test]
fn test_methods_on_classes_instance_greet_with_arg_round_trips() {
    let g = Greeter::create("lovelace".to_string()).unwrap();
    assert_eq!(g.greet("hi".to_string()).unwrap(), "hi");
}

#[tokio::test]
async fn test_methods_on_classes_instance_greet_async_with_arg_round_trips() {
    let g = Greeter::create_async("lovelace".to_string()).await.unwrap();
    assert_eq!(g.greet_async("hi".to_string()).await.unwrap(), "hi");
}
