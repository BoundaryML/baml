// The model side of the pipeline: the client, the transcript, and the plumbing
// every pass shares.
//
// The passes themselves are being rebuilt TypeScript-first. What lived here
// before asked its question from the BAML side — for each BAML symbol, does a
// shortlist of TypeScript candidates hold a counterpart? — and narrowed that
// shortlist lexically because the model had no way to look anything up. Both
// halves of that design are wrong for the question the matrix now answers, so
// both are gone rather than adapted: a reader arrives knowing TypeScript, and a
// model that can search the BAML surface does not need to be handed a menu.
//
// What survives is what is independent of the question: the client and its
// retry policy, the ordered transcript, the id indexes, and the concurrency and
// progress plumbing.
//
// The model is a proposer, never an authority. Every claim names symbols by id
// and is checked against the surface, so a prompt that starts inventing shows
// up as a count rather than as quietly worse data. A call that does not come
// back is recorded as a failure — never as an empty verdict, which reads
// exactly like an honest "found nothing".

// Reliability composes at the client boundary: the base client is wrapped
// rather than configured with a `retry_policy` block, which is gone.
//
// No `temperature`, deliberately: the current reasoning models reject it
// outright rather than ignoring it. Run-to-run stability comes from
// `--previous` instead — an unchanged symbol is never asked about twice.
//
// No `api_key_env` either: `OPENAI_API_KEY` is what `OpenAiClient` falls back
// to, so naming it would restate the default and invite the two to drift.
client AlignerBase = openai.OpenAiClient.new(model = "gpt-5.5");

// `max_attempts` counts attempts, where the old `max_retries: 3` counted
// retries after the first, so four keeps the previous budget. The backoff
// matches too: one second, doubling.
//
// One client serves the sessions and the preflight both. A rejected key
// arrives as `ai.errors.InvalidRequest`, which `default_retry_judgment`
// declines to retry — a request the provider rejected does not become valid by
// resending — so `check_client` gets the provider's own message on the first
// attempt rather than after four rounds of backoff.
client Aligner = ai.clients.Retry.new(
    AlignerBase,
    max_attempts = 4,
    backoff = ai.clients.Backoff.new(initial_ms = 1000, multiplier = 2),
);

/// One turn of a conversation with the model.
///
/// A transcript rather than one string because the pass being built appends to
/// one: ask, receive a request to look something up, answer it, ask again.
class Turn {
    role: string,
    text: string,
}

function render_turns(turns: Turn[]) -> string {
    let parts: string[] = [];
    for (let turn in turns) {
        parts.push(`[${turn.role}]\n${turn.text}`);
    }
    parts.join("\n\n")
}

// ── how a symbol is shown ───────────────────────────────────────────────────

function first_line(doc: string?) -> string {
    let text = doc ?? "";
    if (text.length() == 0) {
        ""
    } else {
        `  // ${text.lines().at(0) ?? ""}`
    }
}

function ts_line(symbol: TsSymbol) -> string {
    `- [${symbol.id}] ${symbol.signature.lines().at(0) ?? ""}${first_line(symbol.doc)}`
}

/// What a previous run concluded about a symbol that has since changed. Shown
/// so the model can say the conclusion still holds rather than starting over.
function prior_line(judgement: Judgement) -> string {
    let counterpart = if (judgement.baml.length() > 0) {
        judgement.baml.join(", ")
    } else {
        "nothing"
    };
    `- [${judgement.ts}] was ${judgement.verdict} with ${counterpart} — ${judgement.reason ?? "no reason recorded"}`
}

// ── indexes ─────────────────────────────────────────────────────────────────

function ts_by_id(matrix: SymbolMatrix) -> map<string, TsSymbol> {
    let index: map<string, TsSymbol> = {};
    for (let symbol in matrix.ts) {
        let _ = index.set(symbol.id, symbol);
    }
    index
}

/// Judgements in a fixed order, so a report diffs against its predecessor
/// rather than against the order concurrent sessions happened to finish in.
function sort_judgements(matrix: SymbolMatrix) -> null {
    matrix.judgements.sort_by_key((judgement) -> {
        `${judgement.ts}${field_separator()}${judgement.baml.join(",")}`
    });
    null
}

// ── running a pass ──────────────────────────────────────────────────────────

/// How many calls may be in flight at once, by default.
///
/// A cold run is a few hundred sessions that would otherwise all start at the
/// same instant, which is the shape of traffic a provider rate-limits hardest.
/// The cap is deliberately modest: the passes are wide rather than deep, so the
/// run is bounded by the slowest call in each batch either way, and a 429 costs
/// a retry with backoff that would have taken longer than waiting.
/// `--concurrency` raises it for an account whose limits allow.
function call_limit() -> int {
    8
}

/// The cap in force: what the caller asked for, or the default when they did
/// not ask. Zero and negatives mean "unset" rather than "no calls", which is
/// what an unpassed flag looks like by the time it reaches here.
function effective_limit(requested: int) -> int {
    if (requested > 0) {
        requested
    } else {
        call_limit()
    }
}

/// Progress, on stderr.
///
/// Stderr because stdout carries the run's JSON result, which the wrapper
/// parses. A cold run takes minutes and makes no visible output otherwise; a
/// pass that silently stalls on a rate limit is indistinguishable from one
/// doing useful work.
function report_progress(message: string) -> void {
    baml.io.eprintln(`  stdlib-matrix: ${message}`);
}

// ── the session ─────────────────────────────────────────────────────────────
//
// One session per TypeScript container, answering for the container and every
// member it holds. A container is the right unit because its members share a
// context: whoever has just worked out that `Date` is `baml.time.Instant` is
// the cheapest possible judge of `Date.now`, and asking per member would pay
// for that discovery fifty-one times.
//
// The session is a loop, not a call. The model may look things up, and what it
// looks up is decided by what it has already seen — which is the whole reason
// the old shortlist could not work: a menu has to be built before the question
// is understood.

/// A lookup the model wants run.
class Lookup {
    tool: string @description("one of: describe, search, examples, docs"),
    query: string,
    why: string? @description("what you expect to learn, under 10 words"),
}

/// The model asking to see something before it answers.
class Investigate {
    lookups: Lookup[] @description("at most 4 at a time"),
}

/// What was concluded about one TypeScript symbol.
class Finding {
    ts: string @description("the TypeScript id from inside [ ], exactly as written"),
    verdict: "match" | "divergent" | "unnecessary" | "none",
    baml: string[] @description(
        "full dotted BAML paths, e.g. baml.time.Instant.now; empty for none and usually for unnecessary",
    ),
    confidence: "high" | "medium" | "low",
    reason: string @description("under 20 words"),
    divergence: string? @description("how the two differ; only for divergent, under 25 words"),
    example: CodeExample? @description("only where seeing both spellings teaches something prose cannot"),
}

/// The model's answer for every symbol it was asked about.
class Answer {
    findings: Finding[],
}

/// A turn of the loop: look something up, or answer.
type Step = Investigate | Answer;

/// What one session was asked about.
class SessionRequest {
    /// The container or module, for progress and for failure reporting.
    subject: string,
    /// The TypeScript ids this session must answer for — all of them. Kept
    /// apart from the rendered lines because completeness is checked against
    /// it: a session that answers for some of its symbols is sent back.
    ts_ids: string[],
    ts_lines: string[],
    prior_lines: string[],
}

/// A session that produced nothing, and why.
type SessionResult = Answer | PassFailure;

function session_rules() -> string {
    #"
        You are mapping the TypeScript standard library onto BAML's, for a developer who
        knows TypeScript and wants to know how to do the same thing in BAML.

        You will be shown TypeScript symbols. For each one, decide what a BAML developer
        would do instead, and say so. You do not know the BAML standard library from
        memory — look it up. Guessing at a symbol that does not exist is the one failure
        that matters here, because it is indistinguishable from an answer.

        Tools, one JSON object each:
          describe  a BAML symbol or namespace by name — `baml.sys.exec`, `baml.time`
          search    what something does, in words — "run a subprocess", "parse a date"
          examples  real call sites for a symbol, from the BAML repository
          docs      one section of the BAML type system reference

        `search` is the one to reach for first when you do not know the name, which is
        most of the time. `describe` is for confirming a name you have.

        Answer once per TypeScript symbol, with one of:
          match        the same operation, reached the same way
          divergent    the same operation, reached differently — say how in `divergence`
          unnecessary  BAML makes the question not arise
          none         nothing in BAML does this symbol's job

        `unnecessary` is the verdict that needs care, and it is often the right one.
        `Array.isArray` is not missing from BAML and has no counterpart: BAML checks
        types by pattern matching, so the question only comes up for a value in a union,
        and `foo is int[]` answers it there. Say that, rather than "none". The same goes
        for anything the language answers instead of the library — `typeof` against
        `match`, `instanceof` against a type pattern.

        Do not assume BAML's semantics; look them up. BAML values are mutable and arrays
        are references, so anything you would justify with "values are immutable" is
        probably wrong.

        Name TypeScript symbols by the id inside [ ], exactly as written. Name BAML
        symbols by their full dotted path, the way `describe --search` prints it —
        `baml.time.Instant.now`, not `Instant.now`. A BAML path you have not seen in a
        tool result does not exist.

        `match` and `divergent` must name at least one BAML symbol — they are claims
        that something over there does this job, so they have to say what. If the answer
        is syntax rather than a symbol — indexing, an operator, `.to_string()` — that is
        `unnecessary`, with an example. If you could not find anything, that is `none`,
        with a reason saying what you looked for.

        Prefer "none" over a strained pairing. TypeScript has plenty BAML has not, and
        saying so is a finding rather than a failure to try harder.

        Attach an `example` only where seeing both spellings teaches something the
        reason cannot — a divergence, or an `unnecessary` whose answer is syntax rather
        than a symbol. Most findings want none.

        An example's `baml` field is compiled before your answer is accepted. It must be
        a complete BAML file: declarations at the top level, statements inside a
        function. This compiles:

          function example() -> string {
              let parts = "a,b,c".split(",");
              parts.join("-")
          }

        If it does not compile you will be shown the error and asked again. Dropping the
        example is a fine outcome; an example that does not compile is not, because it is
        the part a reader will copy.
    "#
}

/// What a session is told before it is asked anything: what BAML is, then what
/// the job is.
///
/// The skill goes first because it is the part that changes what the model
/// believes rather than what it does — a model that thinks BAML is a prompt
/// template DSL will look for the wrong things whatever the instructions say.
function session_system(box: Toolbox) -> string {
    `
        ${box.skill}

        ---

        ${session_rules()}
    `
}

function session_user(request: SessionRequest) -> string {
    let prior = if (request.prior_lines.length() > 0) {
        `

A previous run concluded the following. The symbols have changed since, so these
are up for review — repeat a conclusion if it still holds.
${request.prior_lines.join("\n")}`
    } else {
        ""
    };
    `TypeScript symbols in ${request.subject}:
${request.ts_lines.join("\n")}${prior}

Look up what you need, then answer for every symbol above.`
}

/// One turn of the conversation with the model.
function session_step(transcript: string) -> Step {
    client: Aligner
    prompt: `
        ${transcript}

        ${ctx.output_format}
    `
}

/// The last turn, which cannot ask for anything.
///
/// Identical to `session_step` but for the return type, and that is the whole
/// point: telling a model the lookup budget is spent is advice it can decline,
/// and three sessions in a real run declined it and died with nothing to show.
/// Answering is the only shape this call has, so a session cannot spend its
/// last round asking a seventh time.
function session_final(transcript: string) -> Answer {
    client: Aligner
    prompt: `
        ${transcript}

        ${ctx.output_format}
    `
}

/// One trivial call, before any of the expensive ones.
///
/// A client that is not usable fails every session the same way, and 199
/// sessions all failing takes a couple of minutes — long enough to look like
/// work, and it ends with a report that judged nothing. A rejected key is not a
/// transient error either, so the retry policy does not slow it down: the run
/// completes having learned nothing, and the only signal is 199 identical
/// failures.
///
/// This asks the smallest question there is, first, so a misconfigured client
/// says so while it is still free to fix.
function preflight(question: string) -> string {
    client: Aligner
    prompt: `${question}`
}

/// Fails the run if the model client cannot be reached at all.
function check_client() -> null {
    let _ = preflight("Reply with one word: ready") catch_all (error) {
        _ => {
            throw baml.errors.InvalidArgument {
                message: `the model client rejected a trial call, so every session would fail the same way.\n${error}\nCheck OPENAI_API_KEY, and that this account may use the model the Aligner client names.`,
            };
        },
    };
    null
}

/// How many rounds of lookups a session may take before it must answer.
///
/// Enough to search, read two or three candidates, and check an example; past
/// that a session is not converging and another round is unlikely to help.
function lookup_rounds() -> int {
    6
}

/// At most this many lookups per round, so one round cannot become a scan of
/// the whole surface.
function lookups_per_round() -> int {
    4
}

/// Runs one session to an answer.
///
/// A session that will not answer is a failure, never an empty verdict: an
/// `Answer` with no findings and a session that ran out of rounds are the same
/// JSON, and they mean opposite things about the two libraries.
/// Runs one session until it has answered for every symbol it was asked about.
///
/// Findings accumulate rather than replace. A model that answers for 51 of 52
/// symbols has done nearly all the work, and throwing that away to ask again
/// from scratch would be both wasteful and worse — it gets a second chance to
/// change its mind about the 51 it already got right. So each attempt
/// contributes what it covers, and only what is still outstanding is asked for
/// again.
///
/// A finding is outstanding if it never arrived, or if it arrived carrying an
/// example that does not compile. Those are the same problem — a symbol without
/// a usable answer — so they take the same route back.
///
/// A session that will not answer is a failure, never an empty verdict: an
/// `Answer` with no findings and a session that ran out of rounds are the same
/// JSON, and they mean opposite things about the two libraries.
function run_session(
    box: Toolbox,
    request: SessionRequest,
    baml_names: map<string, string>,
) -> SessionResult {
    let turns: Turn[] = [
        Turn { role: "system", text: session_system(box) },
        Turn { role: "user", text: session_user(request) },
    ];
    let asked: map<string, bool> = {};
    for (let id in request.ts_ids) {
        let _ = asked.set(id, true);
    }

    let collected: Finding[] = [];
    let covered: map<string, bool> = {};
    // Findings that arrived with something wrong that the model can fix — an
    // example that will not compile, a BAML name that does not resolve. Held
    // back so the symbol is asked about again, but kept so that running out of
    // attempts costs the flaw rather than the judgement.
    let held: map<string, Finding> = {};

    // Rounds and attempts are counted apart. Sharing one counter meant an
    // answer consumed a lookup round, so a session that used its lookup budget
    // and then answered incompletely had its retry turn built and immediately
    // discarded — the two further attempts `answer_attempts` promises were
    // never reachable. `steps` is the backstop that keeps the loop finite.
    let round = 0;
    let attempts = 0;
    let steps = 0;
    let step_limit = lookup_rounds() + answer_attempts() + 1;
    let answered = false;
    let failure = `no answer after ${lookup_rounds()} rounds of lookups`;

    while (!answered && steps < step_limit) {
        steps += 1;
        let final_round = round >= lookup_rounds();
        // The last round cannot look anything up: `session_final` returns an
        // answer and nothing else, so the shape of the call enforces what this
        // turn explains. Saying it as well is not redundant — a model handed a
        // schema it did not expect does better for being told why.
        if (final_round && attempts == 0) {
            turns.push(
                Turn {
                    role: "user",
                    text: "No more lookups are available. Answer now for every symbol, using only what you have seen. A symbol you could not resolve is `none` with a reason.",
                },
            );
        }
        // A failed call ends the session but does not empty it. Returning here
        // threw away everything already collected — a session that answered for
        // 28 of 30 symbols and then hit a 529 recorded nothing, contradicting
        // the accumulate-rather-than-replace rule this function is built on.
        let attempt: Step? = if (final_round) {
            session_final(render_turns(turns))
        } else {
            session_step(render_turns(turns))
        } catch_all (error) {
            _ => {
                failure = `${error}`;
                null
            },
        };
        let step = match (attempt) {
            null => {
                break;
            },
            let taken: Step => taken,
        };
        match (step) {
            let investigate: Investigate => {
                round += 1;
                turns.push(Turn { role: "assistant", text: render_lookups(investigate) });
                turns.push(Turn { role: "tool", text: run_lookups(box, investigate) });
            },
            let answer: Answer => {
                attempts += 1;
                let complaints: string[] = [];
                for (let finding in answer.findings) {
                    if (!asked.has(finding.ts)) {
                        // A symbol this session was not shown. Reported rather
                        // than dropped: it never reaches `apply_findings`, so
                        // it would not appear in the refused count either, and
                        // that count is the only signal that a prompt has begun
                        // answering questions nobody asked.
                        report_progress(`${request.subject}: answered about ${finding.ts}, which it was not shown`);
                        continue;
                    }
                    if (covered.has(finding.ts)) {
                        // Settled by an earlier attempt; the first answer wins.
                        continue;
                    }
                    match (finding_fault(box, finding, baml_names)) {
                        null => {
                            let _ = covered.set(finding.ts, true);
                            collected.push(finding);
                        },
                        let fault: string => {
                            let _ = held.set(finding.ts, finding);
                            complaints.push(fault);
                        },
                    }
                }

                let missing: string[] = [];
                for (let id in request.ts_ids) {
                    if (!covered.has(id)) {
                        missing.push(id);
                    }
                }
                if (missing.length() == 0 || attempts >= answer_attempts()) {
                    // Complete, or out of attempts. What was held back is taken
                    // below, on the way out.
                    answered = true;
                } else {
                    turns.push(Turn { role: "assistant", text: render_findings(answer) });
                    turns.push(
                        Turn { role: "user", text: retry_turn(request, covered, complaints) },
                    );
                }
            },
        }
    }

    // Anything held back is taken without the part that was wrong: a judgement
    // with no example is worth having, and one naming a symbol that does not
    // exist is worth having without that name. Drained here rather than where
    // the attempts run out, because the loop can also end by running out of
    // rounds or by a failed call — and on those paths the finding was being
    // discarded entirely, the opposite of the rule it was meant to follow.
    for (let id in request.ts_ids) {
        if (!covered.has(id)) {
            if let finding: Finding = held.get(id) {
                if let salvaged: Finding = without_faults(finding, baml_names) {
                    collected.push(salvaged);
                    let _ = covered.set(id, true);
                }
            }
        }
    }

    // Nothing usable is a failure, never an empty answer. `Answer { findings:
    // [] }` and a session that died are the same JSON and mean opposite things,
    // and only a `PassFailure` reaches `matrix.failures`, the markdown, and the
    // CI annotation.
    if (collected.length() == 0) {
        PassFailure { pass: "session", subject: request.subject, reason: failure }
    } else {
        Answer { findings: collected }
    }
}

/// What is wrong with a finding, or null when it can be accepted as it stands.
///
/// Both checks run here, inside the session, so the model can be told and can
/// fix it. The name check used to run in `apply_findings` instead — after the
/// session had closed — so a hallucinated path was dropped silently with unused
/// attempts still on the table, and the README's claim that a session must
/// satisfy it "before its answer is accepted" was not true of the one failure
/// mode the design says matters most.
function finding_fault(box: Toolbox, finding: Finding, baml_names: map<string, string>) -> string? {
    let unknown: string[] = [];
    for (let named in finding.baml) {
        if (resolve_baml_name(baml_names, named) == null) {
            unknown.push(named);
        }
    }
    if (unknown.length() > 0) {
        return `[${finding.ts}] these are not BAML symbols: ${unknown.join(", ")}. Look them up, or leave the baml list empty and say what you looked for.`;
    }
    // A pairing verdict that names nothing is refused here, not only in
    // `apply_findings`. That check runs after the session has ended, so the
    // symbol was already marked covered: the model was never told, never spent
    // a retry putting it right, and the symbol came out unjudged with the
    // reason recorded somewhere the session could not see.
    if (finding.baml.length() == 0 && judgement_pairs_verdict(finding.verdict)) {
        return `[${finding.ts}] you said ${finding.verdict} but named no BAML symbol. Name what a developer would use, or say none if there is nothing.`;
    }
    // `divergent` is not a stronger `match`, it is the verdict that carries a
    // caveat, and the caveat is the whole of what it adds. Thirteen shipped
    // without one, rendering as a striped swatch whose tooltip had nothing to
    // say. Either the difference can be stated or the verdict is `match`.
    if (finding.verdict == "divergent" && (finding.divergence ?? "").trim().length() == 0) {
        return `[${finding.ts}] you said divergent but did not say how they differ. Fill in divergence, or use match if they do not.`;
    }
    // The converse holds for the other three: a difference stated under a
    // verdict that claims none is a contradiction, and the reader is shown the
    // verdict. Asking is cheaper than guessing which half was meant.
    if (finding.verdict != "divergent" && (finding.divergence ?? "").trim().length() > 0) {
        return `[${finding.ts}] you gave a divergence but the verdict is ${finding.verdict}. Use divergent if they differ, or drop the divergence.`;
    }
    if let error: string = example_error(box, finding) {
        // Clipped like a tool answer, and for the same reason: this goes into
        // the transcript and stays there for every later round. A session with
        // twenty broken examples, each with multi-diagnostic output, otherwise
        // adds tens of kilobytes per attempt — and the snippet is the model's
        // own text, so neither half is bounded by anything.
        let snippet = clip(finding.example?.baml ?? "", complaint_budget());
        return `[${finding.ts}] this example does not compile:\n${snippet}\n\n${clip(error, complaint_budget())}`;
    }
    null
}

/// The most one complaint may contribute to a transcript. Smaller than a tool
/// answer's budget because a retry can carry one per finding.
function complaint_budget() -> int {
    1500
}

/// A held finding with whatever was wrong removed, or null if nothing is left
/// worth keeping.
///
/// A pairing whose every counterpart was invented is dropped: recording it with
/// an empty list would turn a hallucination into a confident "nothing in BAML
/// does this".
function without_faults(finding: Finding, baml_names: map<string, string>) -> Finding? {
    let resolved: string[] = [];
    for (let named in finding.baml) {
        if let id: string = resolve_baml_name(baml_names, named) {
            resolved.push(id);
        }
    }
    if (resolved.length() == 0 && finding.baml.length() > 0 && finding.verdict != "none") {
        return null;
    }
    // A `divergent` with no divergence text is *not* dropped here, though
    // `finding_fault` refused it round after round. An absent caveat cannot be
    // stripped the way a broken example can, so the choice is to keep an
    // incomplete finding or discard a real one — and "these differ" is still
    // what the session concluded. It renders as a difference nobody wrote down,
    // which is worse than the sentence and better than silence.
    Finding {
        ts: finding.ts,
        verdict: finding.verdict,
        baml: resolved,
        confidence: finding.confidence,
        reason: finding.reason,
        divergence: finding.divergence,
        example: null,
    }
}

/// How many answers a session may give before what it has is taken as final.
///
/// Three: the first, one to fill gaps, one to fix what the second broke. A
/// session still incomplete after that is not converging, and the coverage line
/// reports what it left.
function answer_attempts() -> int {
    3
}

/// What to say when an answer was not complete.
///
/// Names the symbols rather than counting them. "You did not answer for 1 of 52"
/// makes the model re-derive which one; naming it does not.
function retry_turn(
    request: SessionRequest,
    covered: map<string, bool>,
    complaints: string[],
) -> string {
    let missing: string[] = [];
    for (let id in request.ts_ids) {
        if (!covered.has(id)) {
            missing.push(`- ${id}`);
        }
    }
    let parts: string[] = [];
    if (complaints.length() > 0) {
        parts.push(complaints.join("\n\n"));
    }
    parts.push(
        `You have not answered for these symbols:\n${missing.join("\n")}\n\nAnswer for exactly these. Everything else you said is kept — do not repeat it. If something above would not compile or names a symbol that does not exist, fix it or leave it out; the finding matters, the example is optional.`,
    );
    parts.join("\n\n")
}

/// Why a finding's example will not do, or null when it has none or it compiles.
function example_error(box: Toolbox, finding: Finding) -> string? {
    let out: string? = null;
    if let example: CodeExample = finding.example {
        out = check_snippet(box, example.baml);
    }
    out
}

/// An answer as the transcript should record it, so a retry can see what it
/// already said rather than being asked to remember.
function render_findings(answer: Answer) -> string {
    let lines: string[] = [];
    for (let finding in answer.findings) {
        lines.push(`${finding.ts} ${finding.verdict} ${finding.baml.join(", ")}`);
    }
    lines.join("\n")
}

function render_lookups(investigate: Investigate) -> string {
    let lines: string[] = [];
    for (let lookup in investigate.lookups) {
        lines.push(`${lookup.tool} ${lookup.query}`);
    }
    lines.join("\n")
}

/// The most one lookup may contribute to a transcript.
///
/// Every round's results stay in the transcript for every later round, so an
/// unbounded tool is a transcript that grows unboundedly: four lookups of the
/// `Interfaces` section of the type-system reference is 44KB in one round and
/// 264KB by the sixth. Eight thousand characters holds every `describe` output
/// of a single symbol whole, and truncates the largest namespace listings and
/// the two largest documentation sections.
function lookup_budget() -> int {
    8000
}

/// A tool's answer, cut to size, saying so where it was cut.
///
/// Silent truncation would be the worse failure by far: the model would read a
/// class's members as complete and conclude that the one it needed is absent.
function clip(text: string, budget: int) -> string {
    if (text.length() <= budget) {
        text
    } else {
        `${text.substring(0, budget)}\n… truncated at ${budget} characters; ask something narrower to see the rest`
    }
}

/// Runs the round's lookups and renders what they said.
function run_lookups(box: Toolbox, investigate: Investigate) -> string {
    let blocks: string[] = [];
    let ran = 0;
    for (let lookup in investigate.lookups) {
        if (ran < lookups_per_round()) {
            ran += 1;
            // The query is the model's own text and is echoed back, so it is
            // bounded too — the answer was clipped and the label was not.
            let asked = clip(lookup.query, complaint_budget());
            let answer = clip(run_tool(box, lookup.tool, lookup.query), lookup_budget());
            blocks.push(`$ ${lookup.tool} ${asked}\n${answer}`);
        }
    }
    if (investigate.lookups.length() > ran) {
        blocks.push(
            `(${investigate.lookups.length() - ran} further lookups were not run — at most ${lookups_per_round()} per round)`,
        );
    }
    blocks.join("\n\n")
}

// ── planning and folding ────────────────────────────────────────────────────

/// What a full alignment did, in numbers a reader can check against the report.
class AlignOutcome {
    matrix: SymbolMatrix,
    sessions: int,
    judged: int,
    /// Claims naming a symbol that was never offered, or a second claim on a
    /// symbol already spoken for. Counted by name so a drifting prompt is
    /// visible rather than merely quieter.
    rejected: string[],
}

/// The session a symbol belongs to: its container, or its module when it is
/// reached on the module itself.
function session_subject(symbol: TsSymbol) -> string {
    if (symbol.origin == "free") {
        symbol.symbol.at(0) ?? "(unknown)"
    } else {
        let module = symbol.symbol.at(0) ?? "";
        let container = symbol.symbol.at(1) ?? "";
        `${module}.${container}`
    }
}

/// The most symbols one session is asked about.
///
/// Not a cost control — the sessions are wide, so splitting one costs a second
/// system prompt and little else. It is a correctness control. `node:fs` has 92
/// symbols and `node:buffer.Buffer` 77, and a model that answers for sixty of
/// them and stops produces exactly what a complete answer produces: findings
/// for the ones it reached, and silence for the rest. Silence is already what
/// "nobody has looked" means, so a truncated session would be indistinguishable
/// from one that never ran. Thirty is comfortably inside what a single response
/// holds, and the median session is ten.
function session_chunk() -> int {
    30
}

/// One request per container, plus one per module for its free functions.
///
/// A container's members ride with it because they share a context; a module's
/// free functions have no container to share, so they are batched into one
/// session for the module rather than asked about one at a time.
function session_requests(matrix: SymbolMatrix, revision: Revision?) -> SessionRequest[] {
    let judged: map<string, bool> = {};
    for (let judgement in matrix.judgements) {
        let _ = judged.set(judgement.ts, true);
    }

    let groups: map<string, TsSymbol[]> = {};
    let order: string[] = [];
    for (let symbol in matrix.ts) {
        if (judged.has(symbol.id)) {
            continue;
        }
        let subject = session_subject(symbol);
        if (!groups.has(subject)) {
            let empty: TsSymbol[] = [];
            let _ = groups.set(subject, empty);
            order.push(subject);
        }
        if let bucket: TsSymbol[] = groups.get(subject) {
            bucket.push(symbol);
        }
    }

    let no_priors: Judgement[] = [];
    let requests: SessionRequest[] = [];
    for (let subject in order) {
        let members: TsSymbol[] = groups.get(subject) ?? [];
        let chunks = (members.length() + session_chunk() - 1) / session_chunk();
        let index = 0;
        while (index < members.length()) {
            let ts_ids: string[] = [];
            let ts_lines: string[] = [];
            let prior_lines: string[] = [];
            let taken = 0;
            while (taken < session_chunk() && index < members.length()) {
                if let member: TsSymbol = members.at(index) {
                    ts_ids.push(member.id);
                    ts_lines.push(ts_line(member));
                    if let before: Revision = revision {
                        for (let judgement in before.judgements.get(member.id) ?? no_priors) {
                            prior_lines.push(prior_line(judgement));
                        }
                    }
                }
                index += 1;
                taken += 1;
            }
            // A part number only when there is more than one, so the common
            // case reads as the container it is.
            let label = if (chunks > 1) {
                `${subject} (part ${(index + session_chunk() - 1) / session_chunk()} of ${chunks})`
            } else {
                subject
            };
            requests.push(
                SessionRequest {
                    subject: label,
                    ts_ids: ts_ids,
                    ts_lines: ts_lines,
                    prior_lines: prior_lines,
                },
            );
        }
    }
    requests
}

/// The sessions whose subject contains `only`, or all of them when it is empty.
///
/// For proving the loop end to end without paying for the whole surface: a cold
/// run is two hundred sessions, and the first thing worth knowing about a
/// prompt change is whether one session works at all.
function only_matching(requests: SessionRequest[], only: string) -> SessionRequest[] {
    let wanted = only.trim().to_lower_case();
    if (wanted.length() == 0) {
        return requests;
    }
    let kept: SessionRequest[] = [];
    for (let request in requests) {
        if (request.subject.to_lower_case().includes(wanted)) {
            kept.push(request);
        }
    }
    report_progress(`--only ${only}: ${kept.length()} of ${requests.length()} sessions`);
    kept
}

/// Every name a BAML symbol answers to in a finding, mapped to its id.
///
/// Distinct from `baml_name_index` in references.baml, which resolves names
/// inside a signature against the scope that wrote them. This resolves names a
/// model typed after reading tool output, so it accepts every spelling the
/// tools print and none of the scoping.
///
/// A finding names symbols the way the tools printed them, which is not the way
/// the export addresses them: `baml describe` says `baml.time.Instant.now`,
/// while the id is `M:baml.time.Instant.now`. The kind prefix is an
/// implementation detail of the export IR — it exists because BAML's type and
/// value namespaces are distinct, so a bare path would be ambiguous between
/// them — and nothing outside this program knows about it or should have to.
/// Demanding it rejected 39 correct answers in a row and, because a pairing
/// with no surviving counterpart is dropped, left 20 symbols unjudged.
function baml_finding_names(matrix: SymbolMatrix) -> map<string, string> {
    let names: map<string, string> = {};
    for (let symbol in matrix.baml) {
        let _ = names.set(symbol.id, symbol.id);
    }
    // Paths first, then displays, so a path always wins a name a display also
    // wants. Both go through the same rule: a name two symbols answer to is
    // recorded as ambiguous rather than resolved.
    for (let symbol in matrix.baml) {
        claim_name(names, strip_kind_prefix(symbol.id), symbol.id);
    }
    for (let symbol in matrix.baml) {
        claim_name(names, symbol.display, symbol.id);
    }
    names
}

/// Records a name for a symbol, or marks it ambiguous if another already has
/// it.
///
/// Ambiguity is possible for a dotted path as well as for a display, which is
/// the whole reason the export prefixes its ids by kind: BAML's type and value
/// namespaces are distinct, so `baml.Foo` can name both a class and a function.
/// The current standard library has no such pair — but resolving one silently
/// to whichever was iterated last would attach a judgement to the wrong symbol,
/// and that is a wrong answer rather than a missing one.
function claim_name(names: map<string, string>, key: string, id: string) -> null {
    if (key.length() == 0) {
        return null;
    }
    match (names.get(key)) {
        null => {
            let _ = names.set(key, id);
        },
        let existing: string => {
            if (existing != id) {
                let _ = names.set(key, "");
            }
        },
    }
    null
}

/// The id a name refers to, or null when nothing does — or when more than one
/// thing does.
function resolve_baml_name(names: map<string, string>, named: string) -> string? {
    let out: string? = null;
    if let found: string = names.get(named.trim()) {
        if (found.length() > 0) {
            out = found;
        }
    }
    out
}

/// Folds a session's findings in.
///
/// Names are resolved again here rather than trusted. The session already
/// refuses a finding whose names do not resolve, so this is the second line
/// rather than the first — but a claim recorded without checking is a claim
/// about two libraries that reads exactly like a verified one, and the count of
/// refusals is how a drifting prompt becomes visible.
function apply_findings(matrix: SymbolMatrix, results: SessionResult[]) -> AlignOutcome {
    let baml_names = baml_finding_names(matrix);
    let ts_index = ts_by_id(matrix);
    let judged: map<string, bool> = {};
    for (let judgement in matrix.judgements) {
        let _ = judged.set(judgement.ts, true);
    }

    let rejected: string[] = [];
    let applied = 0;
    let none: string[] = [];
    for (let result in results) {
        match (result) {
            let failure: PassFailure => {
                matrix.failures.push(failure);
            },
            let answer: Answer => {
                for (let finding in answer.findings) {
                    if (!ts_index.has(finding.ts)) {
                        rejected.push(`${finding.ts} is not a TypeScript symbol`);
                        continue;
                    }
                    if (judged.has(finding.ts)) {
                        rejected.push(`${finding.ts} was already judged`);
                        continue;
                    }
                    let counterparts: string[] = [];
                    let invented = false;
                    for (let named in finding.baml) {
                        match (resolve_baml_name(baml_names, named)) {
                            let id: string => {
                                counterparts.push(id);
                            },
                            null => {
                                rejected.push(`${named} is not a BAML symbol`);
                                invented = true;
                            },
                        }
                    }
                    // A pairing with nothing left to point at is not a pairing.
                    // Recording it with an empty list would turn a hallucination
                    // into a confident "no counterpart" — and a `match` that
                    // named nothing to begin with is the same claim arrived at
                    // by a different route, so both are refused here.
                    if (counterparts.length() == 0 && judgement_pairs_verdict(finding.verdict)) {
                        // Each invented name was already listed above; saying
                        // so again would double-count the same mistake. What
                        // needs its own line is the other route to an empty
                        // list — a pairing verdict that named nothing at all.
                        if (!invented) {
                            rejected.push(`${finding.ts} claims a counterpart but names none`);
                        }
                        continue;
                    }
                    let _ = judged.set(finding.ts, true);
                    matrix.judgements.push(
                        Judgement {
                            ts: finding.ts,
                            baml: counterparts,
                            verdict: finding.verdict,
                            basis: "model",
                            confidence: finding.confidence,
                            reason: finding.reason,
                            divergence: finding.divergence,
                            rejected: none,
                            verified: false,
                            example: finding.example,
                        },
                    );
                    applied += 1;
                }
            },
        }
    }

    matrix.counts = recount(matrix);
    AlignOutcome { matrix: matrix, sessions: results.length(), judged: applied, rejected: rejected }
}

function run_sessions(
    box: Toolbox,
    requests: SessionRequest[],
    limit: int,
    baml_names: map<string, string>,
) -> SessionResult[] {
    let group = baml.spawn.TaskGroup.new(limit, name = "sessions");
    let pending = requests.map((request) -> {
        spawn with baml.spawn.options(group = group) { run_session(box, request, baml_names) }
    });
    report_progress(`${requests.length()} sessions, ${limit} at a time`);
    let results: SessionResult[] = [];
    for (let future in pending) {
        results.push(await future);
        if (results.length() % 10 == 0 || results.length() == pending.length()) {
            report_progress(`sessions: ${results.length()}/${pending.length()}`);
        }
    }
    results
}

/// Judges everything the revision did not carry.
function align_matrix(
    matrix: SymbolMatrix,
    box: Toolbox,
    revision: Revision?,
    concurrency: int,
    only: string,
) -> AlignOutcome {
    let limit = effective_limit(concurrency);
    check_client();
    let requests = only_matching(session_requests(matrix, revision), only);
    //# ask, per container, what a BAML developer would do instead
    let queued = 0;
    for (let request in requests) {
        queued += request.ts_lines.length();
    }
    let outcome = apply_findings(matrix, run_sessions(box, requests, limit, baml_finding_names(matrix)));
    report_progress(`${outcome.judged} of ${queued} judged, ${outcome.rejected.length()} claims refused`);
    // A session that answers for some of its symbols and not the rest leaves
    // exactly what a session that never ran leaves. Saying so is the only way
    // the difference is visible.
    if (outcome.judged < queued) {
        report_progress(`${queued - outcome.judged} symbols were asked about and not answered`);
    }
    if (outcome.matrix.failures.length() > 0) {
        report_progress(`${outcome.matrix.failures.length()} sessions did not come back`);
    }
    sort_judgements(outcome.matrix);
    outcome
}
