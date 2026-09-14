// Conformance suite for the surface a user interface calls: a sitting is a
// pure function of its seed, a prompt carries no answer, an answer is marked
// against the key, what the compiler is quoted as saying matches what the key
// claims, and a finished sitting survives the trip to JSON and back.

function seed_used() -> int throws never {
    11
}

function length_used() -> int throws never {
    8
}

/// The one program a question puts. A sitting planned from a seed alone has
/// no learner to put choices to, so every question of one is a single
/// program, and this says so rather than picking one of two.
///
/// # Panics
///
/// - `baml.panics.Unreachable` if the question put two programs.
function only(question: root.engine.Question) -> root.engine.Case throws never {
    match (question.posed) {
        let case: root.engine.Case => case,
        root.engine.Choice => {
            throw baml.panics.Unreachable { message: "a sitting planned from a seed alone puts one program at a time" }
        },
    }
}

function planned() -> root.engine.Question[] throws never {
    baml.iter.Range.new(0, length_used())
        .filter_map((i) -> {
            root.question_at(seed_used(), length_used(), i)
        })
        .collect()
}

// ─── planning a sitting ──────────────────────────────────────────────────────

/// A sitting has the length it was asked for, and asking again asks the same
/// questions in the same order.
function planning_report() -> string throws never {
    let first = planned();
    let again = planned();
    let counted = root.sitting_length(seed_used(), length_used());
    let past_end = root.question_at(seed_used(), length_used(), length_used()) == null
        && root.prompt_at(seed_used(), length_used(), length_used()) == null;
    `count=${first.length()} reported=${counted} stable=${first == again} past_end=${past_end}`
}

/// Only items the compiler is known to agree with are put to a learner.
function unverified_items_asked() -> string[] throws never {
    let verified = root.bank.items().filter_map((item) -> {
        if (item.status() is root.engine.Verified) {
            item.id()
        } else {
            null
        }
    });
    planned().filter_map((question) -> {
        if (verified.includes(question.item)) {
            null
        } else {
            question.item
        }
    })
}

/// What a learner is shown must be what the compiler says.
function unverified_questions() -> string[] throws never {
    planned().filter_map((question) -> {
        let outcome = root.engine.verify(only(question));
        if (root.engine.agrees(outcome)) {
            null
        } else {
            `${question.item}: ${outcome}`
        }
    })
}

/// A question records where its case came from, so the case can be built
/// again from the bank rather than only read back from a transcript.
function unreproducible_questions() -> string[] throws never {
    let items = root.bank.items();
    planned().filter_map((question) -> {
        let item = items.find((candidate) -> {
            candidate.id() == question.item
        })
            ?? return `${question.item}: no such item`;
        let regenerated = root.engine.generate(item, root.engine.Seed { state: question.seed })
            ?? return `${question.item}: seed ${question.seed} generates nothing`;
        if (root.engine.cases_of(question.posed) == [regenerated.case]) {
            null
        } else {
            `${question.item}: seed ${question.seed} regenerates a different case`
        }
    })
}

test "sitting: the same seed plans the same questions, and the end is the end" {
    assert.equal(planning_report(), "count=8 reported=8 stable=true past_end=true")
}

test "sitting: only verified items are asked" {
    assert.equal(unverified_items_asked().join("\n"), "")
}

test "sitting: every case a sitting asks agrees with the compiler" {
    assert.equal(unverified_questions().join("\n"), "")
}

test "sitting: every question can be generated again from its item and seed" {
    assert.equal(unreproducible_questions().join("\n"), "")
}

// ─── a prompt gives nothing away ─────────────────────────────────────────────

/// A prompt is the program and where it came from, and carries neither the
/// key nor the derivation: a learner reading it has the same information as a
/// learner reading the screen.
function prompts_are_bare() -> string throws never {
    let questions = planned();
    let matching = baml.iter.Range.new(0, questions.length()).every((i) -> {
        match (root.prompt_at(seed_used(), length_used(), i)) {
            let prompt: root.engine.Prompt => {
                prompt.program == only(questions[i]).files
                    && prompt.against == null
                    && prompt.item == questions[i].item
                    && prompt.seed == questions[i].seed * 1n
            },
            null => false,
        }
    });
    let fields = reflect.Type.of<root.engine.Prompt>().as_class()?.fields().map((field) -> {
        field.name
    })
        ?? [];
    `matching=${matching} fields=${fields}`
}

test "prompt: the program and where it came from, and nothing else" {
    assert.equal(prompts_are_bare(), `matching=true fields=["item", "seed", "program", "against"]`)
}

// ─── a driven sitting gives nothing away either ──────────────────────────────

/// What a driver of a sitting is handed before an answer, and what it is
/// handed after.
///
/// `Asked` is the half a learner may see beforehand, so it must carry the
/// programs and nothing else — not the item, not the seed, not the key, and
/// not what separates two programs of a pair, which the page says only in the
/// reveal. `Marked` is the half that may be shown afterwards. A field added
/// to the wrong one of them is how a driver would start asking an easier
/// question than the page asks, so both are read back through reflection.
function driven_halves() -> string throws never {
    let asked = reflect.Type.of<root.Asked>().as_class()?.fields().map((field) -> {
        field.name
    })
        ?? [];
    let marked = reflect.Type.of<root.Marked>().as_class()?.fields().map((field) -> {
        field.name
    })
        ?? [];
    `asked=${asked} marked=${marked}`
}

/// The first step of a driven sitting is the first question of that sitting
/// and nothing has been marked yet; the second carries the marking of the
/// first answer. Driving is a pure function of the session and the answers.
function parsed_step(printed: string) -> root.Step? throws never {
    baml.json.from_string<root.Step>(printed) catch_all (e) {
        baml.json.DecodeError => null,
        baml.json.ParseError => null,
    }
}

function driven_first_steps() -> string throws never {
    let opening = parsed_step(root.adaptive_step(11, ""));
    let after = parsed_step(root.adaptive_step(11, "compiles"));
    let first = root.next_prompt(root.fresh_profile(), root.engine.default_knobs(), 11, 0);
    let same = match (first) {
        let next: root.Next => {
            match (opening?.asked) {
                let asked: root.Asked => {
                    asked.programs == [next.prompt.program.values().join("\n")]
                },
                null => false,
            }
        },
        null => false,
    };
    `same_first=${same} opening_marked=${opening?.marked == null} second_marked=${after?.marked != null}`
}

test "driven: the half a learner sees before answering carries only programs" {
    assert.equal(
        driven_halves(),
        `asked=["programs", "teach"] marked=["said", "correct", "points", "compiles", "reported", "because", "difference"]`,
    )
}

test "driven: a step is the sitting's own question, and marks only what was answered" {
    assert.equal(driven_first_steps(), "same_first=true opening_marked=true second_marked=true")
}

// ─── answering ───────────────────────────────────────────────────────────────

function compiles_by_key(question: root.engine.Question) -> bool throws never {
    root.engine.verdict_kind(only(question).key) == root.engine.VerdictKind.Compiles
}

function said(compiles: bool, reasoning: string, mark: string) -> root.Given throws never {
    root.Given {
        said: if (compiles) {
            "compiles"
        } else {
            "rejected"
        },
        reasoning,
        mark,
    }
}

function unsure() -> root.Given throws never {
    root.Given { said: "unsure", reasoning: "", mark: "" }
}

/// Answering with the key's verdict is right and answering with the other is
/// wrong, at every question of a sitting and in both directions.
function answering_report() -> string throws never {
    let questions = planned();
    let indices = baml.iter.Range.new(0, questions.length()).collect();
    let right = indices.every((i) -> {
        match (root.answer_at(seed_used(), length_used(), i, said(compiles_by_key(questions[i]), "", ""))) {
            let exchange: root.engine.Exchange => {
                exchange.judgement.verdict_correct == true && exchange.question == questions[i]
            },
            null => false,
        }
    });
    let wrong = indices.every((i) -> {
        match (
            root.answer_at(
                seed_used(),
                length_used(),
                i,
                said(!compiles_by_key(questions[i]), "", ""),
            )
        ) {
            let exchange: root.engine.Exchange => exchange.judgement.verdict_correct == false,
            null => false,
        }
    });
    let past_end = root.answer_at(seed_used(), length_used(), length_used(), said(true, "", "")) == null;
    let abstained = match (root.answer_at(seed_used(), length_used(), 0, unsure())) {
        let exchange: root.engine.Exchange => {
            exchange.judgement.verdict_correct == null && exchange.answer.said == null
        },
        null => false,
    };
    `right=${right} wrong=${wrong} past_end=${past_end} abstained=${abstained}`
}

/// Every mark the surface offers becomes the engine's own word for it, and an
/// empty mark leaves the reasoning unjudged.
function marking_report() -> string throws never {
    let marks = ["sound", "partial", "wrong", ""];
    let understandings = marks.map((mark) -> {
        match (root.answer_at(seed_used(), length_used(), 0, said(true, "because", mark))) {
            let exchange: root.engine.Exchange => {
                match (exchange.judgement.reasoning) {
                    let reasoning: root.engine.Reasoning => {
                        root.engine.understanding_word(reasoning.understanding)
                    },
                    null => "unmarked",
                }
            },
            null => "no exchange",
        }
    });
    let carried = match (root.answer_at(seed_used(), length_used(), 0, said(true, "because", "sound"))) {
        let exchange: root.engine.Exchange => exchange.answer.reasoning == "because",
        null => false,
    };
    `marks=${understandings} carried=${carried}`
}

test "answer: the key decides, at every question and in both directions" {
    assert.equal(answering_report(), "right=true wrong=true past_end=true abstained=true")
}

test "answer: every mark the surface offers is understood, and none means none" {
    assert.equal(marking_report(), `marks=["sound", "partial", "wrong", "unmarked"] carried=true`)
}

// ─── quoting the compiler ────────────────────────────────────────────────────

/// What a learner is shown the compiler saying must be what the key claims:
/// the same codes, and the same answer to whether it compiled at all.
function misquoted_compiler() -> string[] throws never {
    let questions = planned();
    baml.iter.Range.new(0, questions.length())
        .filter_map((i) -> {
            let reported = root.compiler_report(seed_used(), length_used(), i)
                ?? return `${i}: nothing reported`;
            let question = questions[i];
            match (only(question).key) {
                let rejected: root.engine.Rejected => {
                    if (reported.compiles) {
                        `${question.item}: keyed rejected, compiler accepted it`
                    } else if (sorted(reported.codes) != sorted(rejected.codes())) {
                        `${question.item}: keyed ${rejected.codes()}, compiler said ${reported.codes}`
                    } else if (reported.messages.length() != reported.codes.length()) {
                        `${question.item}: ${reported.codes.length()} codes but ${reported.messages.length()} messages`
                    } else {
                        null
                    }
                },
                root.engine.Compiles => {
                    if (reported.compiles && reported.codes.length() == 0) {
                        null
                    } else {
                        `${question.item}: keyed compiling, compiler said ${reported.codes}`
                    }
                },
            }
        })
        .collect()
}

function sorted(values: string[]) -> string[] throws never {
    values.slice(0, values.length()).sort_by_key((v) -> {
        v
    })
}

test "report: the compiler's own words match the key" {
    assert.equal(misquoted_compiler().join("\n"), "")
}

// ─── judging, on its own ─────────────────────────────────────────────────────

function question_with(key: root.engine.Verdict) -> root.engine.Question throws never {
    root.engine.Question {
        item: "synthetic",
        seed: 1,
        posed: root.engine.Case { files: root.engine.one_file("case.baml", ""), key, trace: [] },
    }
}

function answered(verdict: root.engine.VerdictKind) -> root.engine.Answer throws never {
    root.engine.Answer { said: verdict, reasoning: "" }
}

function judging_is_the_key() -> bool throws never {
    let compiles = question_with(root.engine.Compiles { tests: [] });
    let rejected = question_with(
        root.engine.Rejected {
            code: "E0001",
            more: [],
            at: root.engine.Region { file: "case.baml", start: 0, end: 1 },
        },
    );
    let said_compiles = answered(root.engine.VerdictKind.Compiles);
    let said_rejected = answered(root.engine.VerdictKind.Rejected);
    let no_verdict = root.engine.Answer { said: null, reasoning: "" };
    root.engine.judge(compiles, said_compiles, null).verdict_correct == true
        && root.engine.judge(compiles, said_rejected, null).verdict_correct == false
        && root.engine.judge(rejected, said_compiles, null).verdict_correct == false
        && root.engine.judge(rejected, said_rejected, null).verdict_correct == true
        && root.engine.judge(compiles, no_verdict, null).verdict_correct == null
}

test "judge: the verdict is right exactly when it matches the key, and absent when none was given" {
    assert.is_true(judging_is_the_key())
}

// ─── the sitting a learner takes away ────────────────────────────────────────

/// A transcript holding every shape one can hold: both verdicts, a named
/// marker and a self-mark, every mark, and an unmarked answer.
function every_shape() -> root.engine.Transcript throws never {
    let question = question_with(
        root.engine.Rejected {
            code: "E0068",
            more: ["E0068"],
            at: root.engine.Region { file: "case.baml", start: 3, end: 9 },
        },
    );
    let marks: root.engine.Reasoning?[] = [
        root.engine.Reasoning { understanding: root.engine.Understanding.Sound, marked_by: null },
        root.engine.Reasoning {
            understanding: root.engine.Understanding.Partial,
            marked_by: "a-model",
        },
        root.engine.Reasoning { understanding: root.engine.Understanding.Wrong, marked_by: null },
        null,
    ];
    root.engine.Transcript {
        mode: root.engine.Mode.Full,
        seed: 7,
        exchanges: marks.map((mark) -> {
            let answer = root.engine.Answer {
                said: root.engine.VerdictKind.Rejected,
                reasoning: "the classes are distinct",
            };
            root.engine.Exchange {
                question,
                answer,
                judgement: root.engine.judge(question, answer, mark),
            }
        }),
        learner: null,
    }
}

/// A transcript written before sittings carried a learner model reads back
/// with none, rather than failing to read at all.
function reads_without_learner() -> string throws never {
    let text = `{"mode":"Simple","seed":1,"exchanges":[]}`;
    match (root.engine.decode("older.json", text)) {
        let failed: root.engine.LoadFailed => `failed: ${failed.message}`,
        let transcript: root.engine.Transcript => `learner_absent=${transcript.learner == null}`,
    }
}

test "transcript: one written without a learner model still reads back" {
    assert.equal(reads_without_learner(), "learner_absent=true")
}

/// One plain answer for each question of the fixed sitting.
function answers_given() -> root.Given[] throws never {
    let marks = ["sound", "partial", "wrong", ""];
    baml.iter.Range.new(0, length_used())
        .map((index) -> {
            if (index % 3 == 2) {
                unsure()
            } else {
                said(index % 2 == 0, `reason ${index}`, marks[index % marks.length()])
            }
        })
        .collect()
}

/// The JSON a learner downloads is exactly what `review` reads back, and the
/// sitting it describes is the one the engine would assemble itself.
function download_round_trip() -> string throws never {
    let expected = root.sitting_of(seed_used(), length_used(), true, answers_given());
    let text = root.sitting_json(seed_used(), length_used(), true, answers_given());
    let back = baml.json.from_string<root.engine.Transcript>(text) catch_all (e) {
        baml.json.ParseError => {
            return "parsing threw";
        },
        baml.json.DecodeError => {
            return `decoding threw on ${text}`;
        },
    };
    if (back == expected) {
        "identical"
    } else {
        "differs"
    }
}

/// The score the interface shows is the engine's own count of the sitting.
function score_matches_the_sitting() -> bool throws never {
    root.sitting_score(seed_used(), length_used(), true, answers_given())
        == root.engine.tally(root.sitting_of(seed_used(), length_used(), true, answers_given()))
}

test "download: the JSON a learner takes away is what review reads back" {
    assert.equal(download_round_trip(), "identical")
}

test "download: the score shown is the engine's count of the same sitting" {
    assert.is_true(score_matches_the_sitting())
}

/// A scratch directory of this process's and this test's own: tests in one
/// process run concurrently, so two sharing a directory would each remove
/// the other's files mid-flight.
function scratch(tag: string) -> string throws never {
    `.type-quiz-scratch-${baml.sys.pid()}-${tag}`
}

function clean_up(dir: string) -> void throws never {
    baml.fs.remove_dir_all(dir) catch (e) {
        baml.errors.Io => {},
    };
}

/// A downloaded sitting survives the trip to disk, and comes back unchanged.
function disk_round_trip() -> string throws never {
    let dir = scratch("disk");
    clean_up(dir);
    let one = every_shape();
    let report = match (root.engine.save(one, dir)) {
        let failed: root.engine.SaveFailed => `saving failed: ${failed.message}`,
        let saved: root.engine.Saved => {
            match (root.engine.load(saved.path)) {
                let load_failed: root.engine.LoadFailed => {
                    `loading failed: ${load_failed.message}`
                },
                let loaded: root.engine.Loaded => `same=${loaded.transcript == one}`,
            }
        },
    };
    clean_up(dir);
    report
}

/// `review` reads a sitting a learner really sat, and reports it standing.
function review_round_trip() -> string throws never {
    let dir = scratch("review");
    clean_up(dir);
    let transcript = root.sitting_of(seed_used(), length_used(), false, answers_given());
    let report = match (root.engine.save(transcript, dir)) {
        let failed: root.engine.SaveFailed => `saving failed: ${failed.message}`,
        let saved: root.engine.Saved => root.review(saved.path),
    };
    clean_up(dir);
    report
}

function missing_file_is_reported() -> bool throws never {
    root.review(`${scratch("missing")}/not-here.json`).starts_with("could not read")
}

test "download: a sitting survives the trip to disk" {
    assert.equal(disk_round_trip(), "same=true")
}

test "review: a sat sitting reads back with its cases unchanged" {
    assert.is_true(review_round_trip().includes("every case still behaves as recorded"))
}

test "review: a missing file is reported, not thrown" {
    assert.is_true(missing_file_is_reported())
}

// ─── counting a sitting ──────────────────────────────────────────────────────

function tally_report() -> string throws never {
    let score = root.engine.tally(every_shape());
    `asked=${score.asked} committed=${score.committed} right=${score.answers_right} sound=${score.sound} partial=${score.partial} wrong=${score.wrong} unmarked=${score.unmarked}`
}

test "transcript: a sitting counts up" {
    assert.equal(tally_report(), "asked=4 committed=4 right=4 sound=1 partial=1 wrong=1 unmarked=1")
}
