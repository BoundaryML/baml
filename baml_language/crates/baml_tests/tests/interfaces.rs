//! Host-only interface checks that cannot yet be expressed as native BAML tests.
//!
//! The first pair inspects `BexExternalValue::Instance` field maps, including
//! the absence of interface-view aliases from the concrete runtime shape. The
//! ignored pair records unsupported first-class interface method dispatch;
//! native BAML test blocks do not currently have an `ignore` equivalent.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

#[tokio::test]
async fn aliased_interface_fields_do_not_create_concrete_runtime_slots() {
    let output = baml_test!(
        r#"
        interface Named {
            name: string
        }
        class Person {
            title: string
            implements Named {
                name as title
            }
        }
        function main() -> Person {
            return Person { title: "Ada" }
        }
        "#
    );

    let Ok(BexExternalValue::Instance {
        class_name, fields, ..
    }) = output.result
    else {
        panic!("expected instance, got: {:?}", output.result);
    };
    assert_eq!(class_name, "user.Person");
    assert_eq!(
        fields.get("title"),
        Some(&BexExternalValue::String("Ada".into()))
    );
    assert!(
        fields.get("name").is_none(),
        "interface field `name` must remain a view, not a concrete runtime slot"
    );
}

#[tokio::test]
async fn interface_return_uses_concrete_implementor_field_shape() {
    let output = baml_test!(
        r#"
        interface Named {
            name: string
        }
        class Person {
            title: string
            implements Named {
                name as title
            }
        }
        function main() -> Named {
            return Person { title: "Ada" }
        }
        "#
    );

    let Ok(BexExternalValue::Instance {
        class_name, fields, ..
    }) = output.result
    else {
        panic!("expected instance, got: {:?}", output.result);
    };
    assert_eq!(class_name, "user.Person");
    assert_eq!(
        fields.get("title"),
        Some(&BexExternalValue::String("Ada".into()))
    );
    assert!(
        fields.get("name").is_none(),
        "interface return should serialize the concrete implementor shape"
    );
}

/// Finding #1 [crash]: Interface method reference on required (abstract) method crashes at runtime
#[ignore = "unsupported: taking an interface method as a first-class value \
            (`let f = Interface.method`) and calling it with dynamic dispatch \
            needs a synthesized dispatcher thunk — not implemented"]
#[tokio::test]
async fn fuzz_bug01_method_ref_required_method_crashes() {
    let output = baml_test!(
        r##"interface Animal {
    function speak(self) -> string throws never
}

class Dog {
    implements Animal {
        function speak(self) -> string { return "Woof!" }
    }
}

function main() -> string {
    let speak_fn = Animal.speak
    let d = Dog {}
    return speak_fn(d)
}
"##
    );
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("Woof!".into())
    );
}

/// Finding #2 [wrong-result]: Interface method references (default body) always call the default, never dispatch to overrides
#[ignore = "unsupported: an interface method taken as a first-class value \
            (`let f = Interface.method`) doesn't dispatch polymorphically on its \
            receiver — needs a synthesized dispatcher thunk — not implemented"]
#[tokio::test]
async fn fuzz_bug02_method_ref_default_dispatches_to_override() {
    let output = baml_test!(
        r##"interface Greeter {
    function greet(self) -> string throws never {
        return "Hello from default"
    }
}

class FormalGreeter {
    title: string
    implements Greeter {
        function greet(self) -> string { return "Good day, " + self.title }
    }
}

class CasualGreeter {
    name: string
    implements Greeter {
        function greet(self) -> string { return "Hey " + self.name }
    }
}

function main() -> string {
    let greet_fn = Greeter.greet
    let formal = FormalGreeter { title: "Sir" }
    let casual = CasualGreeter { name: "Bob" }
    return greet_fn(formal) + "|" + greet_fn(casual)
}
"##
    );
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("Good day, Sir|Hey Bob".into())
    );
}
