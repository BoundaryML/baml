// intentionbench data model. Everything here is plain data — the pipeline
// serializes these to JSON for the cache and the report.
//
// The core idea: an AST-like hierarchy of CodeBlocks (program → file → decl →
// member/var), intention inference on EVERY block, and first-class
// InteractionBlocks (containment, siblings, static references) that carry
// their own intentions and are themselves graded.

// Bump when any prompt changes: it participates in every cache key, so prompt
// edits invalidate stale cached judgments.
function prompt_version() -> string {
    return "intentionbench-v1";
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

// ---- the block tree ----------------------------------------------------------

// One node of the codebase tree. `kind` is one of:
//   depth 0: "program"
//   depth 1: "file"
//   depth 2: "llm_function" | "function" | "class" | "enum" | "interface"
//            | "type_alias" | "test" | "config"
//   depth 3: "field" | "variant" | "param" | "var" | "method"
// (methods recurse: their params/vars sit at depth 4 with the same leaf kinds)
class CodeBlock {
    id: string, // stable id, see id scheme in scanner.baml
    parent_id: string?, // null only for the program block
    kind: string,
    name: string,
    file: string, // path relative to target root; "" for program
    start_line: int, // 1-based inclusive; 0 for program
    end_line: int,
    source: string, // exact span; file: whole file; program: file manifest
    child_ids: string[], // sorted by (start_line, id)
    depth: int,
}

// What a block is for (LLM-inferred, fast tier).
class Intention {
    goal: string, // one sentence: what the author is trying to achieve
    mechanism: string, // one sentence: the approach/primitives actually used
    confidence: int, // 1-5
}

// A first-class relation between two blocks. `kind`:
//   "by_depth"  — containment (child ↔ parent); edge_kind "contains"
//   "by_level"  — siblings under a common parent; edge_kind "sibling"
//   "reference" — static identifier reference; edge_kind "calls" | "uses_type"
//                 | "exercises"
class InteractionBlock {
    id: string, // "I:d:<child>^<parent>" | "I:l:<a>~<b>" | "I:r:<from>-><to>"
    kind: string,
    a_id: string, // by_level: lexicographically smaller endpoint
    b_id: string,
    parent_id: string, // common parent (batching/capping key)
    edge_kind: string,
    evidenced: bool, // a static identifier reference connects a and b
}

// What a relation means (LLM-inferred, fast tier, batched per parent).
class InteractionIntention {
    relationship: string, // one sentence: how a and b relate structurally
    purpose: string, // one sentence: why this relation exists / what it achieves
    confidence: int, // 1-5
}

// ---- catalog -----------------------------------------------------------------

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

// ---- LLM stage outputs (batched shapes) ---------------------------------------

// One leaf's intent inside a per-parent batch call.
class LeafIntent {
    block_id: string,
    goal: string,
    mechanism: string,
    confidence: int, // 1-5
}

class LeafBatchOut {
    intents: LeafIntent[],
}

// One interaction's intent inside a per-parent batch call.
class InterIntent {
    interaction_id: string,
    relationship: string,
    purpose: string,
    confidence: int, // 1-5
}

class InterBatchOut {
    intents: InterIntent[],
}

// One adherence grade for one card, graded in a batch per block.
class CardGrade {
    card_id: string,
    grade: int, // 1-10, anchored rubric
    verdict: string, // "adherent" | "neutral" | "fighting" | "reinventing"
    evidence: string, // verbatim quote from the graded source
    suggested_form: string?, // what intended usage would look like here
}

// All routed cards for one block graded in ONE call.
class MultiGradeOut {
    grades: CardGrade[],
}

// Skeptic pass over a low grade.
class RefuteOut {
    refuted: bool,
    reason: string,
}

// Codebase-level omission.
class OmissionOut {
    card_id: string,
    description: string, // why the codebase's goals wanted this primitive
    evidence: string, // which blocks/goals show the need
}

class OmissionScanOut {
    omissions: OmissionOut[],
}

// ---- assembled results --------------------------------------------------------

// A graded (block, card) pair after validation + refutation. `block_id` names
// either a CodeBlock or an InteractionBlock (is_interaction disambiguates).
class Finding {
    block_id: string,
    block_kind: string, // CodeBlock.kind, or the interaction's kind
    is_interaction: bool,
    card_id: string,
    grade: int, // effective grade (post-refutation/validation)
    raw_grade: int, // what the model said
    verdict: string,
    evidence: string,
    evidence_verified: bool, // evidence quote found in graded source (ws-normalized)
    suggested_form: string?,
    refuted: bool,
    refute_reason: string?,
    weight: float, // card weight × kind weight × size factor
}

class CardStat {
    card_id: string,
    title: string,
    n: int,
    mean_grade: float,
    worst_block_id: string,
    worst_grade: int,
}

class FileStat {
    file: string,
    n: int,
    mean_grade: float,
}

class KindStat {
    kind: string,
    n: int,
    mean_grade: float,
}

class Report {
    target: string,
    prompt_version: string,
    block_count: int,
    interaction_count: int,
    pair_cap_hits: int, // parents whose by_level pairs were truncated by the cap
    card_count: int, // gradeable cards (weight > 0)
    graded_pairs: int,
    coverage: float, // fraction of graded-eligible blocks with >= 1 routed card
    adherence_score: float, // weighted mean, 1-10
    commission_score: float, // mean over pairs where the design has an opinion
    omission_score: float, // 1 - omissions/gradeable cards
    findings: Finding[],
    omissions: OmissionOut[],
    per_card: CardStat[],
    per_file: FileStat[],
    per_kind: KindStat[],
}
