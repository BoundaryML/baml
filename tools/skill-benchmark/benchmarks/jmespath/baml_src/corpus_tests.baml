enum CorpusLicense {
    Mit,
}

class CorpusSourceFile {
    path: string,
    sha256: string,
}

class CorpusSource {
    repository: string,
    commit: string,
    license: CorpusLicense,
    license_path: string,
    files: CorpusSourceFile[],
}

class ComplianceSuccessCase {
    id: string,
    expression: string,
    comment: string?,
    expected: baml.json.json,
}

class ComplianceErrorCase {
    id: string,
    expression: string,
    comment: string?,
    error: JmesPathErrorKind,
}

type ComplianceCase = ComplianceSuccessCase | ComplianceErrorCase;

class ComplianceSuite {
    id: string,
    source_path: string,
    source_suite_index: int,
    comment: string?,
    given: baml.json.json,
    cases: ComplianceCase[],
}

class JmesPathCorpus {
    schema_version: int,
    source: CorpusSource,
    correctness_case_count: int,
    suites: ComplianceSuite[],
}

function load_jmespath_corpus() -> JmesPathCorpus {
    let corpus = baml.json.from_string<JmesPathCorpus>(baml.fs.read("testdata/jmespath-compliance.json"));
    assert.equal(corpus.schema_version, 1);
    assert.equal(corpus.correctness_case_count, 892);
    corpus
}

function assert_compliance_success(suite: ComplianceSuite, c: ComplianceSuccessCase) -> null {
    match (jmespath_search(c.expression, InlineJsonSource { value: suite.given })) {
        JmesPathSuccess { value: let actual } => assert.equal(actual, c.expected),
        let failure: JmesPathFailure => {
            baml.sys.panic(`expected success, got ${failure.kind}: ${failure.message}`)
        },
    }
}

function assert_compliance_error(suite: ComplianceSuite, c: ComplianceErrorCase) -> null {
    match (jmespath_search(c.expression, InlineJsonSource { value: suite.given })) {
        let success: JmesPathSuccess => {
            baml.sys.panic(`expected ${c.error}, got success ${success.value}`)
        },
        JmesPathFailure { kind: let actual } => assert.equal(actual, c.error),
    }
}

function assert_compliance_case(suite: ComplianceSuite, c: ComplianceCase) -> null {
    match (c) {
        let success: ComplianceSuccessCase => assert_compliance_success(suite, success),
        let failure: ComplianceErrorCase => assert_compliance_error(suite, failure),
    }
}

testset "jmespath_compliance" {
    for (let suite in load_jmespath_corpus().suites) {
        for (let c in suite.cases) {
            test c.id {
                assert_compliance_case(suite, c)
            }
        }
    }
}
