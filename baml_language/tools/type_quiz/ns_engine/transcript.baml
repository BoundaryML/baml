// A sitting written down: what was asked, what was answered, and how the
// answer was judged.
//
// Transcripts are values and go to disk as JSON, so a sitting can be replayed
// question for question, reviewed after the fact, and re-verified against the
// compiler long after the run that produced it. A transcript carries the
// cases themselves, not references into the bank, so it stays readable when
// the bank moves on.

/// What a learner is asked to do with a case.
enum Mode {
    /// Predict the verdict alone.
    Simple,
    /// Predict the verdict and say what the case turns on.
    Full,
}

function mode_name(mode: Mode) -> string throws never {
    match (mode) {
        Mode.Simple => "simple",
        Mode.Full => "full",
    }
}

/// A case as it is put to a learner: the program, and where it came from.
///
/// The key and the derivation are deliberately absent. A sitting is a pure
/// function of its seed, so the answer can be worked out again from `item`
/// and `seed` at the moment it is needed, and until the learner has answered
/// there is nothing in front of them to read it off.
class Prompt {
    item: string,
    /// The stream state the case was drawn from, as a `bigint`: a state uses
    /// 63 bits, and an `int` reaches a page through a JavaScript number,
    /// which keeps 53. A `bigint` crosses whole, so the state a page hands
    /// back is the state it was given.
    seed: bigint,
    /// The program to judge, or the first of two to choose between.
    program: map<string, string>,
    /// The second program, present exactly when the question is which of the
    /// two the compiler accepts. What a learner is asked therefore follows
    /// from what they are shown, and the two cannot disagree.
    against: map<string, string>?,
}

/// A case as it was put to a learner, with where it came from so the same
/// case can be generated again and checked against the compiler again.
class Question {
    item: string,
    /// The stream state the case was drawn from. `Seed { state }` regenerates
    /// it exactly.
    seed: int,
    /// What was shown, and thereby what was asked.
    posed: Posed,
}

/// What a learner is shown, from a question they have not answered yet.
function prompt_of(question: Question) -> Prompt throws never {
    let item = question.item;
    let seed = question.seed * 1n;
    match (question.posed) {
        let case: Case => Prompt { item, seed, program: case.files, against: null },
        let choice: Choice => {
            Prompt { item, seed, program: choice.first().files, against: choice.second().files }
        },
    }
}

/// Which of the two programs shown the learner said the compiler accepts.
class Chose {
    /// The second of the two, rather than the first.
    second: bool,
}

/// What a learner committed to: a verdict on the one program they were
/// shown, or a choice between the two they were.
type Said = VerdictKind | Chose;

/// What the learner said. `said` is absent when they said they were not
/// sure, which is an answer in its own right: it is what makes a committed
/// answer worth believing. `reasoning` is empty in simple mode, which asks for
/// none.
class Answer {
    said: Said?,
    reasoning: string,
}

/// How well a learner's reasoning matched what the case turns on.
enum Understanding {
    Sound,
    Partial,
    Wrong,
}

class Reasoning {
    understanding: Understanding,
    /// The model that marked the reasoning, or absent when the learner marked
    /// their own against the derivation they were shown.
    ///
    /// `SelfMarked | ModelMarked` would say this better, but a transcript has
    /// to survive JSON, and an empty class in a union matches any object: a
    /// sibling decodes as the empty member whenever it is listed first, with
    /// no error. Ordering the union to dodge that would make correctness turn
    /// on the order of a type alias. See the README's issue list.
    marked_by: string?,
}

/// A judged answer.
///
/// The verdict is right or wrong by the case's key and nothing else, so that
/// half is never a matter of opinion; it is absent when the learner gave no
/// verdict, since abstaining is neither right nor wrong. The reasoning is
/// judged separately and is absent when there was none to judge.
class Judgement {
    verdict_correct: bool?,
    reasoning: Reasoning?,
}

/// One question, the answer to it, and how the answer was judged.
class Exchange {
    question: Question,
    answer: Answer,
    judgement: Judgement,
}

/// What an adaptive sitting concluded: the knobs it ran under and the
/// profile it ended with.
class Learned {
    knobs: Knobs,
    profile: Profile,
}

/// A whole sitting.
class Transcript {
    mode: Mode,
    /// The stream state the sitting was drawn from. For a sitting planned by
    /// seed alone, the same state asks the same questions in the same order;
    /// for an adaptive one, the same state and the same answers do.
    seed: int,
    exchanges: Exchange[],
    /// What the model concluded about the learner. Absent from a sitting
    /// planned by seed alone, which has no model of its learner.
    learner: Learned?,
}

function understanding_word(understanding: Understanding) -> string throws never {
    match (understanding) {
        Understanding.Sound => "sound",
        Understanding.Partial => "partial",
        Understanding.Wrong => "wrong",
    }
}

function marker_word(reasoning: Reasoning) -> string throws never {
    reasoning.marked_by ?? "the learner"
}

/// Judge `answer` against `question`. A function of the two and of nothing
/// else: whether the answer is right is decided by the key, never by what
/// the learner wrote.
function judge(
    question: Question,
    answer: Answer,
    reasoning: Reasoning?,
) -> Judgement throws never {
    let verdict_correct = match (answer.said) {
        let verdict: VerdictKind => matches_verdict(question.posed, verdict),
        let chose: Chose => matches_choice(question.posed, chose),
        null => null,
    };
    Judgement { verdict_correct, reasoning }
}

/// # Panics
///
/// - `baml.panics.Unreachable` if the question showed two programs. What a
///   learner may say follows from what they were shown, so an answer of the
///   other kind is a defect in whatever asked them rather than something to
///   absorb.
function matches_verdict(posed: Posed, verdict: VerdictKind) -> bool throws never {
    match (posed) {
        let case: Case => verdict == verdict_kind(case.key),
        Choice => {
            throw baml.panics.Unreachable { message: "a verdict was given for a question that showed two programs" }
        },
    }
}

/// # Panics
///
/// - `baml.panics.Unreachable` if the question showed one program, as
///   `matches_verdict`.
function matches_choice(posed: Posed, chose: Chose) -> bool throws never {
    match (posed) {
        Case => {
            throw baml.panics.Unreachable { message: "a choice was given for a question that showed one program" }
        },
        let choice: Choice => chose.second == choice.second_compiles(),
    }
}

// ─── reading a sitting back ──────────────────────────────────────────────────

class Score {
    asked: int,
    /// Answers the learner committed to; the rest abstained.
    committed: int,
    answers_right: int,
    sound: int,
    partial: int,
    wrong: int,
    /// Answers whose reasoning nobody judged: every answer in simple mode,
    /// and any in full mode the learner declined to mark.
    unmarked: int,
}

function marked(transcript: Transcript) -> Understanding[] throws never {
    transcript.exchanges.filter_map((exchange) -> {
        exchange.judgement.reasoning?.understanding
    })
}

function how_many(understandings: Understanding[], of: Understanding) -> int throws never {
    understandings
        .filter((u) -> {
            u == of
        })
        .length()
}

function tally(transcript: Transcript) -> Score throws never {
    let understandings = marked(transcript);
    Score {
        asked: transcript.exchanges.length(),
        committed: transcript.exchanges
            .filter((exchange) -> {
                exchange.judgement.verdict_correct != null
            })
            .length(),
        answers_right: transcript.exchanges
            .filter((exchange) -> {
                exchange.judgement.verdict_correct == true
            })
            .length(),
        sound: how_many(understandings, Understanding.Sound),
        partial: how_many(understandings, Understanding.Partial),
        wrong: how_many(understandings, Understanding.Wrong),
        unmarked: transcript.exchanges.length() - understandings.length(),
    }
}

// ─── disk ────────────────────────────────────────────────────────────────────

class Saved {
    path: string,
}

class SaveFailed {
    path: string,
    message: string,
}

type SaveResult = Saved | SaveFailed;

class Loaded {
    transcript: Transcript,
}

class LoadFailed {
    path: string,
    message: string,
}

type LoadResult = Loaded | LoadFailed;

function exists(path: string) -> bool throws never {
    baml.fs.exists(path) catch (e) {
        baml.errors.Io => false,
    }
}

/// Where a sitting is written: named for the mode and the stream it was drawn
/// from, with a counter, so running the same sitting again never overwrites
/// the one it would be compared against.
function transcript_path(dir: string, mode: Mode, seed: int) -> string throws never {
    let stem = `${dir}/${mode_name(mode)}-${seed}`;
    baml.iter.Range.new(0, 1000)
        .map((n) -> {
            `${stem}-${n}.json`
        })
        .find((path) -> {
            !exists(path)
        })
        ?? `${stem}-full.json`
}

function save(transcript: Transcript, dir: string) -> SaveResult throws never {
    let path = transcript_path(dir, transcript.mode, transcript.seed);
    let written: string | SaveFailed = baml.json.to_string(transcript) catch_all (e) {
        _ => SaveFailed { path, message: `${e}` },
    };
    match (written) {
        let failed: SaveFailed => failed,
        let text: string => {
            let bytes: int | SaveFailed = baml.fs.write(path, `${text}\n`) catch_all (e) {
                _ => SaveFailed { path, message: `${e}` },
            };
            match (bytes) {
                let failed: SaveFailed => failed,
                int => Saved { path },
            }
        },
    }
}

/// `text` as a transcript, or why it is not one. Held apart from `load` so
/// the catch binding fits on one line, which the parser needs.
function decode(path: string, text: string) -> Transcript | LoadFailed throws never {
    baml.json.from_string<Transcript>(text) catch_all (e) {
        _ => LoadFailed { path, message: `${e}` },
    }
}

function load(path: string) -> LoadResult throws never {
    let read: string | LoadFailed = baml.fs.read(path) catch_all (e) {
        _ => LoadFailed { path, message: `${e}` },
    };
    match (read) {
        let failed: LoadFailed => failed,
        let text: string => {
            let parsed: Transcript | LoadFailed = decode(path, text);
            match (parsed) {
                let failed: LoadFailed => failed,
                let transcript: Transcript => Loaded { transcript },
            }
        },
    }
}
