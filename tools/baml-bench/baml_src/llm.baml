// The LLM judgments, as typed BAML functions (agent-tries-baml pattern: return
// type IS the schema via {{ ctx.output_format }}).
//
// Two merged calls instead of four:
//   AnalyzeChunk = intention + routing (one look at the chunk, GraderFast/Haiku)
//   GradeCards   = every routed card graded in one call (Grader/Sonnet)
// RefuteFinding and ScanOmissions stay separate (different inputs).
//
// Each has a cached_* wrapper: content-addressed cache hit -> decode, miss ->
// call the model, deterministically validate, store. Bump prompt_version() in
// types.baml when editing any prompt here.

// ---- stages (c)+(d): intention + routing, merged ----------------------------

function AnalyzeChunk(
    chunk_kind: string,
    chunk_source: string,
    neighbors: string,
    index: string,
) -> ChunkAnalysis {
    client: GraderFast
    prompt: #"
        You are analyzing one chunk of a BAML codebase (BAML is a typed language
        for LLM functions). Two jobs in one pass:

        1. State what the author is trying to achieve (`goal`) and how the code
           actually does it (`mechanism`). One sentence each, cleanly separated —
           the gap between a legitimate goal and a poor mechanism is what a later
           reviewer grades. `confidence` is 1-5.

        2. Route design-principle cards: below is an index where each line is
           `<card_id> [<status>] <title> | trigger: <applicability trigger>`.
           Return in `card_ids` the ids of every card whose trigger genuinely
           matches this chunk — the cards a reviewer should grade this chunk
           against. Be selective: a card applies only when the chunk actually
           does the thing the trigger describes, not merely mentions a related
           word. An empty list is a fine answer. Never invent ids.

        # chunk kind
        {{ chunk_kind }}

        # chunk source
        {{ chunk_source }}

        # 1-hop neighborhood (how this chunk is wired to the rest of the project)
        {{ neighbors }}

        # card index
        {{ index }}

        {{ ctx.output_format }}
    "#
}

// Returns the analysis with card_ids filtered to real ids (deterministic gate),
// deduped, sorted.
function cached_analyze(
    cache_dir: string,
    chunk: Chunk,
    neighbors: string,
    pre: Prefilter,
) -> ChunkAnalysis {
    // The LLM router only arbitrates the prefilter's candidates; statically
    // routed cards are merged in afterwards, deterministically.
    let index = card_index(pre.candidates);
    let static_key = pre.static_ids.join(",");
    let key = cache_key(
        "analyze@" + fast_model(),
        chunk.id + "\n" + chunk.source + "\n" + neighbors + "\n" + static_key + "\n" + index,
    );
    let raw = match (cache_get(cache_dir, key)) {
        let hit: string => {
            baml.json.from_string<ChunkAnalysis>(hit) catch (_) {
                _ => fresh_analyze(cache_dir, key, chunk, neighbors, index)
            }
        },
        null => fresh_analyze(cache_dir, key, chunk, neighbors, index),
    };
    return ChunkAnalysis {
        goal: raw.goal,
        mechanism: raw.mechanism,
        confidence: raw.confidence.clamp(1, 5),
        card_ids: merge_routed(pre, raw.card_ids),
    };
}

// Deterministic post-validation: LLM-proposed ids are kept only when they are
// real candidates (low-confidence gate matches the router confirmed); static
// routes are unioned in. Deduped, sorted.
function merge_routed(pre: Prefilter, raw_ids: string[]) -> string[] {
    let candidate_ids: map<string, bool> = {};
    for (let c in pre.candidates) {
        candidate_ids.set(c.id, true);
    }
    let out: string[] = [];
    let seen: map<string, bool> = {};
    for (let id in pre.static_ids) {
        if (!seen.has(id)) {
            out.push(id);
            seen.set(id, true);
        }
    }
    for (let id in raw_ids) {
        if (candidate_ids.has(id) && !seen.has(id)) {
            out.push(id);
            seen.set(id, true);
        }
    }
    out.sort_by_key(
        (s: string) -> string {
            s
        },
    )
}

function fresh_analyze(
    cache_dir: string,
    key: string,
    chunk: Chunk,
    neighbors: string,
    index: string,
) -> ChunkAnalysis {
    let out = AnalyzeChunk(chunk.kind, truncate(chunk.source, 6000), neighbors, index);
    store_json(cache_dir, key, out.to_json());
    out
}

// ---- stage (e): grading, one call per chunk over all its routed cards -------

function GradeCards(
    card_bodies: string,
    chunk_source: string,
    goal: string,
    mechanism: string,
    neighbors: string,
) -> MultiGradeOut {
    client: Grader
    prompt: #"
        You are grading whether one chunk of a BAML codebase uses language
        features the way the design docs intend. Below: several principle cards
        (distilled from BAML Enhancement Proposals, with intended usage and known
        anti-patterns), then the chunk. Grade the chunk against EACH card —
        return one entry per card, with `card_id` copied exactly from the card's
        heading.

        Anchored scale — grade against it exactly:
        - 9-10 adherent: uses the primitive as the card intends; reads like the
          card's intended-usage example.
        - 7-8 adherent: right primitive, minor deviation from the intended form.
        - 5-6 neutral: the card applies but the code neither exploits nor fights
          the design.
        - 3-4 fighting: works around the primitive (one of the card's
          anti-patterns, or an equivalent workaround).
        - 1-2 reinventing: reimplements the primitive by hand.

        Rules, per entry:
        - `evidence` MUST be a verbatim quote copied from the chunk source below
          (it is checked mechanically against the source; a grade without a real
          quote is discarded). Quote the single most load-bearing line(s) for
          THAT card.
        - `verdict` must be consistent with the grade band above.
        - `suggested_form`: only for grades <= 6 — one short sketch of what the
          intended usage would look like here; otherwise null.
        - Grade each card independently; do not let one card's verdict bleed
          into another's. If a card's feature is not available on the project's
          toolchain and the code uses a reasonable substitute, that is neutral
          (5-6), not fighting.

        # principle cards
        {{ card_bodies }}

        # chunk goal / mechanism
        goal: {{ goal }}
        mechanism: {{ mechanism }}

        # chunk source
        {{ chunk_source }}

        # 1-hop neighborhood
        {{ neighbors }}

        {{ ctx.output_format }}
    "#
}

// One grading call for `cards` (the chunk's routed cards). Returns one
// CardGrade per requested card: model output is validated against the request —
// unknown ids dropped, missing ids filled with a neutral placeholder, grades
// clamped.
function cached_grades(
    cache_dir: string,
    chunk: Chunk,
    cards: PrincipleCard[],
    analysis: ChunkAnalysis,
    neighbors: string,
) -> CardGrade[] {
    let ids_key = "";
    let bodies = "";
    for (let c in cards) {
        ids_key = ids_key + c.id + ",";
        bodies = bodies + c.body + "\n---\n";
    }
    let key = cache_key("grade@" + judge_model(), ids_key + "\n" + bodies + "\n" + chunk.id + "\n" + chunk.source);
    let raw = match (cache_get(cache_dir, key)) {
        let hit: string => {
            baml.json.from_string<MultiGradeOut>(hit) catch (_) {
                _ => fresh_grades(cache_dir, key, chunk, bodies, analysis, neighbors)
            }
        },
        null => fresh_grades(cache_dir, key, chunk, bodies, analysis, neighbors),
    };
    // deterministic reconciliation against the requested card list
    let by_id: map<string, CardGrade> = {};
    for (let g in raw.grades) {
        if (!by_id.has(g.card_id)) {
            by_id.set(g.card_id, g);
        }
    }
    let out: CardGrade[] = [];
    for (let c in cards) {
        match (by_id.get(c.id)) {
            let g: CardGrade => {
                let clamped = g.grade.clamp(1, 10);
                out.push(
                    CardGrade {
                        card_id: c.id,
                        grade: clamped,
                        // never trust the model's verdict string — derive it
                        // from the grade band (models drift into prose here)
                        verdict: verdict_of(clamped),
                        evidence: g.evidence,
                        suggested_form: g.suggested_form,
                    },
                );
            },
            null => {
                // model skipped this card: neutral, no evidence (fails the gate
                // downstream, so it can never contribute a misuse verdict)
                out.push(
                    CardGrade {
                        card_id: c.id,
                        grade: 5,
                        verdict: "neutral",
                        evidence: "",
                        suggested_form: null,
                    },
                );
            },
        }
    }
    out
}

function fresh_grades(
    cache_dir: string,
    key: string,
    chunk: Chunk,
    bodies: string,
    analysis: ChunkAnalysis,
    neighbors: string,
) -> MultiGradeOut {
    let out = GradeCards(bodies, truncate(chunk.source, 6000), analysis.goal, analysis.mechanism, neighbors);
    store_json(cache_dir, key, out.to_json());
    out
}

// ---- stage (f): refutation ---------------------------------------------------

function RefuteFinding(
    card_body: string,
    chunk_source: string,
    grade: int,
    verdict: string,
    evidence: string,
) -> RefuteOut {
    client: Grader
    prompt: #"
        You are a skeptic reviewing a low adherence grade before it reaches the
        report. A reviewer claimed the chunk below fights or reinvents a designed
        BAML primitive. Try to REFUTE the claim: is there a legitimate reason this
        code cannot use the primitive — a host-boundary constraint, the feature
        not shipped on the project's pinned toolchain, a wire-format requirement,
        a deliberate documented trade-off visible in the code or its comments?

        Refute ONLY with a concrete reason grounded in the card or the code.
        "The code works anyway" is not a refutation — the claim is about design
        adherence, not correctness. If the finding stands, refuted=false.

        # the claim
        grade {{ grade }} ({{ verdict }}), evidence: {{ evidence }}

        # principle card
        {{ card_body }}

        # chunk source
        {{ chunk_source }}

        {{ ctx.output_format }}
    "#
}

function cached_refute(
    cache_dir: string,
    chunk: Chunk,
    card: PrincipleCard,
    grade: CardGrade,
) -> RefuteOut {
    let key = cache_key(
        "refute@" + judge_model(),
        card.id
            + "\n"
            + chunk.id
            + "\n"
            + chunk.source
            + "\n"
            + i2s(grade.grade)
            + "\n"
            + grade.evidence,
    );
    match (cache_get(cache_dir, key)) {
        let hit: string => {
            baml.json.from_string<RefuteOut>(hit) catch (_) {
                _ => fresh_refute(cache_dir, key, chunk, card, grade)
            }
        },
        null => fresh_refute(cache_dir, key, chunk, card, grade),
    }
}

function fresh_refute(
    cache_dir: string,
    key: string,
    chunk: Chunk,
    card: PrincipleCard,
    grade: CardGrade,
) -> RefuteOut {
    let out = RefuteFinding(card.body, truncate(chunk.source, 6000), grade.grade, grade.verdict, grade.evidence);
    store_json(cache_dir, key, out.to_json());
    out
}

// ---- stage (g): omission scan --------------------------------------------------

function ScanOmissions(intent_table: string, index: string) -> OmissionScanOut {
    client: Grader
    prompt: #"
        You are looking for design-adherence OMISSIONS in a BAML codebase: cases
        where the codebase clearly has goals a designed primitive exists for, yet
        never reaches for that primitive anywhere. Omissions are the strongest
        slop signal — the codebase that hand-rolls everything triggers no
        per-chunk finding because the primitive never appears.

        Below: the intention table (every chunk's goal and mechanism) and the
        card index. Report a card ONLY when (a) several chunk goals plainly call
        for its primitive, and (b) no mechanism in the table uses it. `evidence`
        names the chunk ids whose goals show the need. Return an empty list when
        the codebase is small or the primitives are genuinely not needed. Never
        invent card ids.

        # intention table (chunk_id | kind | goal | mechanism)
        {{ intent_table }}

        # card index
        {{ index }}

        {{ ctx.output_format }}
    "#
}

function cached_omissions(
    cache_dir: string,
    intent_table: string,
    cards: PrincipleCard[],
) -> OmissionOut[] {
    let index = card_index(cards);
    let key = cache_key("omissions@" + judge_model(), intent_table + "\n" + index);
    let raw = match (cache_get(cache_dir, key)) {
        let hit: string => {
            baml.json.from_string<OmissionScanOut>(hit) catch (_) {
                _ => fresh_omissions(cache_dir, key, intent_table, index)
            }
        },
        null => fresh_omissions(cache_dir, key, intent_table, index),
    };
    let known: map<string, bool> = {};
    for (let c in cards) {
        known.set(c.id, true);
    }
    raw.omissions
        .filter(
            (o: OmissionOut) -> bool {
                known.has(o.card_id)
            },
        )
        .sort_by_key(
            (o: OmissionOut) -> string {
                o.card_id
            },
        )
}

function fresh_omissions(
    cache_dir: string,
    key: string,
    intent_table: string,
    index: string,
) -> OmissionScanOut {
    let out = ScanOmissions(intent_table, index);
    store_json(cache_dir, key, out.to_json());
    out
}

// ---- shared ------------------------------------------------------------------

function store_json(cache_dir: string, key: string, value: json) -> null {
    let text = baml.json.to_string(value) catch (_) {
        _ => ""
    };
    if (text != "") {
        cache_put(cache_dir, key, text);
    }
    null
}
