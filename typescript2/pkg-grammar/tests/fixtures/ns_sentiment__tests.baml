// A guided tour of testing in BAML, using the sentiment classifier next door.
//
// Run everything in this namespace's suites:
//   baml test -i "basics::*" -i "by_label::*" -i "judged::*" \
//             -i "synthetic::*" -i "from_file::*" -i "from_api::*"
//
// Or one step at a time:
//   baml test --list                 # see every case
//   baml test -i "basics::*"         # just step 1
//
// Most tests here call a live LLM on purpose (this namespace is an eval
// suite). The ones that don't are marked "no tokens".

// ---------------------------------------------------------------------------
// 1. Start simple: one test, one call, one expectation.
// ---------------------------------------------------------------------------
testset "basics" {
    test "obviously positive" {
        let v = classify("I absolutely love this — best purchase all year!");
        assert.equal(v.label, "positive");
    }

    // You don't need a live model to test parsing and downstream logic:
    // freeze a model reply once, decode it with the same machinery the
    // runtime uses, and assert on the typed result. (no tokens)
    test "frozen reply decodes" {
        let raw = #"{"label": "negative", "confidence": 0.93}"#;
        let v = baml.json.from_string<Verdict>(raw);
        assert.equal(v.label, "negative");
        assert.is_true(v.confidence > 0.5);
    }
}

// ---------------------------------------------------------------------------
// 2. Testsets: group cases, and generate the groups with a loop.
//    Nested IDs look like "by_label::neutral/classifies its example" —
//    run one group with:  baml test -i "by_label::neutral*"
// ---------------------------------------------------------------------------

// A pure helper the loop below draws from.
function example_for(label: Label) -> string {
    match (label) {
        "positive" => "The keyboard feels fantastic and setup took two minutes.",
        "negative" => "It died a week after the warranty ran out. Never again.",
        "neutral" => "The manual is 14 pages long and printed in four languages.",
    }
}

testset "by_label" {
    let labels: Label[] = ["positive", "negative", "neutral"];
    for (let label in labels) {
        test "classifies its example: " + label {
            let v = classify(example_for(label));
            assert.equal(v.label, label);
        }
    }
}

// ---------------------------------------------------------------------------
// 3. LLM as judge: when the right answer is fuzzy, a second (stronger) model
//    grades the first. Use sparingly — the judge costs tokens and has noise
//    of its own. Everywhere the expected answer is known, prefer a direct
//    expectation like the testsets above.
// ---------------------------------------------------------------------------
testset "judged" {
    test "sarcasm gets a defensible label" {
        let text = "Oh great, the screen cracked on day two. Money well spent.";
        let v = classify(text);
        let j = judge(text, v.label);
        assert.is_true(j.reasonable);
        assert.is_true(j.why.length() > 0);
    }
}

// ---------------------------------------------------------------------------
// 4. Synthetic tests: have a model invent scenarios whose answer is known by
//    construction ("write clearly-negative reviews"), then check the
//    classifier recovers that answer. Because generated text can drift into
//    genuinely ambiguous territory, a mismatch gets a second opinion from
//    the judge instead of failing outright.
// ---------------------------------------------------------------------------
testset "synthetic" {
    let labels: Label[] = ["positive", "negative"];
    for (let label in labels) {
        testset label {
            test "generated examples classify back" {
                let examples = generate_examples(label, 2);
                assert.equal(examples.length(), 2);
                for (let text in examples) {
                    let v = classify(text);
                    let ok = v.label == label;
                    if (!ok) {
                        // lenient grading: accept if a judge finds the
                        // classifier's label defensible for this text.
                        let j = judge(text, v.label);
                        ok = j.reasonable;
                    };
                    assert.is_true(ok);
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// 5. Tests from a file: testdata/sentiment_cases.json is plain JSON — edit
//    it, add cases, re-run. The loop runs at discovery time, so every case
//    in the file becomes its OWN testset, named by its (slugged) text —
//    failures point straight at the offending case, and you can -i filter
//    down to a single one:  baml test -i "from_file::1*"
// ---------------------------------------------------------------------------
testset "from_file" {
    let names = case_names();
    for (let name in names) {
        testset name {
            test "classifies" {
                let c = case_for(name);
                let v = classify(c.text);
                assert.equal(v.label, c.expected);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// 6. Tests from an API: same cases, delivered over HTTP. The test spawns a
//    one-shot server (which reads the same file), fetches the cases back
//    through baml.http, and verifies the transport plus one classification.
// ---------------------------------------------------------------------------
testset "from_api" {
    test "cases served over http" {
        let fut = spawn {
            serve_cases_once("127.0.0.1:8091")
        };
        baml.sys.sleep(150); // let the listener come up

        let fetched = fetch_cases("http://127.0.0.1:8091/cases");
        let from_disk = load_cases(cases_path());

        // the API delivers exactly what's in the file...
        assert.equal(fetched.length(), from_disk.length());
        assert.equal(fetched[0].text, from_disk[0].text);

        // ...and the cases work end to end (first one only — from_file
        // already runs them all).
        let v = classify(fetched[0].text);
        assert.equal(v.label, fetched[0].expected);

        let _ = (await fut) catch (e) {
            _ => null,
        };
    }
}
