// Executable oracles for structured runtime interface witnesses and scoped types.

interface RiwPersonAnchor {
    name: string,
    email: string,
}

interface RiwPublicIdentity {
    type Key;
    key: Self.Key,
}

client RiwTestClient = openai.ResponsesClient.new(
    model = "gpt-4o-mini",
    api_key = "test-key",
    base_url = "http://localhost:1234",
)

function riw_extract_person<T>(input: string) -> T {
    client: RiwTestClient
    prompt: `Extract a person from ${input}.\n${ctx.output_format()}`
}

function riw_extract_equivalent_person<T>() -> T {
    client: RiwTestClient
    prompt: `${ctx.output_format()}`
}

interface RiwGreeter {
    name: string,
    function greet(self) -> string throws never {
        "hello, " + self.name
    }
}

function riw_extract_greeter<T>(input: string) -> T {
    client: RiwTestClient
    prompt: `Extract from ${input}.\n${ctx.output_format()}`
}

interface RiwNamedWithRequiredMethod {
    name: string,
    function describe(self) -> string throws never;
}

test "witnessed_runtime_class_implements_static_interface" {
    let witness = reflect.Interface.implementation<RiwPersonAnchor>().field("name").field("email");
    let person_t = reflect.Class.new(
        "WitnessedPerson",
        {
            "name": string.type(),
            "email": string.type(),
            "favorite_editor": string.type(),
        },
        implementations = [witness],
    );
    assert.is_true(person_t.implements(reflect.Type.of<RiwPersonAnchor>()))
}

test "witness_realizes_associated_field_binding_before_exact_type_check" {
    let witness = reflect.Interface.implementation<RiwPublicIdentity<Key = string>>().field(
        "key",
        class_field = "public_key",
    );
    let account_t = reflect.Class.new(
        "WitnessedAccount",
        { "public_key": string.type() },
        implementations = [witness],
    );
    assert.is_true(
        account_t.implements(reflect.Type.of<RiwPublicIdentity<Key = string>>()),
    )
}

test "scenario_four_pattern_one_uses_typed_anchor_and_runtime_leaves" {
    let anchor_impl = reflect.Interface.implementation<RiwPersonAnchor>().field("name").field(
        "email",
        class_field = "contact_email",
    );
    let person_t = reflect.Class.new(
        "ScenarioPerson",
        {
            "name": string.type(),
            "contact_email": string.type(),
            "favorite_editor": string.type(),
        },
        implementations = [anchor_impl],
    );
    //# Read a rigid runtime class through its witnessed static interface.
    let type ScenarioPerson = person_t;
    let prompt = riw_extract_person@render_prompt<ScenarioPerson>("sample").text();
    let person: RiwPersonAnchor = riw_extract_person@parse<ScenarioPerson>(`{"name":"Ada","contact_email":"ada@example.com","favorite_editor":"vim"}`) else {
        throw "the runtime class does not implement RiwPersonAnchor"
    };
    let runtime_leaf = reflect.Class.get_field<string>(person, "favorite_editor");
    let result = prompt + "\n<RESULT>" + person.name + "|" + person.email + "|" + runtime_leaf;
    assert.contains(result, "favorite_editor");
    assert.is_true(result.ends_with("<RESULT>Ada|ada@example.com|vim"))
}

test "incomplete_witness_fails_before_type_and_chain_is_immutable" {
    let base = reflect.Interface.implementation<RiwPersonAnchor>().field("name");
    let complete = base.field("email", class_field = "contact_email");
    let bad = reflect.Class.new(
        "IncompletePerson",
        { "name": string.type(), "contact_email": string.type() },
        implementations = [base],
    ) catch (e) {
        reflect.errors.CompilationError => e.diagnostics[0].code + "|" + e.diagnostics[0].message
    };
    let good = reflect.Class.new(
        "CompletePerson",
        { "name": string.type(), "contact_email": string.type() },
        implementations = [complete],
    );
    if (bad is string) {
        assert.is_true(bad.starts_with("E0001|"));
        assert.contains(bad, "missing required field `email`")
    } else {
        throw "missing witness did not throw"
    };
    assert.is_true(good.implements(reflect.Type.of<RiwPersonAnchor>()))
}

test "equivalent_witnessed_definitions_render_and_parse_identically" {
    let witness = reflect.Interface.implementation<RiwPersonAnchor>().field("name").field("email");
    let left = reflect.Class.new(
        "EquivalentPerson",
        { "name": string.type(), "email": string.type() },
        implementations = [witness],
    );
    let right = reflect.Class.new(
        "EquivalentPerson",
        { "name": string.type(), "email": string.type() },
        implementations = [witness],
    );
    let type LeftPerson = left;
    let type RightPerson = right;
    let left_prompt = riw_extract_equivalent_person@render_prompt<LeftPerson>().text();
    let right_prompt = riw_extract_equivalent_person@render_prompt<RightPerson>().text();
    let l: RiwPersonAnchor = riw_extract_equivalent_person@parse<LeftPerson>(`{"name":"Ada","email":"ada@example.com"}`) else {
        throw "left does not implement RiwPersonAnchor"
    };
    let r: RiwPersonAnchor = riw_extract_equivalent_person@parse<RightPerson>(`{"name":"Ada","email":"ada@example.com"}`) else {
        throw "right does not implement RiwPersonAnchor"
    };

    assert.is_true(left != right);
    assert.equal(left_prompt, right_prompt);
    assert.equal(l.name, r.name);
    assert.equal(l.email, r.email);
    assert.is_true(left.implements(reflect.Type.of<RiwPersonAnchor>()));
    assert.is_true(right.implements(reflect.Type.of<RiwPersonAnchor>()))
}

test "witness_inherits_interface_default_methods" {
    let witness = reflect.Interface.implementation<RiwGreeter>().field("name");
    let person_t = reflect.Class.new(
        "GreeterPerson",
        { "name": string.type() },
        implementations = [witness],
    );
    let is_member = person_t.implements(reflect.Type.of<RiwGreeter>());
    let type GreeterPerson = person_t;
    let person: RiwGreeter = riw_extract_greeter@parse<GreeterPerson>(`{"name":"Ada"}`) else {
        throw "the runtime class does not implement RiwGreeter"
    };
    assert.is_true(is_member);
    assert.equal(person.greet(), "hello, Ada")
}

test "witness_still_rejects_interfaces_with_required_methods" {
    let result = reflect.Interface.implementation<RiwNamedWithRequiredMethod>().field("name") catch (e) {
        reflect.errors.CompilationError => e.diagnostics[0].message
    };
    if (result is string) {
        assert.contains(result, "cannot be witnessed structurally");
        assert.contains(result, "`describe`")
    } else {
        throw "required-method interface did not throw"
    }
}

test "session_interface_default_methods_are_bound_and_inherited" {
    let s = reflect.Session.new();
    s.eval(
        `interface Greeter { who: string  function greet(self) -> string throws never { "hello" } }`,
    );
    // Keep construction inside a session-declared function: a top-level session
    // `let` combining a map literal and keyword argument is a known parser seam.
    s.eval(
        `
            function build() -> reflect.Type {
                let witness = reflect.Interface.implementation<Greeter>().field("who")
                let person_t = reflect.Class.new(
                    "SessionPerson",
                    { "who": string.type() },
                    implementations = [witness],
                )
                person_t.as_type()
            }
        `,
    );
    // `implemented_by` avoids the known session parser issue around `.implements`.
    assert.is_true(s.eval<bool>(`reflect.Type.of<Greeter>().implemented_by(build())`))
}
