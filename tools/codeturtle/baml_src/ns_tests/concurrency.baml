function parallel_case() -> bool {
    with_mock((mock) -> {
        let owners = "* @baseline\n* @a # codeturtle: when=slow\n* @b # codeturtle: when=slow\n"
            + "* @multi # codeturtle: when=slow_files\n* @pr # codeturtle: when=slow_pr\n";
        let sources = io_sources(mock);
        let parallel = root.check_sources(fixture_pr(), owners, sources, concurrency = 3);
        assert.is_true(parallel.evaluation_complete);
        assert.equal(mock.requests.length(), 10);
        assert.is_true(mock.observed_peak() >= 2 && mock.observed_peak() <= 3);
        assert.equal(parallel.requirements.length(), 20);
        mock.requests = [];
        mock.completed = [];
        mock.overlap = [];
        let sequential = root.check_sources(fixture_pr(), owners, sources, concurrency = 1);
        assert.equal(mock.requests.length(), 10);
        assert.equal(mock.observed_peak(), 1);
        assert.equal(baml.json.to_string(parallel), baml.json.to_string(sequential));
        true
    })
}

test "file, files and PR calls share one concurrency cap and stable output" {
    assert.is_true(parallel_case());
}

function failed_parallel_case() -> bool {
    with_mock((mock) -> {
        let report = root.check_sources(
            fixture_pr(),
            "* @baseline\n* @bad # codeturtle: when=faulty\n* @good # codeturtle: when=slow\n",
            io_sources(mock),
            concurrency = 3,
        );
        assert.equal(mock.requests.length(), 8);
        require_error(report, "Intentional failure");
        assert.equal(report.requirements.length(), 8);
        assert.equal(requirements(report, "@bad"), []);
        assert.equal(requirements(report, "@good").length(), 4);
        true
    })
}

test "failed parallel rule drains every invocation and retains successful rules" {
    assert.is_true(failed_parallel_case());
}

test "file, array and PR inputs are isolated from extension mutation" {
    let sources = {
        "mutation.baml": `
            function mutate_file(change: codeturtle.FileChange) -> bool {
                change.added_code = "corrupt"; false
            }
            function mutate_files(changes: codeturtle.FileChange[]) -> bool {
                for (let change in changes) { change.added_code = "corrupt"; } false
            }
            function mutate_pr(pr: codeturtle.PullRequest) -> bool {
                pr.title = "corrupt";
                for (let change in pr.changes) { change.added_code = "corrupt"; } false
            }
            function verify(pr: codeturtle.PullRequest) -> bool {
                if (pr.title == "corrupt") { throw "Shared PR was mutated"; }
                for (let change in pr.changes) {
                    if (change.added_code == "corrupt") { throw "Shared file was mutated"; }
                }
                true
            }
        `,
    };
    let owners = "* @baseline\n* @mutator # codeturtle: when=mutate_file\n"
        + "* @mutator # codeturtle: when=mutate_files\n* @mutator # codeturtle: when=mutate_pr\n"
        + "* @verifier # codeturtle: when=verify\n";
    let sequential = root.check_sources(fixture_pr(), owners, sources, concurrency = 1);
    let parallel = root.check_sources(fixture_pr(), owners, sources, concurrency = 4);
    assert.is_true(parallel.evaluation_complete);
    assert.equal(parallel.requirements.length(), 8);
    assert.equal(baml.json.to_string(parallel), baml.json.to_string(sequential));
}
