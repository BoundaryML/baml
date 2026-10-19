// Reserved names: keywords never name a declaration, and built-in type names
// never name an item or type parameter. Each source compiles as its own
// package, so every case reports exactly its own diagnostics.

function reserved_names_first_error(source: string) -> string {
    let result = reflect.Package.compile({ "main.baml": source }) catch (e) {
        reflect.errors.CompilationError => e.diagnostics[0].code + "|" + e.diagnostics[0].message,
    };
    if (result is string) {
        result
    } else {
        "ok"
    }
}

function reserved_names_builtin_types() -> string[] {
    [
        "int",
        "bigint",
        "float",
        "string",
        "bool",
        "uint8array",
        "image",
        "audio",
        "video",
        "pdf",
        "map",
        "json",
        "void",
        "never",
        "unknown",
    ]
}

function reserved_names_keywords() -> string[] {
    [
        "class",
        "enum",
        "interface",
        "implements",
        "implement",
        "extends",
        "requires",
        "function",
        "if",
        "else",
        "for",
        "while",
        "let",
        "in",
        "break",
        "continue",
        "return",
        "throw",
        "match",
        "catch",
        "catch_all",
        "catch_all_panics",
        "throws",
        "spawn",
        "await",
        "defer",
        "is",
        "const",
        "true",
        "false",
        "null",
        "self",
        "root",
    ]
}

function reserved_names_weak_keywords() -> string[] {
    [
        "client",
        "generator",
        "test",
        "testset",
        "retry_policy",
        "template_string",
        "type",
        "as",
        "with",
        "instanceof",
    ]
}

test "built-in type names cannot name an item or type parameter" {
    for (let name in reserved_names_builtin_types()) {
        let reserved = "E0176|`" + name + "` is a built-in type name and cannot name ";
        let label = ": `" + name + "` is reserved";
        assert.equal(reserved_names_first_error("class " + name + " { x: int }"), reserved + "a class" + label);
        assert.equal(reserved_names_first_error("enum " + name + " { A }"), reserved + "an enum" + label);
        assert.equal(reserved_names_first_error("interface " + name + " { }"), reserved + "an interface" + label);
        assert.equal(reserved_names_first_error("type " + name + " = int"), reserved + "a type alias" + label);
        assert.equal(
            reserved_names_first_error("function " + name + "() -> int { 1 }"),
            reserved + "a function" + label,
        );
        assert.equal(
            reserved_names_first_error("function f<" + name + ">(x: int) -> int { x }"),
            reserved + "a type parameter" + label,
        );
        assert.equal(
            reserved_names_first_error("class Box<" + name + "> { x: int }"),
            reserved + "a type parameter" + label,
        );
    }
}

test "built-in type names stay legal as members, parameters, and locals" {
    for (let name in reserved_names_builtin_types()) {
        assert.equal(reserved_names_first_error("class C { " + name + ": int }"), "ok");
        assert.equal(
            reserved_names_first_error("class C { function " + name + "(self) -> int { 1 } }"),
            "ok",
        );
        assert.equal(reserved_names_first_error("enum E { " + name + " }"), "ok");
        assert.equal(
            reserved_names_first_error("function f(" + name + ": int) -> int { " + name + " }"),
            "ok",
        );
        assert.equal(
            reserved_names_first_error("function f() -> int { let " + name + " = 1; " + name + " }"),
            "ok",
        );
    }
}

test "keywords cannot name a function or method" {
    for (let name in reserved_names_keywords()) {
        let reserved = "E0176|`" + name + "` is a keyword and cannot name ";
        let label = ": `" + name + "` is reserved";
        assert.equal(
            reserved_names_first_error("function " + name + "() -> int { 1 }"),
            reserved + "a function" + label,
        );
        assert.equal(
            reserved_names_first_error("class C { function " + name + "(self) -> int { 1 } }"),
            reserved + "a method" + label,
        );
        assert.equal(
            reserved_names_first_error("interface I { function " + name + "(self) -> int }"),
            reserved + "a method" + label,
        );
    }
}

test "keywords cannot name any declaration" {
    for (let name in reserved_names_keywords()) {
        let reserved = "E0176|`" + name + "` is a keyword and cannot name ";
        let label = ": `" + name + "` is reserved";
        assert.equal(reserved_names_first_error("class " + name + " { x: int }"), reserved + "a class" + label);
        assert.equal(reserved_names_first_error("enum " + name + " { A }"), reserved + "an enum" + label);
        assert.equal(reserved_names_first_error("interface " + name + " { }"), reserved + "an interface" + label);
        assert.equal(reserved_names_first_error("type " + name + " = int"), reserved + "a type alias" + label);
        assert.equal(
            reserved_names_first_error("function f<" + name + ">(x: int) -> int { x }"),
            reserved + "a type parameter" + label,
        );
        assert.equal(reserved_names_first_error("class C { " + name + ": int }"), reserved + "a field" + label);
        assert.equal(reserved_names_first_error("interface I { " + name + ": int }"), reserved + "a field" + label);
        assert.equal(reserved_names_first_error("enum E { " + name + " }"), reserved + "an enum variant" + label);
        assert.equal(
            reserved_names_first_error("function f(" + name + ": int) -> int { 1 }"),
            reserved + "a parameter" + label,
        );
        assert.equal(
            reserved_names_first_error("function f() -> int { let g = (" + name + ": int) -> int { 1 }; 1 }"),
            reserved + "a parameter" + label,
        );
        assert.equal(
            reserved_names_first_error("function f() -> int { let " + name + " = 1; 1 }"),
            reserved + "a binding" + label,
        );
        assert.equal(
            reserved_names_first_error("function f() -> int { 1 catch (" + name + ") { _ => 2 } }"),
            reserved + "a binding" + label,
        );
    }
}

test "weak keywords are ordinary names outside their declarations" {
    for (let name in reserved_names_weak_keywords()) {
        assert.equal(reserved_names_first_error("class " + name + " { x: int }"), "ok");
        assert.equal(reserved_names_first_error("class C { " + name + ": int }"), "ok");
        assert.equal(
            reserved_names_first_error("class C { x: int\n function " + name + "(self) -> int { self.x } }"),
            "ok",
        );
        assert.equal(
            reserved_names_first_error(
                "function f(" + name + ": int) -> int { let y = " + name + "; y }",
            ),
            "ok",
        );
        assert.equal(
            reserved_names_first_error("function g() -> int { 1 }\nfunction " + name + "() -> int { g() }"),
            "ok",
        );
    }
}

test "keyword-prefixed names are ordinary identifiers" {
    assert.equal(
        reserved_names_first_error(
            "class is_function { if_x: int }\nfunction class_name(self_ref: int) -> int { let root_y = self_ref; root_y }",
        ),
        "ok",
    )
}
