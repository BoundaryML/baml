// The grader: a model marks a learner's reasoning against the case's own
// explanation.
//
// It is the one place the quiz asks a model anything, and it asks with the
// learner's own key: the page holds the key in the browser and hands it in
// per call, and it goes into exactly one request, to the provider, with the
// header that provider documents for a call made from a browser. Nothing here
// reads an environment, and nothing of ours ever holds the key.
//
// The rubric is the case's claims — the rules instantiated at its types —
// so the grader cites the specification without having read it, and its
// verdict is about whether the learner's reasoning reaches the answer by
// those rules, not about whether the answer was right: the key decided that
// already, and a right answer for a wrong reason is what this is for.

/// What the grader makes of a piece of reasoning.
class Grade {
    understanding: Understanding,
    /// A short paragraph to the learner: what they had right, and what the
    /// explanation says that they did not.
    feedback: string,
    /// The claims their reasoning did not reach, quoted from the rubric;
    /// empty when it reached them all.
    missed: string[],
}

/// Why a judgement could not be made, in the words of whatever refused it.
///
/// A judgement is asked for from a browser, where the only thing to be done
/// with a failure is to show it and offer to ask again — so a failure is a
/// VALUE here rather than a throw, and a caller that can see one in the type
/// cannot forget to handle it. The words are the failure's own rendering,
/// fields and all: the provider says which of an unusable key, an unreachable
/// model and an exhausted quota it was, and paraphrasing that would be
/// throwing away the only thing the learner can act on.
class Unjudged {
    why: string,
}

/// What asking for a judgement comes back as.
type Judged = Grade | Unjudged;

/// The client a judgement is made with: the model named, the learner's own
/// key, and the header Anthropic requires of a request made from a browser.
function judge_client(key: string, model: string) -> ai.Client throws never {
    anthropic.Client.new(
        model = model,
        api_key = key,
        headers = { "anthropic-dangerous-direct-browser-access": "true" },
    )
}

/// The learner's answer as the sentence a grader is told.
///
/// What they claimed, rather than the word they clicked: "first" says nothing
/// to a model about which program the learner thinks compiles. The word was
/// read into a value by `said_of`, so this and the rubric cannot disagree
/// about which program the answer is about.
function answer_sentence(said: Said?) -> string throws never {
    match (said) {
        null => "they were not sure",
        VerdictKind.Compiles => "the compiler accepts it",
        VerdictKind.Rejected => "the compiler rejects it",
        let chose: Chose => {
            if (chose.second) {
                "the compiler accepts the second program and rejects the first"
            } else {
                "the compiler accepts the first program and rejects the second"
            }
        },
    }
}

/// What the learner was asked to explain, as the grader is told it.
///
/// Derived from the answer, like the rubric is, so the grader cannot be told
/// one question while the learner was asked another. The scope note travels
/// with the question it belongs to: only a learner choosing between two
/// programs has another program they were not asked about.
function asked_sentence(said: Said?) -> string throws never {
    match (said) {
        null => "nothing: they held back, and a reason was not asked of them",
        VerdictKind.Compiles => "why the compiler accepts it",
        VerdictKind.Rejected => "why the compiler rejects it",
        Chose => {
            "why the program they say the compiler rejects fails. They were not asked to account for the other one, and reasoning that leaves it unmentioned is not incomplete for that"
        },
    }
}

/// Mark `reasoning` against the rubric. The client named here is a
/// placeholder the compiler requires; every call overrides it with the
/// learner's. The programs are delimited by tags rather than a code fence: a
/// prompt is a template, and a fence's backticks would end it.
function grade(
    programs: string[],
    compiles: bool[],
    claims: string[],
    said: string,
    asked: string,
    reasoning: string,
) -> Grade {
    client: "anthropic/claude-sonnet-4-5"
    prompt: `
        You are marking a learner's reasoning about the BAML type system.

        They were shown ${programs.length()} program(s) and asked what the
        compiler makes of them. The compiler's answer is known and is not in
        question; you are marking whether their REASONING reaches it by the
        rules the case actually turns on.

        Mark it Sound if it applies the rules of the rubric correctly to reach
        the answer; Partial if it reaches the right answer with a gap or an
        error the rules would have caught; Wrong if it rests on a rule the
        type system does not have, or contradicts one it does. A right answer
        for a wrong reason is Wrong. Quote in the missed list every rubric
        line their reasoning did not reach. In the feedback, tell them in a
        short paragraph what they had right and what the rubric says that
        they did not: speak to the learner, cite the rules, and do not restate
        the program.

        ${ctx.output_format()}

        ${role("user")}
        ${for (let i = 0; i < programs.length(); i = i + 1)}Program ${i + 1}, which the compiler ${if (compiles[i])}ACCEPTS${else}REJECTS${endif}:
        <program>
        ${programs[i]}
        </program>

        ${endfor}The rules this case turns on, instantiated at its types. This is the
        rubric: reasoning is sound when it reaches the answer by these, and
        wrong when it reaches it by something else or not at all.
        ${for (let claim in claims)}- ${claim}
        ${endfor}
        The learner answered that ${said}. They were then asked for one
        thing only: ${asked}. Mark what they were asked for.

        Their reasoning:
        """
        ${reasoning}
        """
    `
}

/// Mark `reasoning` with the learner's own key and chosen model.
///
/// The revealed case goes in whole and everything the grader is told is
/// worked out from it here: the programs as text, what the compiler made of
/// each, and the rubric the answer's own question asks for. A caller cannot
/// hand the grader programs that disagree with the verdicts beside them, or
/// a rubric belonging to a different question, because it does not assemble
/// any of them.
///
/// The arms are the whole error channel of a client call: `ai.errors.Failure`
/// is every typed provider failure — a refused key, a model the key cannot
/// reach and a spent quota all arrive as one of its implementors — and the
/// four beside it are what the runtime itself can raise. Nothing is caught
/// that is not named, so a new channel would stop this compiling rather than
/// reaching a learner as a blank.
function judge_reasoning(
    key: string,
    model: string,
    revealed: Revealed,
    said: string,
    reasoning: string,
) -> Judged throws never {
    let answer = said_of(said);
    let programs = revealed.programs.map((program) -> {
        source_of(program.files)
    });
    let compiles = revealed.programs.map((program) -> {
        program.compiles
    });
    let claims = rubric(revealed, answer).map((claim) -> {
        claim.instance
    });
    grade@spec(programs, compiles, claims, answer_sentence(answer), asked_sentence(answer), reasoning)
        .call(client = judge_client(key, model)) catch (e) {
        ai.errors.Failure => Unjudged { why: `${e}` },
        baml.errors.InvalidArgument => Unjudged { why: `${e}` },
        baml.errors.Timeout => Unjudged { why: `${e}` },
        baml.errors.UnknownError => Unjudged { why: `${e}` },
        reflect.errors.CompilationError => Unjudged { why: `${e}` },
    }
}
