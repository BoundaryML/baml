function AssertOpaqueRejected(source: string, code: string, message: string) -> void throws never {
    let diagnostics = root.compiler.CompileDiagnostics({ "case.baml": source });
    assert.equal(diagnostics.length() > 0, true);
    if (
        !diagnostics.some((diagnostic) -> bool {
            diagnostic.code == code && diagnostic.message.includes(message)
        })
    ) {
        throw baml.panics.AssertionFailed {
            message: source + "\n" + diagnostics[0].code + ": " + diagnostics[0].message,
        };
    }
}

test "opaque fields cannot be read or reassigned" {
    for (let class_name in ["trace.Options", "trace.ReservedSpan", "trace.SpanId"]) {
        AssertOpaqueRejected(`function F(value: ${class_name}) -> unknown { value._handle }`, "E0007", "_handle");
        AssertOpaqueRejected(`function F(value: ${class_name}) -> void { value._handle = null; }`, "E0007", "_handle");
        // Exported class patterns currently reject before field lookup.
        let diagnostics = root.compiler.CompileDiagnostics(
            {
                "case.baml": `
                    function F(value: ${class_name}) -> unknown {
                                    match (value) {
                                        ${class_name} { _handle } => _handle,
                                    }
                                }
                `,
            },
        );
        assert.equal(diagnostics.length() > 0, true);
    }
}

test "opaque constructors reject empty literals and spreads" {
    for (let class_name in ["trace.Options", "trace.ReservedSpan", "trace.SpanId"]) {
        AssertOpaqueRejected(
            `function F() -> ${class_name} { ${class_name} {} }`,
            "E0001",
            "cannot construct opaque class",
        );
        AssertOpaqueRejected(
            `function F(value: ${class_name}) -> ${class_name} { ${class_name} { ...value } }`,
            "E0001",
            "cannot construct opaque class",
        );
        AssertOpaqueRejected(
            `function F(value: ${class_name}) -> ${class_name} { ${class_name} { _handle: value._handle } }`,
            "E0001",
            "cannot construct opaque class",
        );
    }
}

test "runtime filenames cannot grant builtin access" {
    for (
        let filename in [
            "../<builtin>/spoof.baml",
            "../<builtin>/trace/spoof.baml",
            "..\\<builtin>\\spoof.baml",
        ]
    ) {
        let sources: map<string, string> = {};
        sources[filename] = `function F(value: trace.Options) -> unknown { value._handle }`;
        let fields = root.compiler.CompileDiagnostics(sources);
        assert.equal(
            fields.some((diagnostic) -> bool {
                diagnostic.code == "E_RUNTIME_SOURCE"
            }),
            true,
        );
        sources[filename] = `function F() -> trace.Options { trace.Options {} }`;
        let constructors = root.compiler.CompileDiagnostics(sources);
        assert.equal(
            constructors.some((diagnostic) -> bool {
                diagnostic.code == "E_RUNTIME_SOURCE"
            }),
            true,
        );
    }
}

test "native state protection is not trace specific" {
    AssertOpaqueRejected(`function F(value: reflect.class.Builder) -> unknown { value._handle }`, "E0007", "_handle");
    AssertOpaqueRejected(
        `
            function F(value: reflect.class.Builder) -> reflect.class.Builder {
                        reflect.class.Builder { ...value }
                    }
        `,
        "E0001",
        "cannot construct opaque class",
    );
    root.compiler.AssertCompiles(
        {
            "case.baml": `
                class UserState { _handle: int }
                function F() -> int {
                    let value = UserState { _handle: 1 };
                    value._handle = 2;
                    value._handle
                }
            `,
        },
    );
}

test "reflection cannot extract opaque state" {
    let value = trace.options();
    let rejected = reflect.class.get_field<unknown>(value, "_handle") catch (error) {
        reflect.errors.CompilationError => true,
    };
    assert.equal(rejected, true);
    let reflected: reflect.AnyClass = value;
    assert.equal(reflected.has_field("_handle"), false);
    assert.equal(reflected.get_field("_handle"), null);
    assert.equal(reflected.list_fields().length(), 0);
    let class_type: reflect.class.Type = reflect.Type.of<trace.Options>().as_class() else {
        throw baml.panics.UserPanic { message: "Options must remain a class" };
    };
    assert.equal(class_type.fields().length(), 0);
}
