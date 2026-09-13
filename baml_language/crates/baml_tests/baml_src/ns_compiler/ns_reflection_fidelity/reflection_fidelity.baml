function prompt(body: string) -> string {
    "function G() -> string {\n  client: \"openai/gpt-4o-mini\"\n  prompt: `" + body + "`\n}\n"
}

test "failed compile exposes structured diagnostic fidelity" {
    let diagnostics = root.compiler.CompileDiagnostics(
        {
            "first.baml": "class Widget { first int }",
            "second.baml": "class Widget { second string }",
        },
    );
    assert.equal(diagnostics.length(), 1);

    let diagnostic = diagnostics[0];
    assert.equal(diagnostic.code, "E0011");
    assert.equal(diagnostic.severity, "error");
    assert.equal(diagnostic.phase, "validation");
    assert.equal(diagnostic.headline, "duplicate class `Widget`");
    assert.equal(diagnostic.primary_label, "duplicate class definition");
    assert.equal(diagnostic.message, "duplicate class `Widget`: duplicate class definition");
    assert.equal(diagnostic.message_highlights.length(), 1);
    assert.equal(diagnostic.message_highlights[0].kind, "code");
    assert.equal(diagnostic.annotations.length(), 2);
    let primary = diagnostic.annotations.filter((annotation) -> bool {
        annotation.is_primary
    });
    let secondary = diagnostic.annotations.filter((annotation) -> bool {
        !annotation.is_primary
    });
    assert.equal(primary.length(), 1);
    assert.equal(secondary.length(), 1);
    assert.equal(primary[0].message, diagnostic.primary_label);
    assert.equal(primary[0].message_highlights.length(), 0);
    assert.equal(secondary[0].message_highlights.length(), 0);
    let files = diagnostic.annotations.map((annotation) -> string? {
        annotation.span.file
    });
    assert.is_true(files.includes("first.baml"));
    assert.is_true(files.includes("second.baml"));
    assert.equal(diagnostic.related_info.length(), 0)
}

test "successful public compile exposes warnings" {
    let diagnostics = root.compiler.CompileSuccessDiagnostics(
        { "warning.baml": prompt("${for (const x in [1, 2])}${x}${endfor}") },
    );
    let matches = diagnostics.filter((diagnostic) -> bool {
        diagnostic.message.includes("treated like `let`")
    });
    assert.equal(matches.length(), 1);

    let warning = matches[0];
    assert.equal(warning.severity, "warning");
    assert.not_null(warning.phase);
    assert.is_true(warning.headline.includes("treated like `let`"));
    assert.is_true(warning.message_highlights.length() > 0);
    assert.equal(warning.annotations.length(), 1);
    assert.is_true(warning.annotations[0].is_primary);
    assert.equal(warning.annotations[0].span.file, "warning.baml")
}

test "compiler related locations and their highlights stay structured" {
    let diagnostics = root.compiler.CompileDiagnostics(
        {
            "tag.baml": "function sql(body: (x: int) -> baml.TaggedString) -> string { \"ok\" }\nfunction Demo() -> string { sql`hi ${1}` }\n",
        },
    );
    let matches = diagnostics.filter((diagnostic) -> bool {
        diagnostic.message.includes("not a tagged-string function")
    });
    assert.equal(matches.length(), 1);

    let diagnostic = matches[0];
    assert.equal(diagnostic.severity, "error");
    assert.equal(diagnostic.phase, "type");
    assert.equal(diagnostic.related_info.length(), 1);
    let related = diagnostic.related_info[0];
    assert.equal(related.span.file, "tag.baml");
    assert.is_true(related.message.includes("add a `//baml:tagged_string` marker"));
    assert.equal(related.message_highlights.length(), 1);
    assert.equal(related.message_highlights[0].kind, "code")
}
