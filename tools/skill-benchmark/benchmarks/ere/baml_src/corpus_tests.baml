class CorpusSourceFile {
    path: string,
    sha256: string,
}

class CorpusSource {
    id: string,
    repository: string,
    commit: string,
    license: string,
    files: CorpusSourceFile[],
}

class CorpusExpectation {
    kind: string,
    captures: Capture?[]?,
    capture_texts: string?[]?,
    error: string?,
    group_count: int?,
}

class CorpusProfile {
    status: string,
    reason: string?,
    canonical_case_id: string?,
    expectation: CorpusExpectation?,
}

class CorpusCase {
    id: string,
    source_id: string,
    source_path: string,
    source_start_line: int,
    source_end_line: int,
    raw: string,
    dialect: string,
    flags: string[],
    pattern: string,
    input: string?,
    source_expectation: CorpusExpectation,
    ere_ascii: CorpusProfile,
}

class EreCorpus {
    schema_version: int,
    sources: CorpusSource[],
    cases: CorpusCase[],
}

function load_ere_corpus() -> EreCorpus {
    let corpus = baml.json.from_string<EreCorpus>(baml.fs.read("testdata/ere-corpus.json"));
    assert.equal(corpus.schema_version, 1);
    corpus
}

function runnable_corpus_cases() -> CorpusCase[] {
    let runnable: CorpusCase[] = [];
    for (let c in load_ere_corpus().cases) {
        if (c.ere_ascii.status == "included") {
            runnable.push(c);
        }
    }
    runnable
}

function corpus_input(c: CorpusCase) -> string {
    match (c.input) {
        null => "",
        let input: string => input,
    }
}

function required_expectation(c: CorpusCase) -> CorpusExpectation {
    match (c.ere_ascii.expectation) {
        null => baml.sys.panic("included corpus case has no expectation"),
        let expected: CorpusExpectation => expected,
    }
}

function required_captures(expected: CorpusExpectation) -> Capture?[] {
    match (expected.captures) {
        null => baml.sys.panic("exact match expectation has no captures"),
        let captures: Capture?[] => captures,
    }
}

function required_capture_texts(expected: CorpusExpectation) -> string?[] {
    match (expected.capture_texts) {
        null => baml.sys.panic("capture text expectation has no captures"),
        let captures: string?[] => captures,
    }
}

function assert_capture_texts(actual: EreMatch?, expected: string?[]) -> null {
    match (actual) {
        null => baml.sys.panic("expected a match"),
        let found: EreMatch => {
            let texts: string?[] = [];
            for (let capture in found.captures) {
                texts.push(
                    match (capture) {
                        null => null,
                        let value: Capture => value.text,
                    },
                );
            }
            assert.equal(texts, expected)
        },
    }
}

function assert_exact_prefix(actual: EreMatch?, expected: Capture?[]) -> null {
    match (actual) {
        null => baml.sys.panic("expected a match"),
        let found: EreMatch => {
            assert.is_true(found.captures.length() >= expected.length());
            let index = 0;
            for (let capture in expected) {
                assert.equal(found.captures[index], capture);
                index += 1;
            }
            null
        },
    }
}

function assert_capture_text_prefix(actual: EreMatch?, expected: string?[], sparse: bool) -> null {
    match (actual) {
        null => baml.sys.panic("expected a match"),
        let found: EreMatch => {
            assert.is_true(found.captures.length() >= expected.length());
            let index = 0;
            for (let capture in expected) {
                if (!sparse || capture != null) {
                    let actual_capture = found.captures[index];
                    assert.equal(
                        match (actual_capture) {
                            null => null,
                            let value: Capture => value.text,
                        },
                        capture,
                    );
                }
                index += 1;
            }
            null
        },
    }
}

function assert_parse_error(pattern: string) -> null {
    let accepted = pattern_compiles(pattern) catch (e) {
        baml.errors.ParseError => false
    };
    assert.equal(accepted, false)
}

function assert_corpus_case(c: CorpusCase) -> null {
    let expected = required_expectation(c);
    let input = corpus_input(c);
    match (expected.kind) {
        "exact_match" => {
            assert.equal(
                exec_ere(c.pattern, input),
                EreMatch { captures: required_captures(expected) },
            )
        },
        "exact_prefix" => {
            assert_exact_prefix(exec_ere(c.pattern, input), required_captures(expected))
        },
        "capture_texts" => {
            assert_capture_texts(exec_ere(c.pattern, input), required_capture_texts(expected))
        },
        "capture_text_prefix" => {
            assert_capture_text_prefix(exec_ere(c.pattern, input), required_capture_texts(expected), false)
        },
        "capture_texts_sparse" => {
            assert_capture_text_prefix(exec_ere(c.pattern, input), required_capture_texts(expected), true)
        },
        "match" => assert.is_true(exec_ere(c.pattern, input) != null),
        "no_match" => assert.equal(exec_ere(c.pattern, input), null),
        "compile_ok" => {
            let _ = exec_ere(c.pattern, input);
            null
        },
        "parse_error" => assert_parse_error(c.pattern),
        _ => baml.sys.panic("unknown corpus expectation kind: " + expected.kind),
    }
}

testset "ere_corpus" {
    for (let c in runnable_corpus_cases()) {
        test c.id {
            assert_corpus_case(c)
        }
    }
}
