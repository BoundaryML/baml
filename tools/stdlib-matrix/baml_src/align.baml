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
    let counterpart = if (judgement.baml.length() > 0) {
        judgement.baml.join(", ")
    } else {
        "nothing"
    };
    `- [${judgement.ts}] was ${judgement.verdict} with ${counterpart} — ${judgement.reason ?? "no reason recorded"}`
}

// ── indexes ─────────────────────────────────────────────────────────────────

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

/// Judgements in a fixed order, so a report diffs against its predecessor
/// rather than against the order concurrent sessions happened to finish in.
function sort_judgements(matrix: SymbolMatrix) -> null {
    matrix.judgements.sort_by_key((judgement) -> {
        `${judgement.ts}\u{1f}${judgement.baml.join(",")}`
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
