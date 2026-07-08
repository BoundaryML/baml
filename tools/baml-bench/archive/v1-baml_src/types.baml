// Shared data model for the adherence pipeline. Everything here is plain data —
// the pipeline serializes these to JSON for the cache and the report.

// Bump when any prompt in llm.baml changes: it participates in every cache key,
// so prompt edits invalidate stale cached judgments.
function prompt_version() -> string {
    return "adherence-v3";
}

// Model tiers, mirrored into cache-key stage tags so swapping a model
// invalidates that stage's cached judgments instead of silently replaying the
// old model's answers. Keep in sync with clients.baml.
function fast_model() -> string {
    return "claude-haiku-4-5";
}

function judge_model() -> string {
    return "claude-sonnet-4-6";
}

// ---- deterministic stages ---------------------------------------------------

// One graded unit of code. `kind` is one of:
//   "llm_function" | "function" | "type" | "test" | "config"
class Chunk {
    id: string, // "<relpath>:<name>" — stable across runs
    file: string, // path relative to the target root
    name: string,
    kind: string,
    start_line: int, // 1-based, inclusive
    end_line: int,
    source: string,
}

// A directed reference between chunks, found by identifier scan.
//   "exercises" (test -> anything) | "uses_type" (-> type chunk) | "calls"
class Edge {
    from_id: string,
    to_id: string,
    kind: string,
}

// One principle card parsed out of design-principles.md.
class PrincipleCard {
    id: string, // "P-044-1"
    title: string,
    status: string, // implemented | accepted | pending | proposed | draft | rejected | superseded | unknown
    weight: float, // scoring weight derived from status; 0.0 = not graded
    trigger: string, // the Applicability trigger line (routing predicate)
    detectability: string,
    body: string, // full card markdown (fed to the grader verbatim)
}

// ---- LLM stage outputs ------------------------------------------------------

// Stages (c)+(d) merged: one look at the chunk yields both its intention and
// which cards apply — both are "read the chunk once" tasks, and merging halves
// the per-chunk call count.
class ChunkAnalysis {
    goal: string, // one sentence: what the author is trying to achieve
    mechanism: string, // one sentence: the approach/primitives actually used
    confidence: int, // 1-5
    card_ids: string[], // applicable principle cards (validated downstream)
}

// Stage (e): one adherence grade for one card, graded in a batch per chunk.
class CardGrade {
    card_id: string,
    grade: int, // 1-10, anchored rubric
    verdict: string, // "adherent" | "neutral" | "fighting" | "reinventing"
    evidence: string, // verbatim quote from the chunk source
    suggested_form: string?, // what intended usage would look like here
}

// Stage (e): all routed cards for one chunk graded in ONE call (the chunk
// source rides in the prompt once instead of once per card).
class MultiGradeOut {
    grades: CardGrade[],
}

// Stage (f): skeptic pass over a low grade.
class RefuteOut {
    refuted: bool,
    reason: string,
}

// Stage (g): codebase-level omission.
class OmissionOut {
    card_id: string,
    description: string, // why the codebase's goals wanted this primitive
    evidence: string, // which chunks/goals show the need
}

class OmissionScanOut {
    omissions: OmissionOut[],
}

// ---- assembled results ------------------------------------------------------

// A graded (chunk, card) pair after validation + refutation.
class Finding {
    chunk_id: string,
    card_id: string,
    grade: int, // effective grade (post-refutation/validation)
    raw_grade: int, // what the model said
    verdict: string,
    evidence: string,
    evidence_verified: bool, // evidence quote found in chunk source (whitespace-normalized)
    suggested_form: string?,
    refuted: bool,
    refute_reason: string?,
    weight: float, // card weight; effective score weight
}

class CardStat {
    card_id: string,
    title: string,
    n: int,
    mean_grade: float,
    worst_chunk_id: string,
    worst_grade: int,
}

class FileStat {
    file: string,
    n: int,
    mean_grade: float,
}

class Report {
    target: string,
    prompt_version: string,
    chunk_count: int,
    edge_count: int,
    card_count: int, // gradeable cards (weight > 0)
    graded_pairs: int,
    coverage: float, // fraction of chunks with >= 1 applicable card
    adherence_score: float, // weighted mean, 1-10
    commission_score: float, // mean over misuse-eligible pairs
    omission_score: float, // 1 - omissions/gradeable cards
    findings: Finding[],
    omissions: OmissionOut[],
    per_card: CardStat[],
    per_file: FileStat[],
}
