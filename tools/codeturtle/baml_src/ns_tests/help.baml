test "help discovers public schemas and excludes internal bookkeeping" {
    let report = root.agent_help();
    assert.is_true(report.types.length() > 0);
    for (let type in report.types) {
        assert.is_true(type.description != null);
        assert.equal((type.description ?? "").starts_with("(internal)"), false);
        assert.equal(
            type.name,
            "codeturtle." + type.name.slice("codeturtle.".length(), type.name.length()),
        );
        for (let member in type.members) {
            assert.is_true(member.description != null);
        }
    }
    assert.equal(
        report.contracts.map((c) -> {
            c.input
        }),
        ["FileChange", "FileChange[]", "PullRequest"],
    );
    assert.equal(
        report.contracts.map((c) -> {
            c.returns
        }),
        ["Match | bool", "Match[] | bool", "Match[] | bool"],
    );
}

test "help reflects new classes, fields and enum docs without a schema registry" {
    let package = reflect.Package.compile(
        {
            "future.baml": `
                /// A newly introduced context.
                class FutureContext {
                    /// Independently documented revision.
                    revision: string?,
                }
                /// Newly introduced choices.
                enum FutureChoice {
                    /// First future choice.
                    First,
                }
            `,
        },
    );
    let report = root.describe_package(package);
    assert.equal(report.types.length(), 2);
    assert.equal(report.types[0].name, "codeturtle.FutureContext");
    assert.equal(report.types[0].description, "A newly introduced context.");
    assert.equal(report.types[0].members[0].type, "string | null");
    assert.equal(report.types[0].members[0].description, "Independently documented revision.");
    assert.equal(report.types[1].name, "codeturtle.FutureChoice");
    assert.equal(report.types[1].members[0].description, "First future choice.");
}

function help_example(start: string, end: string) -> string {
    let block = root.authoring_guide().split(start)[1].split(end)[0];
    let lines: string[] = [];
    for (let line in block.lines()) {
        lines.push(
            if (line.starts_with("  ")) {
                line.slice(2, line.length())
            } else {
                line
            },
        );
    }
    lines.join("\n")
}

test "published help examples compile and lint as direct predicates" {
    let source = help_example(".github/codeturtle/ns_checks/contracts.baml\n", "  AI functions work directly.")
        + "\n"
        + help_example("Or use ordinary code\n", "\nRules");
    let report = lint(
        "* @owner # codeturtle: when=needs_contract_review\n* @owner # codeturtle: when=adds_env_read\n",
        source,
    );
    assert.is_true(report.valid);
    assert.equal(report.conditions.length(), 2);
}
