function assert_code_and_messages(
    files: map<string, string>,
    code: string,
    needles: string[],
) -> void throws never {
    for (let diagnostic in root.compiler.CompileDiagnostics(files)) {
        if (diagnostic.code != code) {
            continue;
        }
        let matches = true;
        for (let needle in needles) {
            if (!diagnostic.message.includes(needle)) {
                matches = false;
            }
        }
        if (matches) {
            return;
        }
    }
    baml.sys.panic(`expected ${code} diagnostic containing ${needles.join(", ")}`)
}

test "AnyClass requirement rejects a primitive implementor" {
    root.compiler.AssertHasCode(
        {
            "main.baml": `
                interface Tagged requires reflect.AnyClass {
                    function tag(self) -> string throws never
                }
                implements Tagged for int {
                    function tag(self) -> string { "primitive" }
                }
            `,
        },
        "E0125",
    )
}

test "AnyClass blanket bound does not expose members on nonclasses" {
    root.compiler.AssertHasCode(
        {
            "main.baml": `
                interface Wrapped {
                    function tag(self) -> string throws never
                }
                class Box<T> { value T }
                implements<T extends reflect.AnyClass> Wrapped for Box<T> {
                    function tag(self) -> string { "wrapped:" + self.value.type().to_string() }
                }
                function bad(value: Box<int>) -> string { value.tag() }
            `,
        },
        "E0007",
    )
}

test "pending runtime type is not a static reflect Type" {
    root.compiler.AssertHasCode(
        {
            "main.baml": `
                function accepts_type(value: reflect.Type) -> bool { true }
                function main() -> bool {
                    let builder = reflect.class.builder("StillPending")
                    accepts_type(builder.type())
                }
            `,
        },
        "E0001",
    )
}

test "unreflect operand must be a type value" {
    root.compiler.AssertHasCode(
        {
            "main.baml": `
                function generic<T>() -> bool { true }
                function main() -> bool {
                    type Bad = unreflect(42)
                    generic<Bad>()
                }
            `,
        },
        "E0001",
    )
}

test "unknown values do not expose get_field" {
    assert_code_and_messages(
        {
            "main.baml": `
                function inspect(value: unknown) -> string {
                    value.get_field<string>("name")
                }
            `,
        },
        "E0007",
        ["get_field", "unknown"],
    )
}

test "computed generic argument diagnostic names unreflect" {
    assert_code_and_messages(
        {
            "main.baml": `
                function id<T>(value: T) -> T { value }
                function main() -> unknown {
                    let runtime_t = reflect.Type.of<string>()
                    id<runtime_t>("x")
                }
            `,
        },
        "E0002",
        ["unreflect"],
    )
}

test "runtime duplicate serialized key matches the static diagnostic" {
    let diagnostics = root.compiler.CompileDiagnostics(
        {
            "main.baml": `
                class Collision {
                    wire string
                    internal int @alias("wire")
                }
            `,
        },
    );
    let static_matches = diagnostics.filter((diagnostic) -> bool {
        diagnostic.code == "E0149"
    });
    assert.equal(static_matches.length(), 1);
    let expected = static_matches[0];

    let result = reflect.class.new(
        "Collision",
        {
            "wire": reflect.Type.of<string>(),
            "internal": reflect.Type.of<int>().meta(alias = "wire"),
        },
    ) catch (error) {
        reflect.errors.CompilationError => {
            error.diagnostics[0].code + "|" + error.diagnostics[0].message
        },
    };
    let actual: string = result else {
        throw "runtime duplicate serialized key did not throw"
    };
    // The Rust oracle compared the compiler's unflattened headline with the
    // runtime-generated message. The legacy reflected `message` also appends
    // the primary label, so use the newly exposed headline here.
    assert.equal(actual, expected.code + "|" + expected.headline)
}

test "top-level runtime type binding is rejected" {
    root.compiler.AssertHasMessage(
        {
            "main.baml": `
                type T = unreflect(reflect.Type.of<string>())
                function main() -> string { "unreachable" }
            `,
        },
        "this runtime type must be given a name before it can be used here",
    )
}

test "block-scoped runtime type binding does not escape" {
    assert_code_and_messages(
        {
            "main.baml": `
                function invalid() -> unknown {
                    let t = reflect.Type.of<string>()
                    let escaped: unknown = {
                        type T = unreflect(t)
                        null
                    }
                    let invalid: T = escaped
                    invalid
                }
                function main() -> null { null }
            `,
        },
        "E0002",
        ["unresolved type", "T"],
    )
}
