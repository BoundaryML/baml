// Stages (e)-(g) — grading, refutation, omission scan, on the judge tier
// (Grader/Sonnet). One GradeJob = one subject (CodeBlock OR InteractionBlock)
// × one batch of ≤12 routed cards, so prompts stay bounded and the judge pool
// parallelizes freely. Since routing is deterministic (no LLM router), the
// grading prompt itself arbitrates borderline routes: a genuinely
// inapplicable card grades neutral with empty evidence, and the mechanical
// evidence gate keeps it out of every misuse verdict.

function grade_batch_size() -> int {
    return 12;
}

// One judge-pool work item.
class GradeJob {
    subject_id: string,
    subject_kind: string, // block kind, or interaction kind (by_depth/by_level/reference)
    is_interaction: bool,
    graded_source: string, // block source (truncated) or two-endpoint interaction source
    intention_text: string, // rendered intention lines for the prompt + cache key
    neighbors: string,
    span_lines: int, // for size_factor; 0 for interactions
    cards: PrincipleCard[],
}

// ---- stage (e): grading ------------------------------------------------------------

function GradeCards(
    card_bodies: string,
    subject_kind: string,
    subject_source: string,
    intention: string,
    neighbors: string,
) -> MultiGradeOut {
    client: Grader
    prompt: #"
        You are grading whether one block of a BAML codebase uses language
        features the way the design docs intend. Below: several principle cards
        (distilled from BAML Enhancement Proposals, with intended usage and
        known anti-patterns), then the graded subject — a code block, or a PAIR
        of blocks when the subject is an interaction (how two blocks relate).
        Grade the subject against EACH card — return one entry per card, with
        `card_id` copied exactly from the card's heading.

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
        - Routing here is keyword-based, so a card may not truly apply. If this
          card genuinely does not apply to this subject, grade 5 with EMPTY
          evidence — do not stretch a card to fit.
        - `evidence` MUST be a verbatim quote copied from the subject source
          below (it is checked mechanically; a grade whose quote is not real is
          discarded). Quote the single most load-bearing line(s) for THAT card.
        - `verdict` must be consistent with the grade band above.
        - `suggested_form`: only for grades <= 6 — one short sketch of what the
          intended usage would look like here; otherwise null.
        - Grade each card independently. If a card's feature is not available
          on the project's toolchain and the code uses a reasonable substitute,
          that is neutral (5-6), not fighting.
        - For an interaction subject, grade the RELATION (how block A uses,
          contains, or sits beside block B), not each block in isolation.

        # principle cards
        {{ card_bodies }}

        # subject kind
        {{ subject_kind }}

        # subject intention (inferred goal / mechanism)
        {{ intention }}

        # subject source
        {{ subject_source }}

        # reference neighborhood
        {{ neighbors }}

        {{ ctx.output_format }}
    "#
}

// One grading call for a job's card batch. Model output is validated against
// the request — unknown ids dropped, missing ids filled neutral, grades
// clamped, verdicts derived from the grade band.
function cached_grades(cache_dir: string, job: GradeJob) -> CardGrade[] {
    let ids_key = "";
    let bodies = "";
    for (let c in job.cards) {
        ids_key = ids_key + c.id + ",";
        bodies = bodies + c.body + "\n---\n";
    }
    let key = cache_key(
        "grade@" + judge_model(),
        ids_key
            + "\n"
            + bodies
            + "\n"
            + job.subject_id
            + "\n"
            + job.graded_source
            + "\n"
            + job.intention_text,
    );
    let raw = match (cache_get(cache_dir, key)) {
        let hit: string => {
            baml.json.from_string<MultiGradeOut>(hit) catch (_) {
                _ => fresh_grades(cache_dir, key, job, bodies)
            }
        },
        null => fresh_grades(cache_dir, key, job, bodies),
    };
    let by_id: map<string, CardGrade> = {};
    for (let g in raw.grades) {
        if (!by_id.has(g.card_id)) {
            by_id.set(g.card_id, g);
        }
    }
    let out: CardGrade[] = [];
    for (let c in job.cards) {
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
                // model skipped this card: neutral, no evidence (fails the
                // gate downstream, so it can never contribute a misuse verdict)
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

function fresh_grades(cache_dir: string, key: string, job: GradeJob, bodies: string) -> MultiGradeOut {
    let out = GradeCards(bodies, job.subject_kind, job.graded_source, job.intention_text, job.neighbors);
    store_json(cache_dir, key, out.to_json());
    out
}

// ---- stage (f): refutation ------------------------------------------------------------

function RefuteFinding(
    card_body: string,
    subject_source: string,
    grade: int,
    verdict: string,
    evidence: string,
) -> RefuteOut {
    client: Grader
    prompt: #"
        You are a skeptic reviewing a low adherence grade before it reaches the
        report. A reviewer claimed the subject below fights or reinvents a
        designed BAML primitive. Try to REFUTE the claim: is there a legitimate
        reason this code cannot use the primitive — a host-boundary constraint,
        the feature not shipped on the project's pinned toolchain, a wire-format
        requirement, a deliberate documented trade-off visible in the code or
        its comments?

        Refute ONLY with a concrete reason grounded in the card or the code.
        "The code works anyway" is not a refutation — the claim is about design
        adherence, not correctness. If the finding stands, refuted=false.

        # the claim
        grade {{ grade }} ({{ verdict }}), evidence: {{ evidence }}

        # principle card
        {{ card_body }}

        # subject source
        {{ subject_source }}

        {{ ctx.output_format }}
    "#
}

function cached_refute(
    cache_dir: string,
    subject_id: string,
    graded_source: string,
    card: PrincipleCard,
    grade: CardGrade,
) -> RefuteOut {
    let key = cache_key(
        "refute@" + judge_model(),
        card.id
            + "\n"
            + subject_id
            + "\n"
            + graded_source
            + "\n"
            + i2s(grade.grade)
            + "\n"
            + grade.evidence,
    );
    match (cache_get(cache_dir, key)) {
        let hit: string => {
            baml.json.from_string<RefuteOut>(hit) catch (_) {
                _ => fresh_refute(cache_dir, key, subject_id, graded_source, card, grade)
            }
        },
        null => fresh_refute(cache_dir, key, subject_id, graded_source, card, grade),
    }
}

function fresh_refute(
    cache_dir: string,
    key: string,
    subject_id: string,
    graded_source: string,
    card: PrincipleCard,
    grade: CardGrade,
) -> RefuteOut {
    let out = RefuteFinding(card.body, graded_source, grade.grade, grade.verdict, grade.evidence);
    store_json(cache_dir, key, out.to_json());
    out
}

// ---- per-job execution -------------------------------------------------------------------

// All the work one GradeJob needs: one grading call, the deterministic
// evidence gate, and PARALLEL refutation of surviving low grades. Must not
// throw (errors -> empty findings).
function run_grade_job(cache_dir: string, job: GradeJob) -> Finding[] {
    let findings: Finding[] = [];
    if (job.cards.length() == 0) {
        return findings;
    }
    let grades = cached_grades(cache_dir, job) catch (_) {
        _ => {
            baml.io.eprintln("[error] grading failed for " + job.subject_id);
            let empty: CardGrade[] = [];
            empty
        }
    };
    let card_by_id: map<string, PrincipleCard> = {};
    for (let card in job.cards) {
        card_by_id.set(card.id, card);
    }
    // refutes for this job's low grades run concurrently
    let refute_grades: CardGrade[] = [];
    let refute_futs = [];
    for (let g in grades) {
        if (evidence_in_source(g.evidence, job.graded_source) && (g.grade <= 4)) {
            match (card_by_id.get(g.card_id)) {
                let card: PrincipleCard => {
                    refute_grades.push(g);
                    refute_futs
                        .push(
                            spawn {
                                cached_refute(cache_dir, job.subject_id, job.graded_source, card, g) catch (_) {
                                    _ => RefuteOut { refuted: false, reason: "" }
                                }
                            },
                        );
                },
                null => null,
            }
        }
    }
    let refutes: map<string, RefuteOut> = {};
    let ri = 0;
    for (let fut in refute_futs) {
        let r = await fut;
        match (refute_grades.at(ri)) {
            let g: CardGrade => {
                refutes.set(g.card_id, r);
            },
            null => null,
        }
        ri = ri + 1;
    }
    for (let g in grades) {
        match (card_by_id.get(g.card_id)) {
            let card: PrincipleCard => {
                let verified = evidence_in_source(g.evidence, job.graded_source);
                // deterministic gates: a grade without a real quote is
                // neutralized; low grades face the skeptic.
                let effective = if (verified) {
                    g.grade
                } else {
                    5
                };
                let verdict = if (verified) {
                    g.verdict
                } else {
                    "neutral"
                };
                let refuted = false;
                let refute_reason: string? = null;
                if (verified && (effective <= 4)) {
                    match (refutes.get(g.card_id)) {
                        let r: RefuteOut => {
                            if (r.refuted) {
                                refuted = true;
                                refute_reason = r.reason;
                                effective = 5;
                                verdict = "neutral";
                            }
                        },
                        null => null,
                    }
                }
                findings
                    .push(
                        Finding {
                            block_id: job.subject_id,
                            block_kind: job.subject_kind,
                            is_interaction: job.is_interaction,
                            card_id: card.id,
                            grade: effective,
                            raw_grade: g.grade,
                            verdict: verdict,
                            evidence: g.evidence,
                            evidence_verified: verified,
                            suggested_form: g.suggested_form,
                            refuted: refuted,
                            refute_reason: refute_reason,
                            weight: finding_weight(card.weight, job.subject_kind, job.span_lines, job.is_interaction),
                        },
                    );
            },
            null => null,
        }
    }
    findings
}

// ---- stage (g): omission scan ----------------------------------------------------------

function ScanOmissions(intent_table: string, index: string) -> OmissionScanOut {
    client: Grader
    prompt: #"
        You are looking for design-adherence OMISSIONS in a BAML codebase: cases
        where the codebase clearly has goals a designed primitive exists for, yet
        never reaches for that primitive anywhere. Omissions are the strongest
        slop signal — the codebase that hand-rolls everything triggers no
        per-block finding because the primitive never appears.

        Below: the intention table (every declaration's and file's inferred goal
        and mechanism) and the card index. Report a card ONLY when (a) several
        block goals plainly call for its primitive, and (b) no mechanism in the
        table uses it. `evidence` names the block ids whose goals show the need.
        Return an empty list when the codebase is small or the primitives are
        genuinely not needed. Never invent card ids.

        # intention table (block_id | kind | goal | mechanism)
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
