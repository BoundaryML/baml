// Shared real fixture data. Each call reads fresh values; tests never write the fixture.
function fixture_dir() -> string {
    for (let path in ["fixture", "tools/codeturtle/fixture", "../tools/codeturtle/fixture"]) {
        if (baml.fs.exists(path + "/changes.json")) {
            return path;
        }
    }
    throw "Run baml test from tools/codeturtle (or use --directory tools/codeturtle)";
}

function fixture_pr() -> root.PullRequest {
    baml.json.from_string<root.PullRequest>(baml.fs.read(fixture_dir() + "/changes.json"))
}

function fixture_owners() -> string {
    baml.fs.read(fixture_dir() + "/.github/codeturtle/OWNERS")
}

function fixture_sources() -> map<string, string> {
    root.load_sources(fixture_dir() + "/.github/codeturtle")
}

function check(
    owners: string? = null,
    extra: string? = null,
    pr: root.PullRequest? = null,
    concurrency: int = 8,
) -> root.Report {
    let sources = fixture_sources();
    if (extra != null) {
        sources["extra.baml"] = extra;
    }
    root.check_sources(
        pr ?? fixture_pr(),
        owners ?? fixture_owners(),
        sources,
        concurrency = concurrency,
    )
}

function requirements(report: root.Report, owner: string) -> root.Requirement[] {
    let results: root.Requirement[] = [];
    for (let requirement in report.requirements) {
        if (requirement.owners.includes(owner)) {
            results.push(requirement);
        }
    }
    results
}

function condition_results(report: root.Report, condition: string) -> root.Match[] {
    let results: root.Match[] = [];
    for (let evaluation in report.evaluations) {
        if (evaluation.condition == condition) {
            results.push(evaluation.result);
        }
    }
    results
}

function require_error(report: root.Report, fragment: string) -> void {
    assert.equal(report.evaluation_complete, false);
    assert.equal(report.errors.length(), 1);
    assert.contains(report.errors[0].message, fragment);
}

function lint(owners: string, source: string) -> root.LintReport {
    root.lint_sources(owners, { "predicate.baml": source })
}
