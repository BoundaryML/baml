function prompt(body: string) -> string {
    `
        function G() -> string {
          client: "openai/gpt-4o-mini"
          prompt: \`${body}\`
        }\n
    `
}

function untagged(body: string) -> string {
    `
        function G() -> string {
          let s = \`${body}\`;
          s
        }\n
    `
}

test "unclosed for reports on prompt and untagged templates" {
    root.compiler.AssertHasMessage(
        { "prompt.baml": prompt("${for (let x in [1])}${x}") },
        "unclosed ${for}",
    );
    root.compiler.AssertHasMessage(
        { "untagged.baml": untagged("${for (let x in [1])}${x}") },
        "unclosed ${for}",
    )
}

test "unclosed if reports on prompt and untagged templates" {
    root.compiler.AssertHasMessage({ "prompt.baml": prompt("${if (true)}yes") }, "unclosed ${if}");
    root.compiler.AssertHasMessage(
        { "untagged.baml": untagged("${if (true)}yes") },
        "unclosed ${if}",
    )
}

test "for closed by endif names endfor on both template paths" {
    root.compiler.AssertHasMessage(
        { "prompt.baml": prompt("${for (let x in [1])}${x}${endif}") },
        "${endfor}",
    );
    root.compiler.AssertHasMessage(
        { "untagged.baml": untagged("${for (let x in [1])}${x}${endif}") },
        "${endfor}",
    )
}

test "if closed by endfor names endif" {
    root.compiler.AssertHasMessage({ "prompt.baml": prompt("${if (true)}a${endfor}") }, "${endif}")
}

test "stray endfor reports on prompt and untagged templates" {
    root.compiler.AssertHasMessage({ "prompt.baml": prompt("a${endfor}b") }, "stray ${endfor}");
    root.compiler.AssertHasMessage({ "untagged.baml": untagged("a${endfor}b") }, "stray ${endfor}")
}

test "stray endif reports" {
    root.compiler.AssertHasMessage({ "prompt.baml": prompt("a${endif}b") }, "stray ${endif}")
}

test "stray else and else-if report independently" {
    root.compiler.AssertHasMessage({ "else.baml": prompt("a${else}b") }, "stray ${else}");
    root.compiler.AssertHasMessage(
        { "else_if.baml": prompt("a${else if (true)}b") },
        "stray ${else if}",
    )
}

test "out-of-order else branches report on both template paths" {
    root.compiler.AssertHasMessage(
        { "prompt_duplicate.baml": prompt("${if (true)}a${else}b${else}c${endif}") },
        "duplicate ${else}",
    );
    root.compiler.AssertHasMessage(
        { "untagged_duplicate.baml": untagged("${if (true)}a${else}b${else}c${endif}") },
        "duplicate ${else}",
    );
    root.compiler.AssertHasMessage(
        { "prompt_after.baml": prompt("${if (true)}a${else}b${else if (false)}c${endif}") },
        "${else if} after ${else}",
    );
    root.compiler.AssertHasMessage(
        { "untagged_after.baml": untagged("${if (true)}a${else}b${else if (false)}c${endif}") },
        "${else if} after ${else}",
    )
}

test "unresolved interpolation name reports on both paths" {
    root.compiler.AssertHasMessage({ "untagged.baml": untagged("${ nope }") }, "nope");
    root.compiler.AssertHasMessage({ "prompt.baml": prompt("${ nope }") }, "nope")
}

test "unresolved statement-only interpolation name reports" {
    root.compiler.AssertHasMessage({ "untagged.baml": untagged("${ let a = nope }${a}") }, "nope")
}

test "nullable interpolation keeps original diagnostic and suppresses synthetic call noise" {
    let files = {
        "main.baml": `
            function G(a: string?) -> string {
              let s = \`\${a}\`;
              s
            }
        `,
    };
    root.compiler.AssertHasMessage(files, "null");
    root.compiler.AssertLacksMessage(files, "it cannot be called")
}

test "prompt empty interpolation reports a successful warning" {
    root.compiler.AssertSuccessHasMessage({ "prompt.baml": prompt("x${}y") }, "empty interpolation");
    root.compiler.AssertSuccessHasMessage(
        { "untagged.baml": untagged("x${}y") },
        "empty interpolation",
    )
}

test "const for binding reaches the ordinary const-to-let advisory" {
    let files = { "prompt.baml": prompt("${for (const x in [1, 2])}${x}${endfor}") };
    root.compiler.AssertSuccessHasMessage(files, "treated like `let`");
    root.compiler.AssertSuccessLacksMessage(files, "unexpected");
    root.compiler.AssertSuccessLacksMessage(files, "unclosed");
    root.compiler.AssertSuccessLacksMessage(files, "stray")
}
