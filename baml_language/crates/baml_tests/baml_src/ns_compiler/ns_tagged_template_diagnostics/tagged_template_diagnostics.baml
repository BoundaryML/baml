function valid_tag() -> string {
    `
        //baml:tagged_string
        function sql(body: (x: int) -> baml.TaggedString) -> string {
          "ok"
        }\n
    `
}

function role_tag() -> string {
    `
        //baml:tagged_string
        function chat(body: (role: string) -> baml.TaggedString) -> string {
          "ok"
        }\n
    `
}

test "unmarked tagged-string function is rejected" {
    root.compiler.AssertHasMessage(
        {
            "main.baml": `
                function sql(body: (x: int) -> baml.TaggedString) -> string { "ok" }
                function Demo() -> string { sql\`hi \${1}\` }
            `,
        },
        "not a tagged-string function",
    )
}

test "unresolved tag reports exactly once" {
    root.compiler.AssertMessageCount(
        {
            "main.baml": `
                function Demo() -> string { nope\`hi \${1}\` }
            `,
        },
        "nope",
        1,
    )
}

test "non-function tagged-string target is rejected" {
    root.compiler.AssertHasMessage(
        {
            "main.baml": `
                function Demo() -> string {
                  let sql = 3
                  sql\`hi \${1}\`
                }
            `,
        },
        "is not a function",
    )
}

test "tagged-string body parameter shape matrix is rejected" {
    root.compiler.AssertHasMessage(
        {
            "wrong_name.baml": `
                //baml:tagged_string
                function sql(notbody: (x: int) -> baml.TaggedString) -> string { "ok" }
                function Demo() -> string { sql\`hi \${1}\` }
            `,
        },
        "must be `body:",
    );
    root.compiler.AssertHasMessage(
        {
            "wrong_return.baml": `
                //baml:tagged_string
                function sql(body: (x: int) -> string) -> string { "ok" }
                function Demo() -> string { sql\`hi \${1}\` }
            `,
        },
        "must be `body:",
    );
    root.compiler.AssertHasMessage(
        {
            "missing.baml": `
                //baml:tagged_string
                function sql() -> string { "ok" }
                function Demo() -> string { sql\`hi \${1}\` }
            `,
        },
        "must be `body:",
    )
}

test "unknown tagged interpolation name is unresolved" {
    root.compiler.AssertHasMessage(
        {
            "main.baml": role_tag()
                + `
                    function Demo() -> string { chat\`hello \${nonexistent}\` }
                `,
        },
        "unresolved name: `nonexistent`",
    )
}

test "tagged for binding is out of scope after endfor" {
    root.compiler.AssertHasMessage(
        {
            "main.baml": role_tag()
                + `
                    function Demo(items: int[]) -> string { chat\`\${for (let x in items)}a\${endfor}\${x}\` }
                `,
        },
        "unresolved name: `x`",
    )
}

test "tagged for rejects a non-iterable collection" {
    root.compiler.AssertHasMessage(
        {
            "main.baml": role_tag()
                + `
                    function Demo(n: int) -> string { chat\`\${for (let x in n)}a\${endfor}\` }
                `,
        },
        "cannot iterate over type",
    )
}

test "tagged body parameter does not leak after template" {
    root.compiler.AssertHasMessage(
        {
            "main.baml": role_tag()
                + `
                    function Demo() -> string {
                      let r = chat\`hi \${role}\`
                      role
                    }
                `,
        },
        "unresolved name: `role`",
    )
}
