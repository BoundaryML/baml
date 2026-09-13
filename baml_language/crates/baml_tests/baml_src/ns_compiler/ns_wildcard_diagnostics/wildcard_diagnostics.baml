function with_slow(body: string) -> string {
    "class MyErr { msg: string }\n"
        + "function slow(x: int) -> int throws MyErr {\n"
        + "  if (x < 0) { throw MyErr { msg: \"neg\" }; }\n"
        + "  x\n}\n"
        + body
}

test "expression-position wildcard holes are rejected cleanly" {
    root.compiler.AssertHasCode(
        {
            "turbofish.baml": `
                function pick<T, U>(x: T) -> T { x }
                function main() -> int { pick<int, _>(5) }
            `,
        },
        "E0147",
    );
    root.compiler.AssertHasCode(
        {
            "generic_apply.baml": `
                function id<T>(x: T) -> T { x }
                function main() -> int { let f = id<_>; f(5) }
            `,
        },
        "E0147",
    );
    root.compiler.AssertHasCode(
        {
            "upcast.baml": `
                interface Show<T> { function show(self) -> T throws never }
                class C {
                    v: int,
                    implements Show<int> {
                        function show(self) -> int { self.v }
                    }
                }
                function main() -> int {
                    let c = C { v: 5 };
                    let s = c.as<Show<_>>;
                    0
                }
            `,
        },
        "E0147",
    )
}

test "non-inferable declaration wildcard positions report E0147" {
    root.compiler.AssertHasCode({ "return.baml": "function main() -> _ { 5 }" }, "E0147");
    root.compiler.AssertHasCode(
        { "parameter.baml": "function f(x: _) -> int { 0 }\nfunction main() -> int { 0 }" },
        "E0147",
    );
    root.compiler.AssertHasCode(
        { "field.baml": "class C { x: _ }\nfunction main() -> int { 0 }" },
        "E0147",
    );
    root.compiler.AssertHasCode(
        { "bound.baml": "function f<T extends _>(x: T) -> int { 0 }\nfunction main() -> int { 0 }" },
        "E0147",
    );
    root.compiler.AssertHasCode(
        { "requires.baml": "interface I requires _ {}\nfunction main() -> int { 0 }" },
        "E0147",
    );
    root.compiler.AssertHasCode(
        {
            "nested_throws.baml": "class Err<T> { v: T }\nfunction f() -> int throws Err<_> { throw Err<int> { v: 1 }; }\nfunction main() -> int { 0 }",
        },
        "E0147",
    );
    root.compiler.AssertHasCode(
        {
            "interface_throws.baml": "interface I {\n  function run(self) -> int throws _\n}\nfunction main() -> int { 0 }",
        },
        "E0147",
    )
}

test "future wildcard inference preserves its precise contracts" {
    root.compiler.AssertHasCode(
        {
            "not_erased.baml": with_slow(
                "function main() -> int throws never {\n  let fs: baml.future.Future<int, _>[] = [spawn { slow(1) }];\n  await fs[0]\n}\n",
            ),
        },
        "E0096",
    );
    root.compiler.AssertHasCode(
        {
            "never.baml": with_slow(
                "function main() -> int {\n  let fs: baml.future.Future<int, never>[] = [spawn { slow(1) }];\n  await fs[0]\n}\n",
            ),
        },
        "E0001",
    );
    root.compiler.AssertHasCode(
        {
            "wrong_value.baml": with_slow(
                "function main() -> int {\n  let fs: baml.future.Future<string, _>[] = [spawn { slow(1) }];\n  0\n}\n",
            ),
        },
        "E0001",
    )
}

test "throws wildcard is open to inferred errors while plain throws remains exhaustive" {
    let open_source = `
        class BadInput { msg: string }
        function parse(s: string) -> int throws BadInput | _ {
            let v: json = baml.json.from_string(s);
            throw BadInput { msg: "nope" };
        }
        function caller(s: string) -> int throws never { parse(s) }
        function main() -> int { 0 }
    `;
    root.compiler.AssertHasMessage({ "open.baml": open_source }, "ParseError");

    let closed_source = `
        class BadInput { msg: string }
        function parse(s: string) -> int throws BadInput {
            let v: json = baml.json.from_string(s);
            throw BadInput { msg: "nope" };
        }
        function main() -> int { 0 }
    `;
    root.compiler.AssertHasCode({ "closed.baml": closed_source }, "E0096")
}
