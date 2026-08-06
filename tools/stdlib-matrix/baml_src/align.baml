// The model passes: what the name heuristic cannot reach.
//
// Three passes, each answering a different question:
//
//   1. propose — for each BAML symbol still unjudged, does its pool hold a
//      counterpart? Runs BAML-first, one call per owner.
//   2. sweep   — for each TypeScript symbol nothing claims, is there a BAML
//      symbol that does its job? The other direction, because a BAML name that
//      suggests nothing gives pass 1 nothing to search for.
//   3. verify  — for every proposed pairing, and every pairing the name pass
//      produced, is it actually true? Adversarial, and framed to refute.
//
// The model is a proposer, never an authority. Every claim names symbols by id
// and is checked against the pool that was actually offered, so a prompt that
// starts inventing shows up as a count rather than as quietly worse data. A
// call that does not come back is recorded as a failure — never as an empty
// verdict, which reads exactly like an honest "found nothing".
//
// Each pass takes an ordered transcript rather than one string, and answers
// with a union of variants. Both are for what comes next: a `Lookup` variant
// the runner executes and appends as another turn is then an addition rather
// than a rewrite. Pool-narrowing is the non-agentic approximation of that
// lookup — showing a plausible menu up front instead of letting the model ask.

retry_policy AlignerRetry {
    max_retries: 3,
    backoff_multiplier: 2.0,
    initial_delay_ms: 1000,
}

// No `temperature`: claude-sonnet-5 rejects it ("`temperature` is deprecated
// for this model"). Run-to-run stability comes from `--previous` instead — an
// unchanged symbol is never asked about twice.
client<llm> Aligner {
    provider: anthropic,
    retry_policy: AlignerRetry,
    options: {
        model: "claude-sonnet-5",
        api_key: env.ANTHROPIC_API_KEY,
    },
}

/// One turn of a conversation with the model.
///
/// The passes each send a single turn today. The shape is a transcript because
/// the next thing to build appends to one: ask, receive a request for detail,
/// answer it, ask again.
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

// ── what the model is shown ─────────────────────────────────────────────────

function first_line(doc: string?) -> string {
    let text = doc ?? "";
    if (text.length() == 0) {
        ""
    } else {
        `  // ${text.lines().at(0) ?? ""}`
    }
}

/// A BAML symbol as one line: how to name it, what it is, and enough of what it
/// does to judge. Ids rather than displays, because a claim has to be checkable
/// — displays collide, and matching them back was where the old pass leaked.
function baml_line(symbol: BamlSymbol) -> string {
    let signature = symbol.signature?.display ?? "";
    `- [${symbol.id}] ${symbol.kind} ${symbol.display}${signature}${first_line(symbol.doc)}`
}

function ts_line(symbol: TsSymbol) -> string {
    `- [${symbol.id}] ${symbol.signature.lines().at(0) ?? ""}${first_line(symbol.doc)}`
}

/// What a previous run concluded about a symbol that has since changed. Shown
/// so the model can say the conclusion still holds rather than starting over.
function prior_line(judgement: Judgement) -> string {
    let counterpart = judgement.ts ?? "nothing";
    `- [${judgement.baml}] was ${judgement.verdict} with ${counterpart} — ${judgement.reason ?? "no reason recorded"}`
}

// ── pass 1: propose ─────────────────────────────────────────────────────────

class Proposal {
    baml: string @description("the BAML id from inside [ ], exactly as written"),
    verdict: "match" | "divergent" | "none" | "bless",
    ts: string? @description("the TypeScript id from inside [ ]; omit for none and bless"),
    confidence: "high" | "medium" | "low",
    reason: string @description("under 15 words"),
    divergence: string? @description("how the two differ; only for divergent, under 20 words"),
}

class ProposeVerdict {
    proposals: Proposal[],
}

/// A pass's answer, or the reason there is none.
type PassResult = ProposeVerdict | PassFailure;

class ProposeRequest {
    owner: string,
    baml_lines: string[],
    ts_lines: string[],
    prior_lines: string[],
}

function propose_turns(request: ProposeRequest) -> Turn[] {
    let prior_section = if (request.prior_lines.length() > 0) {
        `

A previous run concluded the following. The symbols have changed since, so the
conclusions are up for review — answer "bless" if one still holds.
${request.prior_lines.join("\n")}`
    } else {
        ""
    };
    [
        Turn {
            role: "system",
            text: #"
                You are comparing two standard libraries: BAML and TypeScript.

                For each BAML symbol, decide whether the TypeScript candidates hold a
                counterpart. Judge behaviour, not spelling: `substring` and `slice` are the same
                operation; `push` and `pop` are not, despite both mutating an array.

                Expect the two libraries to reach the same operation differently, and record
                that rather than rejecting the pair. BAML puts static methods on primitives
                where TypeScript uses free functions, and reaches operators through interface
                impls where TypeScript uses built-in syntax. Those are "divergent", not "none".

                Answer once per BAML symbol, naming symbols by the id inside [ ]:
                  match      the same operation, reached the same way
                  divergent  the same operation, reached differently — say how
                  none       nothing offered does this symbol's job
                  bless      a previous conclusion, shown to you, still holds

                Use each TypeScript candidate at most once. A symbol with no counterpart is a
                real finding about the two libraries, not a failure to try harder — prefer
                "none" over a strained pairing.
            "#,
        },
        Turn {
            role: "user",
            text: `BAML symbols owned by ${request.owner}:
${request.baml_lines.join("\n")}

TypeScript candidates:
${request.ts_lines.join("\n")}${prior_section}`,
        },
    ]
}

function propose(transcript: string) -> ProposeVerdict {
    client: Aligner
    prompt: `
        ${transcript}

        ${ctx.output_format}
    `
}

/// One call, with its failure captured rather than flattened into silence.
function propose_once(request: ProposeRequest) -> PassResult {
    //# ask which of this owner's symbols have a counterpart in the pool
    propose(render_turns(propose_turns(request))) catch_all (error) {
        _ => PassFailure { pass: "propose", subject: request.owner, reason: `${error}` },
    }
}

// ── pass 2: sweep ───────────────────────────────────────────────────────────

class SweepRequest {
    container: string,
    ts_lines: string[],
    baml_lines: string[],
}

function sweep_turns(request: SweepRequest) -> Turn[] {
    [
        Turn {
            role: "system",
            text: #"
                You are comparing two standard libraries: BAML and TypeScript.

                This is the reverse direction: for each TypeScript symbol, decide whether any
                BAML candidate does its job. Everything here went unclaimed when the question
                was asked the other way round, usually because the BAML name gives nothing to
                search for — `baml.sys.exec` for `child_process.spawn`, `baml.id.new` for
                `crypto.randomUUID`.

                Judge behaviour, not spelling. Answer once per TypeScript symbol, naming symbols
                by the id inside [ ]:
                  match      the same operation, reached the same way
                  divergent  the same operation, reached differently — say how
                  none       no BAML candidate does this symbol's job

                TypeScript has plenty BAML has not, and saying so is the point of the exercise.
                Prefer "none" over a strained pairing.
            "#,
        },
        Turn {
            role: "user",
            text: `Unclaimed TypeScript symbols in ${request.container}:
${request.ts_lines.join("\n")}

BAML candidates:
${request.baml_lines.join("\n")}`,
        },
    ]
}

/// The sweep answers about TypeScript symbols, so a proposal's `baml` field
/// carries the TypeScript id and `ts` the BAML one. `apply_sweep` puts them
/// back the right way round.
function sweep(transcript: string) -> ProposeVerdict {
    client: Aligner
    prompt: `
        ${transcript}

        ${ctx.output_format}
    `
}

function sweep_once(request: SweepRequest) -> PassResult {
    //# ask which unclaimed TypeScript symbols have a BAML counterpart
    sweep(render_turns(sweep_turns(request))) catch_all (error) {
        _ => PassFailure { pass: "sweep", subject: request.container, reason: `${error}` },
    }
}

// ── pass 3: verify ──────────────────────────────────────────────────────────

class Ruling {
    baml: string @description("the BAML id from inside [ ]"),
    ts: string @description("the TypeScript id from inside [ ]"),
    upheld: bool,
    reason: string @description("under 20 words"),
    divergence: string? @description("how they differ, if it survives but is not exact"),
}

class VerifyVerdict {
    rulings: Ruling[],
}

type VerifyResult = VerifyVerdict | PassFailure;

class VerifyRequest {
    owner: string,
    pair_lines: string[],
}

function verify_turns(request: VerifyRequest) -> Turn[] {
    [
        Turn {
            role: "system",
            text: #"
                You are checking claimed equivalences between the BAML and TypeScript standard
                libraries. Each pairing below has been proposed by someone else. Your job is to
                try to refute it.

                A pairing survives only if the two really do the same job. Look for the ways
                that fails: different units, different edge-case behaviour, one throwing where
                the other returns null, one mutating where the other copies, one handling a case
                the other does not, a name that promises more than the signature delivers.

                Default to refuting when you are unsure. A wrong pairing is worse than a missing
                one — it is a claim about two libraries that someone will act on, and nothing
                downstream will question it.

                If a pairing survives but the two differ in a way a reader should know, say so
                as a divergence. Answer once per pairing.
            "#,
        },
        Turn {
            role: "user",
            text: `Claimed equivalences involving ${request.owner}:
${request.pair_lines.join("\n")}`,
        },
    ]
}

function verify(transcript: string) -> VerifyVerdict {
    client: Aligner
    prompt: `
        ${transcript}

        ${ctx.output_format}
    `
}

function verify_once(request: VerifyRequest) -> VerifyResult {
    //# try to refute each claimed equivalence
    verify(render_turns(verify_turns(request))) catch_all (error) {
        _ => PassFailure { pass: "verify", subject: request.owner, reason: `${error}` },
    }
}

// ── running the passes ──────────────────────────────────────────────────────

/// What a full alignment did, in numbers a reader can check against the report.
class AlignOutcome {
    matrix: SymbolMatrix,
    proposed: int,
    swept: int,
    upheld: int,
    refuted: int,
    /// Claims naming a symbol that was never offered, or a second claim on a
    /// symbol already spoken for. Counted by name so a drifting prompt is
    /// visible rather than merely quieter.
    rejected: string[],
}

function symbols_by_id(matrix: SymbolMatrix) -> map<string, BamlSymbol> {
    let index: map<string, BamlSymbol> = {};
    for (let symbol in matrix.baml) {
        let _ = index.set(symbol.id, symbol);
    }
    index
}

function ts_by_id(matrix: SymbolMatrix) -> map<string, TsSymbol> {
    let index: map<string, TsSymbol> = {};
    for (let symbol in matrix.ts) {
        let _ = index.set(symbol.id, symbol);
    }
    index
}

/// Builds one request per owner that has symbols left to judge and a pool to
/// judge them against. An owner whose pool is empty never becomes a request —
/// `record_empty_pools` answers for it without a call.
function propose_requests(
    matrix: SymbolMatrix,
    pools: CandidatePool[],
    revision: Revision?,
) -> ProposeRequest[] {
    let pending = pending_symbols(matrix);
    let by_owner: map<string, BamlSymbol[]> = {};
    for (let symbol in pending) {
        let owner = baml_owner_display(symbol);
        let bucket: BamlSymbol[] = by_owner.get(owner) ?? [];
        bucket.push(symbol);
        let _ = by_owner.set(owner, bucket);
    }

    let ts_index = ts_by_id(matrix);
    let requests: ProposeRequest[] = [];
    for (let pool in pools) {
        if (pool.ts_ids.length() == 0) {
            continue;
        }
        let members: BamlSymbol[] = by_owner.get(pool.owner) ?? [];
        let baml_lines: string[] = [];
        let prior_lines: string[] = [];
        // A pre-typed empty: `?? []` in an expression position yields
        // `Judgement[] | _[]` rather than unifying with the map's value type.
        let no_priors: Judgement[] = [];
        for (let member in members) {
            baml_lines.push(baml_line(member));
            if let before: Revision = revision {
                for (let judgement in before.judgements.get(member.id) ?? no_priors) {
                    prior_lines.push(prior_line(judgement));
                }
            }
        }
        let ts_lines: string[] = [];
        for (let id in pool.ts_ids) {
            if let symbol: TsSymbol = ts_index.get(id) {
                ts_lines.push(ts_line(symbol));
            }
        }
        requests
            .push(
                ProposeRequest {
                    owner: pool.owner,
                    baml_lines: baml_lines,
                    ts_lines: ts_lines,
                    prior_lines: prior_lines,
                },
            );
    }
    requests
}

/// An owner with no plausible counterpart is answered without asking.
///
/// This is the cheapest judgement in the pipeline and one of the most honest:
/// `baml.csv` has no TypeScript counterpart, the table says so, and spending a
/// call to be told the same thing would only add a way to be wrong.
function record_empty_pools(matrix: SymbolMatrix, pools: CandidatePool[]) -> int {
    let pending = pending_symbols(matrix);
    let by_owner: map<string, BamlSymbol[]> = {};
    for (let symbol in pending) {
        let owner = baml_owner_display(symbol);
        let bucket: BamlSymbol[] = by_owner.get(owner) ?? [];
        bucket.push(symbol);
        let _ = by_owner.set(owner, bucket);
    }
    let recorded = 0;
    let none: string[] = [];
    let no_members: BamlSymbol[] = [];
    for (let pool in pools) {
        if (pool.ts_ids.length() > 0) {
            continue;
        }
        for (let member in by_owner.get(pool.owner) ?? no_members) {
            matrix
                .judgements
                .push(
                    Judgement {
                        baml: member.id,
                        ts: null,
                        verdict: "none",
                        basis: "no-candidates",
                        confidence: null,
                        reason: `nothing in TypeScript corresponds to ${pool.owner}`,
                        divergence: null,
                        rejected: none,
                        verified: false,
                    },
                );
            recorded += 1;
        }
    }
    recorded
}

/// Folds proposals in, keeping only those that name symbols actually offered.
function apply_proposals(
    matrix: SymbolMatrix,
    results: PassResult[],
    revision: Revision?,
) -> AlignOutcome {
    let baml_index = symbols_by_id(matrix);
    let ts_index = ts_by_id(matrix);
    let judged: map<string, bool> = {};
    for (let judgement in matrix.judgements) {
        let _ = judged.set(judgement.baml, true);
    }

    let rejected: string[] = [];
    let applied = 0;
    let none: string[] = [];
    for (let result in results) {
        match (result) {
            let failure: PassFailure => {
                matrix.failures.push(failure);
            },
            let verdict: ProposeVerdict => {
                for (let proposal in verdict.proposals) {
                    if (!baml_index.has(proposal.baml)) {
                        rejected.push(`${proposal.baml} is not a BAML symbol`);
                        continue;
                    }
                    if (judged.has(proposal.baml)) {
                        rejected.push(`${proposal.baml} was already judged`);
                        continue;
                    }
                    let pairs = proposal.verdict == "match" || proposal.verdict == "divergent";
                    let counterpart = proposal.ts ?? "";
                    if (pairs && !ts_index.has(counterpart)) {
                        rejected.push(`${counterpart} is not a TypeScript symbol`);
                        continue;
                    }
                    if (proposal.verdict == "bless") {
                        let carried = carry_blessed(proposal, revision);
                        match (carried) {
                            null => {
                                rejected.push(`${proposal.baml} has no previous verdict to bless`);
                            },
                            let judgement: Judgement => {
                                let _ = judged.set(proposal.baml, true);
                                matrix.judgements.push(judgement);
                                applied += 1;
                            },
                        }
                        continue;
                    }
                    let _ = judged.set(proposal.baml, true);
                    matrix
                        .judgements
                        .push(
                            Judgement {
                                baml: proposal.baml,
                                ts: if (pairs) {
                                    counterpart
                                } else {
                                    null
                                },
                                verdict: proposal.verdict,
                                basis: "model",
                                confidence: proposal.confidence,
                                reason: proposal.reason,
                                divergence: proposal.divergence,
                                rejected: none,
                                verified: false,
                            },
                        );
                    applied += 1;
                }
            },
        }
    }

    matrix.counts = recount(matrix);
    AlignOutcome {
        matrix: matrix,
        proposed: applied,
        swept: 0,
        upheld: 0,
        refuted: 0,
        rejected: rejected,
    }
}

/// A blessed proposal takes the previous verdict whole, and records that a
/// model looked at it again. The pairing is the old one; only the endorsement
/// is new.
function carry_blessed(proposal: Proposal, revision: Revision?) -> Judgement? {
    let out: Judgement? = null;
    let no_priors: Judgement[] = [];
    if let before: Revision = revision {
        if let prior: Judgement = (before.judgements.get(proposal.baml) ?? no_priors).at(0) {
            out
                = Judgement {
                    baml: prior.baml,
                    ts: prior.ts,
                    verdict: prior.verdict,
                    basis: "model",
                    confidence: proposal.confidence,
                    reason: `unchanged in substance: ${proposal.reason}`,
                    divergence: prior.divergence,
                    rejected: prior.rejected,
                    verified: true,
                };
        }
    }
    out
}

/// One request per container that still holds unclaimed symbols.
function sweep_requests(matrix: SymbolMatrix) -> SweepRequest[] {
    let claimed: map<string, bool> = {};
    for (let judgement in matrix.judgements) {
        if (judgement_pairs(judgement)) {
            if let ts: string = judgement.ts {
                let _ = claimed.set(ts, true);
            }
        }
    }

    let by_container: map<string, TsSymbol[]> = {};
    let order: string[] = [];
    for (let symbol in matrix.ts) {
        if (claimed.has(symbol.id)) {
            continue;
        }
        let container = symbol.symbol.at(0) ?? "";
        if (!by_container.has(container)) {
            let empty: TsSymbol[] = [];
            let _ = by_container.set(container, empty);
            order.push(container);
        }
        if let bucket: TsSymbol[] = by_container.get(container) {
            bucket.push(symbol);
        }
    }

    let candidates = unpaired_symbols(matrix);
    let scope = sweep_scope(candidates);
    let requests: SweepRequest[] = [];
    for (let container in order) {
        let members: TsSymbol[] = by_container.get(container) ?? [];
        let offered = baml_candidates_for(candidates, container, members, scope);
        if (offered.length() == 0) {
            continue;
        }
        let ts_lines: string[] = [];
        for (let member in members) {
            ts_lines.push(ts_line(member));
        }
        let baml_lines: string[] = [];
        for (let candidate in offered) {
            baml_lines.push(baml_line(candidate));
        }
        requests
            .push(SweepRequest { container: container, ts_lines: ts_lines, baml_lines: baml_lines });
    }
    requests
}

/// The pool, inverted: which BAML symbols could answer for this container.
///
/// The same two sources as the forward pool — the correspondence table, read
/// backwards, and a lexical shortlist — so the two directions agree about what
/// is in scope even though they ask different questions.
function baml_candidates_for(
    universe: BamlSymbol[],
    container: string,
    members: TsSymbol[],
    scope: SweepScope,
) -> BamlSymbol[] {
    let candidates: BamlSymbol[] = [];
    let taken: map<string, bool> = {};
    let index = 0;
    for (let symbol in universe) {
        // `scope.keys` is empty for the kinds a TypeScript member could never
        // correspond to — a declaration is not an operation.
        let eligible = (scope.keys.at(index) ?? "").length() > 0;
        if (eligible && (scope.containers.at(index)?.includes(container) ?? false)) {
            let _ = taken.set(symbol.id, true);
            candidates.push(symbol);
        }
        index += 1;
    }
    let member_keys = ts_match_keys(members);
    let scan = 0;
    for (let symbol in universe) {
        let key = scope.keys.at(scan) ?? "";
        scan += 1;
        if (taken.has(symbol.id) || key.length() == 0) {
            continue;
        }
        if (candidates.length() >= pool_cap()) {
            continue;
        }
        let shares = false;
        for (let member_key in member_keys) {
            if (!shares && stems_share(key, member_key)) {
                shares = true;
            }
        }
        if (shares) {
            let _ = taken.set(symbol.id, true);
            candidates.push(symbol);
        }
    }
    candidates
}

/// The BAML side, normalized once for the whole sweep.
///
/// Every container asks the same two questions of every BAML symbol — which
/// containers does its owner correspond to, and what is its match key — and
/// answering them per container is what made this pass quadratic. Types and
/// variants are excluded here rather than per container: neither is something a
/// TypeScript member could be.
class SweepScope {
    containers: string[][],
    keys: string[],
}

function sweep_scope(universe: BamlSymbol[]) -> SweepScope {
    let by_owner: map<string, string[]> = {};
    let containers: string[][] = [];
    let keys: string[] = [];
    let no_containers: string[] = [];
    for (let symbol in universe) {
        let owner = baml_owner_display(symbol);
        if (!by_owner.has(owner)) {
            let _ = by_owner.set(owner, pool_containers(owner));
        }
        containers.push(by_owner.get(owner) ?? no_containers);
        keys.push(
            if (symbol.origin == "type" || symbol.origin == "variant") {
                ""
            } else {
                match_key(baml_name_of(symbol))
            },
        );
    }
    SweepScope { containers: containers, keys: keys }
}

/// Folds the sweep in. Its answers are about TypeScript symbols, so the sides
/// arrive swapped: `baml` holds the TypeScript id it was asked about, `ts` the
/// BAML symbol it proposes.
function apply_sweep(matrix: SymbolMatrix, results: PassResult[]) -> AlignOutcome {
    let baml_index = symbols_by_id(matrix);
    let ts_index = ts_by_id(matrix);
    let paired: map<string, bool> = {};
    let claimed: map<string, bool> = {};
    for (let judgement in matrix.judgements) {
        if (judgement_pairs(judgement)) {
            let _ = paired.set(judgement.baml, true);
            if let ts: string = judgement.ts {
                let _ = claimed.set(ts, true);
            }
        }
    }
    // A symbol the forward pass judged to have no counterpart may be overturned
    // here: that judgement was made against a pool which evidently did not hold
    // the answer, and finding it from the other side is what this pass is for.
    let overturned: map<string, bool> = {};

    let rejected: string[] = [];
    let applied = 0;
    let none: string[] = [];
    for (let result in results) {
        match (result) {
            let failure: PassFailure => {
                matrix.failures.push(failure);
            },
            let verdict: ProposeVerdict => {
                for (let proposal in verdict.proposals) {
                    let ts_id = proposal.baml;
                    let baml_id = proposal.ts ?? "";
                    if (proposal.verdict == "none" || proposal.verdict == "bless") {
                        // The sweep only adds pairings. A TypeScript symbol with
                        // no BAML counterpart is already recorded by absence.
                        continue;
                    }
                    if (!ts_index.has(ts_id)) {
                        rejected.push(`${ts_id} is not a TypeScript symbol`);
                        continue;
                    }
                    if (!baml_index.has(baml_id)) {
                        rejected.push(`${baml_id} is not a BAML symbol`);
                        continue;
                    }
                    if (paired.has(baml_id)) {
                        rejected.push(`${baml_id} already has a counterpart`);
                        continue;
                    }
                    if (claimed.has(ts_id)) {
                        rejected.push(`${ts_id} was already claimed`);
                        continue;
                    }
                    let _ = paired.set(baml_id, true);
                    let _ = claimed.set(ts_id, true);
                    let _ = overturned.set(baml_id, true);
                    matrix
                        .judgements
                        .push(
                            Judgement {
                                baml: baml_id,
                                ts: ts_id,
                                verdict: proposal.verdict,
                                basis: "model",
                                confidence: proposal.confidence,
                                reason: proposal.reason,
                                divergence: proposal.divergence,
                                rejected: none,
                                verified: false,
                            },
                        );
                    applied += 1;
                }
            },
        }
    }

    // Drop the absences this pass overturned, keeping the pairing that replaced
    // them. Leaving both would have one symbol claiming a counterpart and
    // denying one in the same report.
    let kept: Judgement[] = [];
    for (let judgement in matrix.judgements) {
        if (!judgement_pairs(judgement) && overturned.has(judgement.baml)) {
            continue;
        }
        kept.push(judgement);
    }
    matrix.judgements = kept;
    matrix.counts = recount(matrix);
    AlignOutcome {
        matrix: matrix,
        proposed: 0,
        swept: applied,
        upheld: 0,
        refuted: 0,
        rejected: rejected,
    }
}

/// One request per owner whose pairings have not been checked. Name matches go
/// in alongside model proposals: name agreement is a heuristic, and this is the
/// pass that decides whether to believe it.
function verify_requests(matrix: SymbolMatrix) -> VerifyRequest[] {
    let baml_index = symbols_by_id(matrix);
    let ts_index = ts_by_id(matrix);
    let by_owner: map<string, string[]> = {};
    let order: string[] = [];
    for (let judgement in matrix.judgements) {
        if (!judgement_pairs(judgement) || judgement.verified) {
            continue;
        }
        if let symbol: BamlSymbol = baml_index.get(judgement.baml) {
            if let counterpart: TsSymbol = ts_index.get(judgement.ts ?? "") {
                let owner = baml_owner_display(symbol);
                if (!by_owner.has(owner)) {
                    let empty: string[] = [];
                    let _ = by_owner.set(owner, empty);
                    order.push(owner);
                }
                if let bucket: string[] = by_owner.get(owner) {
                    bucket.push(pair_line(symbol, counterpart, judgement));
                }
            }
        }
    }

    let requests: VerifyRequest[] = [];
    for (let owner in order) {
        let lines: string[] = by_owner.get(owner) ?? [];
        requests.push(VerifyRequest { owner: owner, pair_lines: lines });
    }
    requests
}

function pair_line(symbol: BamlSymbol, counterpart: TsSymbol, judgement: Judgement) -> string {
    let claim = judgement.reason ?? "names and owners agree";
    [
        `- [${symbol.id}] ${symbol.display}${symbol.signature?.display ?? ""}`,
        `  ↔ [${counterpart.id}] ${counterpart.signature.lines().at(0) ?? ""}`,
        `  claimed because: ${claim}`,
    ]
    .join("\n")
}

/// Folds rulings in.
///
/// A refuted pairing becomes a recorded absence carrying the counterpart that
/// was refused, so the next run does not propose it again. That is the whole
/// reason to record a refutation rather than simply dropping the pairing.
function apply_rulings(matrix: SymbolMatrix, results: VerifyResult[]) -> AlignOutcome {
    let rulings: map<string, Ruling> = {};
    for (let result in results) {
        match (result) {
            let failure: PassFailure => {
                matrix.failures.push(failure);
            },
            let verdict: VerifyVerdict => {
                for (let ruling in verdict.rulings) {
                    let _ = rulings.set(`${ruling.baml}\u{1f}${ruling.ts}`, ruling);
                }
            },
        }
    }

    let upheld = 0;
    let refuted = 0;
    let kept: Judgement[] = [];
    for (let judgement in matrix.judgements) {
        let key = `${judgement.baml}\u{1f}${judgement.ts ?? ""}`;
        match (rulings.get(key)) {
            null => {
                kept.push(judgement);
            },
            let ruling: Ruling => {
                if (ruling.upheld) {
                    judgement.verified = true;
                    if let divergence: string = ruling.divergence {
                        judgement.verdict = "divergent";
                        judgement.divergence = divergence;
                    }
                    kept.push(judgement);
                    upheld += 1;
                } else {
                    let refused = judgement.rejected;
                    if let counterpart: string = judgement.ts {
                        refused.push(counterpart);
                    }
                    kept.push(
                        Judgement {
                            baml: judgement.baml,
                            ts: null,
                            verdict: "none",
                            basis: "model",
                            confidence: null,
                            reason: `refuted: ${ruling.reason}`,
                            divergence: null,
                            rejected: refused,
                            verified: true,
                        },
                    );
                    refuted += 1;
                }
            },
        }
    }
    matrix.judgements = kept;
    matrix.counts = recount(matrix);
    AlignOutcome {
        matrix: matrix,
        proposed: 0,
        swept: 0,
        upheld: upheld,
        refuted: refuted,
        rejected: [],
    }
}

/// Judgements in a fixed order, so a report diffs against its predecessor
/// rather than against the order three concurrent passes happened to finish in.
function sort_judgements(matrix: SymbolMatrix) -> null {
    matrix.judgements.sort_by_key((judgement) -> {
        `${judgement.baml}\u{1f}${judgement.ts ?? ""}`
    });
    null
}

/// How many calls may be in flight at once.
///
/// A cold run is a few hundred calls that would otherwise all start at the same
/// instant, which is the shape of traffic a provider rate-limits hardest. The
/// cap is deliberately modest: the passes are wide rather than deep, so the run
/// is bounded by the slowest call in each batch either way, and a 429 costs a
/// retry with backoff that would have taken longer than waiting.
function call_limit() -> int {
    8
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

/// Runs a pass's calls with a bounded number in flight, reporting as they land.
///
/// The group caps concurrency; the spawn still returns immediately and queues
/// FIFO, so this reads exactly like the unthrottled version. Each result is
/// awaited in order rather than through `future.all`, which is what makes
/// progress reportable at all — `all` yields once, at the end.
function run_propose_pass(pass: string, requests: ProposeRequest[]) -> PassResult[] {
    let group = baml.spawn.TaskGroup.new(call_limit(), name = pass);
    let pending = requests.map((request) -> {
        spawn with baml.spawn.options(group = group) { propose_once(request) }
    });
    report_progress(`${pass}: ${requests.length()} calls, ${call_limit()} at a time`);
    let results: PassResult[] = [];
    for (let future in pending) {
        results.push(await future);
        if (results.length() % 10 == 0 || results.length() == pending.length()) {
            report_progress(`${pass}: ${results.length()}/${pending.length()}`);
        }
    }
    results
}

function run_sweep_pass(requests: SweepRequest[]) -> PassResult[] {
    let group = baml.spawn.TaskGroup.new(call_limit(), name = "sweep");
    let pending = requests.map((request) -> {
        spawn with baml.spawn.options(group = group) { sweep_once(request) }
    });
    report_progress(`sweep: ${requests.length()} calls, ${call_limit()} at a time`);
    let results: PassResult[] = [];
    for (let future in pending) {
        results.push(await future);
        if (results.length() % 10 == 0 || results.length() == pending.length()) {
            report_progress(`sweep: ${results.length()}/${pending.length()}`);
        }
    }
    results
}

function run_verify_pass(requests: VerifyRequest[]) -> VerifyResult[] {
    let group = baml.spawn.TaskGroup.new(call_limit(), name = "verify");
    let pending = requests.map((request) -> {
        spawn with baml.spawn.options(group = group) { verify_once(request) }
    });
    report_progress(`verify: ${requests.length()} calls, ${call_limit()} at a time`);
    let results: VerifyResult[] = [];
    for (let future in pending) {
        results.push(await future);
        if (results.length() % 10 == 0 || results.length() == pending.length()) {
            report_progress(`verify: ${results.length()}/${pending.length()}`);
        }
    }
    results
}

/// Runs all three passes over a matrix, in order.
function align_matrix(matrix: SymbolMatrix, revision: Revision?) -> AlignOutcome {
    // Built once: every owner's pool is drawn from the same normalized view of
    // the TypeScript surface.
    report_progress("building pools");
    let pools = build_pools(matrix, pending_symbols(matrix));
    let requests = propose_requests(matrix, pools, revision);
    let empty_pools = record_empty_pools(matrix, pools);
    report_progress(`${empty_pools} symbols answered without a call (no candidates)`);

    let proposed = apply_proposals(matrix, run_propose_pass("propose", requests), revision);
    report_progress(`propose: ${proposed.proposed} judged, ${proposed.rejected.length()} claims refused`);

    let sweeps = run_sweep_pass(sweep_requests(proposed.matrix));
    let swept = apply_sweep(proposed.matrix, sweeps);
    report_progress(`sweep: ${swept.swept} judged, ${swept.rejected.length()} claims refused`);

    let rulings = run_verify_pass(verify_requests(swept.matrix));
    let verified = apply_rulings(swept.matrix, rulings);
    report_progress(`verify: ${verified.upheld} upheld, ${verified.refuted} refuted`);
    if (verified.matrix.failures.length() > 0) {
        report_progress(`${verified.matrix.failures.length()} calls did not come back`);
    }

    sort_judgements(verified.matrix);
    let rejected: string[] = [];
    for (let name in proposed.rejected) {
        rejected.push(name);
    }
    for (let name in swept.rejected) {
        rejected.push(name);
    }
    AlignOutcome {
        matrix: verified.matrix,
        proposed: proposed.proposed + empty_pools,
        swept: swept.swept,
        upheld: verified.upheld,
        refuted: verified.refuted,
        rejected: rejected,
    }
}
