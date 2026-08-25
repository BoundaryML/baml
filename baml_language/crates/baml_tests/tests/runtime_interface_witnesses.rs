//! Executable oracles for structured runtime interface witnesses,
//! bounded `unreflect`, open-schema rendering failures, and dynamic-rule GC.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

#[tokio::test]
async fn witnessed_runtime_class_implements_static_interface() {
    let output = baml_test!(
        r#"
        interface PersonAnchor {
            name: string
            email: string
        }

        function main() -> bool {
            let witness = reflect.interface.implementation<PersonAnchor>()
                .field("name")
                .field("email")
            let person_t = reflect.class.new("Person", {
                "name": reflect.Type.of<string>(),
                "email": reflect.Type.of<string>(),
                "favorite_editor": reflect.Type.of<string>(),
            }, implementations = [witness])
            return person_t.as_type().implements(reflect.Type.of<PersonAnchor>())
        }
        "#
    );

    assert_eq!(
        output.result.expect("witness should construct"),
        BexExternalValue::Bool(true)
    );
}

#[tokio::test]
async fn witness_realizes_associated_field_binding_before_exact_type_check() {
    let output = baml_test!(
        r#"
        interface PublicIdentity {
            type Key
            key: Self.Key
        }

        function main() -> bool {
            let witness = reflect.interface.implementation<PublicIdentity<Key = string>>()
                .field("key", class_field = "public_key")
            let account_t = reflect.class.new("Account", {
                "public_key": reflect.Type.of<string>(),
            }, implementations = [witness])
            return account_t.as_type().implements(reflect.Type.of<PublicIdentity<Key = string>>())
        }
        "#
    );

    assert_eq!(
        output.result.expect("associated binding should realize"),
        BexExternalValue::Bool(true)
    );
}

#[tokio::test]
async fn scenario_four_pattern_one_uses_typed_anchor_and_runtime_leaves() {
    let output = baml_test!(
        r##"
        interface PersonAnchor {
            name: string
            email: string
        }

        client TestClient = openai.ResponsesClient.new(
    model = "gpt-4o-mini",
    api_key = "test-key",
    base_url = "http://localhost:1234",
);

        function ExtractPerson<T extends PersonAnchor>(input: string) -> T {
            client: TestClient
            prompt: `Extract a person from ${input}.\n${ctx.output_format()}`
        }

        function main() -> string {
            let anchor_impl = reflect.interface.implementation<PersonAnchor>()
                .field("name")
                .field("email", class_field = "contact_email")
            let person_t = reflect.class.new("Person", {
                "name": reflect.Type.of<string>(),
                "contact_email": reflect.Type.of<string>(),
                "favorite_editor": reflect.Type.of<string>(),
            }, implementations = [anchor_impl])

            let prompt = ExtractPerson$render_prompt<unreflect(person_t.as_type())>("sample").text()
            let person: PersonAnchor = ExtractPerson$parse<unreflect(person_t.as_type())>(
                `{"name":"Ada","contact_email":"ada@example.com","favorite_editor":"vim"}`
            )
            let runtime_leaf = reflect.class.get_field<string>(person, "favorite_editor")
            return prompt
                + "\n<RESULT>"
                + person.name + "|" + person.email + "|" + runtime_leaf
        }
        "##
    );

    let BexExternalValue::String(result) = output
        .result
        .expect("witnessed bounded extraction should succeed")
    else {
        panic!("expected string result")
    };
    assert!(
        result.contains("favorite_editor"),
        "runtime leaf missing: {result}"
    );
    assert!(
        result.ends_with("<RESULT>Ada|ada@example.com|vim"),
        "typed virtual fields or runtime leaf failed: {result}"
    );
}

#[tokio::test]
async fn bounded_unreflect_fails_before_rendering() {
    let output = baml_test!(
        r##"
        interface PersonAnchor {
            name: string
            email: string
        }

        client TestClient = openai.ResponsesClient.new(
    model = "gpt-4o-mini",
    api_key = "test-key",
    base_url = "http://localhost:1234",
);

        function ExtractPerson<T extends PersonAnchor>() -> T {
            client: TestClient
            prompt: `${ctx.output_format()}`
        }

        function main() -> string {
            // If rendering ran first this empty enum would produce E0159.
            let not_a_person = reflect.enum.new("NoPerson", [])
            let result = ExtractPerson$render_prompt<unreflect(not_a_person)>() catch (e) {
                reflect.errors.CompilationError => {
                    e.diagnostics[0].code + "|" + e.diagnostics[0].message
                }
            }
            if result is string {
                return result
            }
            return "bound did not throw"
        }
        "##
    );

    let BexExternalValue::String(result) =
        output.result.expect("bound failure should be catchable")
    else {
        panic!("expected string result")
    };
    assert_eq!(
        result.as_str(),
        "E0001|mismatched types",
        "render ran before the static-equivalent bound diagnostic: {result}"
    );
}

#[tokio::test]
async fn unreflect_argument_is_revalidated_against_the_runtime_type() {
    let output = baml_test!(
        r#"
        interface PersonAnchor {
            name: string
            email: string
        }

        function Echo<T extends PersonAnchor>(value: T) -> T {
            value
        }

        function main() -> string {
            let witness = reflect.interface.implementation<PersonAnchor>()
                .field("name")
                .field("email")
            let person_t = reflect.class.new("Person", {
                "name": reflect.Type.of<string>(),
                "email": reflect.Type.of<string>(),
            }, implementations = [witness])
            let result = Echo<unreflect(person_t.as_type())>(42) catch (e) {
                reflect.errors.CompilationError => {
                    e.diagnostics[0].code + "|" + e.diagnostics[0].message
                }
            }
            if result is string {
                return result
            }
            return "argument check did not throw"
        }
        "#
    );

    let BexExternalValue::String(result) = output
        .result
        .expect("runtime argument mismatch should be catchable")
    else {
        panic!("expected string result")
    };
    assert_eq!(result.as_str(), "E0001|mismatched types");
}

#[tokio::test]
async fn incomplete_witness_fails_before_type_and_chain_is_immutable() {
    let output = baml_test!(
        r#"
        interface PersonAnchor {
            name: string
            email: string
        }

        function main() -> string {
            let base = reflect.interface.implementation<PersonAnchor>().field("name")
            let complete = base.field("email", class_field = "contact_email")
            let bad = reflect.class.new("Person", {
                "name": reflect.Type.of<string>(),
                "contact_email": reflect.Type.of<string>(),
            }, implementations = [base]) catch (e) {
                reflect.errors.CompilationError => {
                    e.diagnostics[0].code + "|" + e.diagnostics[0].message
                }
            }
            let good = reflect.class.new("Person", {
                "name": reflect.Type.of<string>(),
                "contact_email": reflect.Type.of<string>(),
            }, implementations = [complete])
            let failure = "missing witness did not throw"
            if bad is string {
                failure = bad
            }
            return failure + "|" + good.as_type().implements(reflect.Type.of<PersonAnchor>()).to_string()
        }
        "#
    );

    let BexExternalValue::String(result) = output
        .result
        .expect("incomplete witness failure and retry should be catchable")
    else {
        panic!("expected string result")
    };
    assert!(result.starts_with("E0001|"), "wrong diagnostic: {result}");
    assert!(
        result.contains("missing required field `email`"),
        "wrong message: {result}"
    );
    assert!(
        result.ends_with("|true"),
        "failed construction leaked a type: {result}"
    );
}

#[tokio::test]
async fn equivalent_witnessed_definitions_render_and_parse_identically() {
    let output = baml_test!(
        r##"
        interface PersonAnchor {
            name: string
            email: string
        }

        client TestClient = openai.ResponsesClient.new(
    model = "gpt-4o-mini",
    api_key = "test-key",
    base_url = "http://localhost:1234",
);

        function ExtractPerson<T extends PersonAnchor>() -> T {
            client: TestClient
            prompt: `${ctx.output_format()}`
        }

        function main() -> bool {
            let witness = reflect.interface.implementation<PersonAnchor>()
                .field("name")
                .field("email")
            let left = reflect.class.new("Person", {
                "name": reflect.Type.of<string>(),
                "email": reflect.Type.of<string>(),
            }, implementations = [witness])
            let right = reflect.class.new("Person", {
                "name": reflect.Type.of<string>(),
                "email": reflect.Type.of<string>(),
            }, implementations = [witness])
            let left_prompt = ExtractPerson$render_prompt<unreflect(left.as_type())>().text()
            let right_prompt = ExtractPerson$render_prompt<unreflect(right.as_type())>().text()
            let l: PersonAnchor = ExtractPerson$parse<unreflect(left.as_type())>(
                `{"name":"Ada","email":"ada@example.com"}`
            )
            let r: PersonAnchor = ExtractPerson$parse<unreflect(right.as_type())>(
                `{"name":"Ada","email":"ada@example.com"}`
            )
            return left != right
                && left_prompt == right_prompt
                && l.name == r.name
                && l.email == r.email
                && left.as_type().implements(reflect.Type.of<PersonAnchor>())
                && right.as_type().implements(reflect.Type.of<PersonAnchor>())
        }
        "##
    );

    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn open_interface_occurrence_fails_at_render_boundary() {
    let output = baml_test!(
        r##"
        interface PersonAnchor {
            name: string
            email: string
        }

        class Envelope {
            person PersonAnchor
        }

        client TestClient = openai.ResponsesClient.new(
    model = "gpt-4o-mini",
    api_key = "test-key",
    base_url = "http://localhost:1234",
);

        function ExtractEnvelope() -> Envelope {
            client: TestClient
            prompt: `${ctx.output_format()}`
        }

        function main() -> string {
            let rendered = ExtractEnvelope$render_prompt() catch (e) {
                reflect.errors.CompilationError => {
                    e.diagnostics[0].code + "|" + e.diagnostics[0].message
                }
            }
            if rendered is string {
                return rendered
            }
            return "render did not throw"
        }
        "##
    );

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String(
            "E0161|field `Envelope.person` has open interface type `PersonAnchor`, which cannot be rendered as an LLM output schema".into()
        ))
    );
}

/// A method-bearing interface whose methods all carry default bodies is
/// structurally witnessable: the witness supplies the fields, and each method
/// resolves to its default. Before emit named the defaults, every method-
/// bearing interface was rejected as unwitnessable.
#[tokio::test]
async fn witness_inherits_interface_default_methods() {
    let output = baml_test!(
        r##"
        interface Greeter {
            name: string
            function greet(self) -> string {
                "hello, " + self.name
            }
        }

        client TestClient = openai.ResponsesClient.new(
    model = "gpt-4o-mini",
    api_key = "test-key",
    base_url = "http://localhost:1234",
);

        function ExtractGreeter<T extends Greeter>(input: string) -> T {
            client: TestClient
            prompt: `Extract from ${input}.\n${ctx.output_format()}`
        }

        function main() -> string {
            let witness = reflect.interface.implementation<Greeter>().field("name")
            let person_t = reflect.class.new("Person", {
                "name": reflect.Type.of<string>(),
            }, implementations = [witness])
            let is_member = person_t.as_type().implements(reflect.Type.of<Greeter>())
            let person: Greeter = ExtractGreeter$parse<unreflect(person_t.as_type())>(
                `{"name":"Ada"}`
            )
            // Virtual dispatch on the witnessed value reaches the interface's
            // default body, whose inner `self.name` reads the linked field.
            let prefix = "no|"
            if is_member {
                prefix = "yes|"
            }
            return prefix + person.greet()
        }
        "##
    );

    let BexExternalValue::String(result) = output
        .result
        .expect("all-default-method interface should be witnessable")
    else {
        panic!("expected string result")
    };
    assert_eq!(result.as_str(), "yes|hello, Ada");
}

/// A required method (no default body) still makes the interface structurally
/// unwitnessable — a witness has no way to supply a body.
#[tokio::test]
async fn witness_still_rejects_interfaces_with_required_methods() {
    let output = baml_test!(
        r#"
        interface Named {
            name: string
            function describe(self) -> string
        }

        function main() -> string {
            let result = reflect.interface.implementation<Named>().field("name") catch (e) {
                reflect.errors.CompilationError => e.diagnostics[0].message
            }
            if result is string {
                return result
            }
            return "did not throw"
        }
        "#
    );

    let BexExternalValue::String(result) = output
        .result
        .expect("required-method rejection should be catchable")
    else {
        panic!("expected string result")
    };
    assert!(
        result.contains("cannot be witnessed structurally") && result.contains("`describe`"),
        "unexpected message: {result}"
    );
}

/// A runtime-compiled interface (declared inside a Session) with a default
/// method: its default body lives in the session package's own object pool,
/// so the pointer must be bound through the runtime graft — not the static
/// image — and the session's witness must still inherit it.
#[tokio::test]
async fn session_interface_default_methods_are_bound_and_inherited() {
    let output = baml_test!(
        r#####"
        function main() -> string throws unknown {
            let s = reflect.Session.new()
            s.eval(#"interface Greeter { who: string  function greet(self) -> string { "hello" } }"#)
            // BUG (session hygiene, pre-existing): a top-level session `let`
            // whose initializer combines an inline map literal with a keyword
            // argument (`reflect.class.new("P", { "f": t }, implementations = [w])`)
            // fails to parse after identifier rewriting. Wrapping the same
            // expression in a session-declared function sidesteps it.
            s.eval(#"
                function build() -> reflect.Type {
                    let witness = reflect.interface.implementation<Greeter>().field("who")
                    let person_t = reflect.class.new("Person", { "who": reflect.Type.of<string>() }, implementations = [witness])
                    person_t.as_type()
                }
            "#)
            // BUG (session parse, pre-existing): `x.implements(...)` inside a
            // Session eval fails to parse — `implements` lexes as the keyword and
            // the session's parse wrapper does not accept it as a method name
            // after `.`, unlike the main compiler. `implemented_by` is the same
            // relation with the operands flipped, so it stands in here.
            let is_member = s.eval<bool>(#"reflect.Type.of<Greeter>().implemented_by(build())"#)
            if is_member {
                return "witnessed"
            }
            return "not witnessed"
        }
        "#####
    );

    let BexExternalValue::String(result) = output
        .result
        .expect("session-declared interface should be witnessable")
    else {
        panic!("expected string result")
    };
    assert_eq!(result.as_str(), "witnessed");
}
