function direct_ai_case() -> bool {
    with_mock((mock) -> {
        let owners = "* @baseline\n* @file-team # codeturtle: when=needs_file_review\n"
            + "* @files-team # codeturtle: when=needs_files_review\n* @pr-team # codeturtle: when=needs_pr_review\n";
        let sources = ai_sources(mock);
        assert.is_true(root.lint_sources(owners, sources).valid);
        assert.equal(mock.requests, []);
        let parallel = root.check_sources(fixture_pr(), owners, sources, concurrency = 3);
        assert.is_true(parallel.evaluation_complete);
        assert.equal(mock.requests.length(), 6);
        assert.is_true(mock.observed_peak() >= 2 && mock.observed_peak() <= 3);
        for (let request in mock.requests) {
            assert.equal(request.url, "/systemone");
            assert.equal(request.headers.get("authorization"), "Bearer test-key");
        }
        assert.equal(
            requirements(parallel, "@file-team").map((r) -> {
                r.path
            }),
            ["src/timeout.rs"],
        );
        assert.equal(parallel.requirements.length(), 13);
        mock.requests = [];
        mock.completed = [];
        mock.overlap = [];
        let sequential = root.check_sources(fixture_pr(), owners, sources, concurrency = 1);
        assert.equal(mock.requests.length(), 6);
        assert.equal(mock.observed_peak(), 1);
        assert.equal(baml.json.to_string(parallel), baml.json.to_string(sequential));
        true
    })
}

test "direct AI predicates use default clients for every supported input scope" {
    assert.is_true(direct_ai_case());
}

function failed_ai_case() -> bool {
    with_mock((mock) -> {
        let owners = "* @baseline\n* @file-team # codeturtle: when=needs_file_review\n";
        let sources = ai_sources(mock);
        assert.is_true(root.lint_sources(owners, sources).valid);
        assert.equal(mock.requests, []);
        mock.fail = true;
        let report = root.check_sources(fixture_pr(), owners, sources, concurrency = 3);
        assert.equal(mock.requests.length(), 4);
        require_error(report, "JEV");
        assert.equal(report.requirements.length(), 4);
        assert.equal(requirements(report, "@file-team"), []);
        true
    })
}

test "model failure blocks its rule and retains baseline requirements" {
    assert.is_true(failed_ai_case());
}
