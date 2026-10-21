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

/// The mark a grade amounts to, in the words a mark is kept in.
///
/// Takes the grade rather than its `understanding` for the reason given on
/// `words_of`: an enum argument does not survive the generated web SDK. The
/// word itself is `understanding_word`'s, so a judged mark and one the
/// learner made by hand are spelled by the same function.
function mark_of(grade: Grade) -> string throws never {
    understanding_word(grade.understanding)
}

/// Who answers a judgement.
///
/// The provider decides how the request is made, which is the only thing
/// outside the model's name that differs between them.
enum Provider {
    Anthropic,
    OpenAI,
}

/// How a provider is put to a learner: its name, and what its keys look
/// like. A surface asking for a key needs both and should not be deciding
/// either from the provider's spelling — adding a provider adds an arm below
/// and stops compiling until it has words.
class ProviderWords {
    name: string,
    key_hint: string,
}

/// BUG: an enum cannot be passed as an argument from the generated web SDK —
/// a TypeScript string enum's member IS a string at run time, the bridge's
/// encoder lowers it as one, and the call dies in the VM with "expected
/// variant, got string". So nothing here takes a `Provider` from a caller
/// across that boundary: the words ride on the `Offer` the caller already
/// holds. See the README's toolchain issues.
function words_of(provider: Provider) -> ProviderWords throws never {
    match (provider) {
        Provider.Anthropic => ProviderWords { name: "Anthropic", key_hint: "sk-ant-…" },
        Provider.OpenAI => ProviderWords { name: "OpenAI", key_hint: "sk-…" },
    }
}

/// A model a judgement can be asked of.
class Offer {
    /// The provider's own name for the model, which is also what a learner's
    /// saved settings hold.
    id: string,
    /// What the learner reads.
    label: string,
    provider: Provider,
    /// How to ask for this provider's key. Carried on the row rather than
    /// looked up by a caller, so asking for a key needs nothing but the row.
    words: ProviderWords,
}

/// One row of the table.
function offer(id: string, label: string, provider: Provider) -> Offer throws never {
    Offer { id, label, provider, words: words_of(provider) }
}

/// The models a judgement may be asked of, best first for each provider.
///
/// One table. A surface offers a learner exactly these and hands an `id`
/// back, and `judge_client` builds a client for exactly these, so a model
/// that can be chosen but not asked — or asked but never offered — does not
/// exist. Adding one is adding a row.
function offered() -> Offer[] throws never {
    [
        offer("claude-sonnet-4-5", "Claude Sonnet 4.5", Provider.Anthropic),
        offer("claude-haiku-4-5", "Claude Haiku 4.5", Provider.Anthropic),
        offer("gpt-5", "GPT-5", Provider.OpenAI),
        offer("gpt-5-mini", "GPT-5 mini", Provider.OpenAI),
    ]
}

/// The offer `id` names.
///
/// # Panics
///
/// - `baml.panics.Unreachable` if the id is not one of `offered()`. The
///   caller is an interface of ours rather than a stranger: it chose from
///   this table, so a name it never showed anyone is a defect in it and not
///   something to absorb.
function offer_of(id: string) -> Offer throws never {
    offered().find((offer) -> {
        offer.id == id
    })
        ?? throw baml.panics.Unreachable { message: `no such model: ${id}` }
}

/// The client a judgement is made with: the model, and the learner's own key.
///
/// Anthropic asks for a header before it will answer a request made from a
/// browser; OpenAI asks for nothing beyond the key, so there is nothing to
/// pass. Everything else about the two requests is the provider's business.
function judge_client(key: string, model: string) -> ai.Client throws never {
    let offer = offer_of(model);
    match (offer.provider) {
        Provider.Anthropic => {
            anthropic.Client.new(
                model = offer.id,
                api_key = key,
                headers = { "anthropic-dangerous-direct-browser-access": "true" },
            )
        },
        Provider.OpenAI => openai.ChatClient.new(model = offer.id, api_key = key),
    }
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
