// Core data shapes for agent-tries-baml-v2.
//
// Two casings are deliberate and must stay byte-compatible on the wire:
//   * Convex doc classes  -> camelCase  (mirror convex/schema.ts exactly)
//   * proxy/HTTP contract -> snake_case (mirror libs/bench_core/schemas.py)
// Stringly-typed Convex fields (v.string()) are kept as `string` with the
// allowed values in a comment, rather than enums, so serialization matches the
// DB verbatim. Enums are reserved for LLM-function outputs (llm/*.baml) where
// BAML's structured parsing earns its keep.

// ======================================================================
// Pricing  (schemas.py: Prices)
// ======================================================================
class Prices {
    input_per_million_usd: float,
    output_per_million_usd: float,
    cache_read_per_million_usd: float,
    cache_write_per_million_usd: float,
}

// ======================================================================
// claude-proxy contract  (schemas.py — snake_case wire format)
// ======================================================================
class RunAgentRequest {
    cell_id: string,
    model: string, // default "claude-opus-4-8"
    max_turns: int, // default 40
    prompt: string,
    system_prompt: string?,
    files: map<string, string>,
    prices: Prices?,
    baml_version: string?, // sha of the baml CLI to place on PATH
    install_skill: bool, // run `baml agent install` before the agent
    post_file_patterns: string[],
    max_file_bytes: int, // default 256*1024
    max_total_file_bytes: int, // default 4*1024*1024
    invocation_timeout_secs: int?,
}

class CheckBamlRequest {
    check_id: string,
    files: map<string, string>,
    command: string, // default "baml generate"
    baml_version: string?,
    timeout_secs: int, // default 90
}

class CheckBamlResult {
    check_id: string,
    exit_code: int,
    timed_out: bool,
    baml_on_path: bool,
    stdout_tail: string?,
    stderr_tail: string?,
}

class AgentResult {
    cell_id: string,
    status: string, // ok | timeout | error
    exit_code: int,
    transcript: string?,
    stderr_tail: string?,
    turns: int?,
    tool_calls: int?,
    api_calls: int?,
    input_tokens: int?,
    output_tokens: int?,
    total_tokens: int?,
    cache_read_tokens: int?,
    cache_write_tokens: int?,
    wall_clock_ms: int?,
    estimated_cost_usd: float?,
    turn_log: json[]?, // raw turn objects (typed TurnRow[]? serializes elements to null)
    post_files: map<string, string>,
    skill_installed: bool?,
}

// ======================================================================
// Trophy metric bag  (schemas.py: Metrics)
// ======================================================================
class Metrics {
    turns: int?,
    tool_calls: int?,
    api_calls: int?,
    invocations: int?,
    input_tokens: int?,
    output_tokens: int?,
    total_tokens: int?,
    cache_read_tokens: int?,
    cache_write_tokens: int?,
    wall_clock_ms: int?,
    setup_build_ms: int?,
    agent_thinking_ms: int?,
    debug_retries: int?,
    estimated_cost_usd: float?,
    files_touched: int?,
    loc_changed: int?,
}

// ======================================================================
// Evidence / findings  (schemas.py: EvidenceAnchor, Finding)
// kind: "skill" | "language"
// ======================================================================
class EvidenceAnchor {
    trophyId: string?,
    turnIndex: int?,
    callIndex: int?,
    note: string?,
}

class Finding {
    kind: string, // skill | language
    title: string,
    description: string,
    call_index: int?,
    turn_index: int?,
    repro: string?,
}

// ======================================================================
// Shared queue fields  (convex/schema.ts: queueFields, spread on every
// claimable table). Inlined per class below since BAML classes don't spread;
// keep these field names/types identical across all queue docs.
//   status: string        per-table state machine
//   claimedBy: string?
//   claimedAt: int?
//   leaseExpiresAt: int?
//   attempts: int
//   lastError: string?
//   createdAt: int
//   updatedAt: int
// Plus Convex's own _id / _creationTime on every fetched doc.
// ======================================================================

// ---- tasks ----
class Task {
    _id: string?,
    _creationTime: float?,
    source: string, // slack | cron | bug_report | pr_canary
    prompt: string,
    repo: string?,
    ref: string?,
    sha: string?,
    bamlVersion: string?,
    bamlPin: string?,
    bamlChannel: string?, // nightly | canary
    coldStart: bool?,
    cohortId: string?,
    skillRef: string?,
    slackChannel: string?,
    slackThreadTs: string?,
    slackUser: string?,
    notionProposerPageId: string?,
    transcriptStorageId: string?,
    skillStorageId: string?,
    // queueFields
    status: string,
    claimedBy: string?,
    claimedAt: float?,
    leaseExpiresAt: float?,
    attempts: float,
    lastError: string?,
    createdAt: float,
    updatedAt: float,
}

// ---- trophies ----
class Trophy {
    _id: string?,
    _creationTime: float?,
    taskId: string,
    source: string?, // worker (default/absent) | feedback
    outcome: string, // success | partial | failed | quota_skipped | feedback
    compileOk: bool?,
    compileStderr: string?,
    bamlVersion: string?,
    metrics: json, // full Metrics bag
    transcriptStorageId: string?,
    turnLog: json[]?,
    summary: string?,
    whatWentWell: string[]?,
    whatFailed: string[]?,
    reportMd: string?,
    findings: json[]?, // {kind,title,description,anchor,suggestion?,repro?}
    reporterAgentId: string?, // vestigial
    suggestions: json[]?, // {target, suggestion, rationale}
    cohortId: string?,
    isCohortReport: bool?,
    // queueFields
    status: string,
    claimedBy: string?,
    claimedAt: float?,
    leaseExpiresAt: float?,
    attempts: float,
    lastError: string?,
    createdAt: float,
    updatedAt: float,
}

// ---- issues ----
class Issue {
    _id: string?,
    _creationTime: float?,
    kind: string, // skill | language
    category: string?, // bug | suggestion
    title: string,
    description: string,
    suggestion: string?,
    evidence: json[], // {trophyId, turnIndex?, callIndex?, note?}
    repro: string?,
    // Linear board
    linearIssueId: string?,
    linearSyncStatus: string?, // dirty | syncing | synced
    // Notion (deprecated)
    notionPageId: string?,
    fixSlackTs: string?,
    fixThreadTs: string?,
    // cursor-fix / PR tracking
    cursorAgentId: string?,
    prUrl: string?,
    prBranch: string?,
    prNumber: float?,
    fixAttempts: float?,
    lastFixedSha: string?,
    checkState: string?, // pending | passing | failing
    coderabbitState: string?, // none | blocking | clear
    lastHumanCommentAt: string?,
    // bug-verify
    verifiedAt: float?,
    verifyBamlVersion: string?,
    brokeIn: string?,
    fixedIn: string?,
    firstSeenAt: float,
    lastSeenAt: float,
    notionSyncStatus: string?, // deprecated
    // queueFields
    status: string, // open|confirmed|approved|dispatching|tocursor|prprep|pr_ready|closed|rejected|needs_human|redraft|redrafting
    claimedBy: string?,
    claimedAt: float?,
    leaseExpiresAt: float?,
    attempts: float,
    lastError: string?,
    createdAt: float,
    updatedAt: float,
}

// ---- cohorts ----
class Cohort {
    _id: string?,
    _creationTime: float?,
    prompt: string,
    skillRefs: string[],
    memberTaskIds: string[],
    source: string?, // slack | bug_report
    slackChannel: string?,
    slackThreadTs: string?,
    slackUser: string?,
    reportTrophyId: string?,
    // queueFields  (status: pending -> queued -> comparing -> done | failed)
    status: string,
    claimedBy: string?,
    claimedAt: float?,
    leaseExpiresAt: float?,
    attempts: float,
    lastError: string?,
    createdAt: float,
    updatedAt: float,
}

// ---- bamlBuilds ----
class BamlBuild {
    _id: string?,
    _creationTime: float?,
    sha: string,
    ref: string,
    channel: string?, // nightly | canary
    binaryStorageId: string?,
    sizeBytes: float?,
    contentHash: string?,
    buildLogTail: string?,
    builtAt: float?,
    // queueFields  (status: queued -> building -> ready | failed)
    status: string,
    claimedBy: string?,
    claimedAt: float?,
    leaseExpiresAt: float?,
    attempts: float,
    lastError: string?,
    createdAt: float,
    updatedAt: float,
}

// ---- changelogEntries ----
class ChangelogEntry {
    _id: string?,
    _creationTime: float?,
    version: string,
    tag: string?,
    channel: string, // nightly | canary | alpha | engine | unknown
    date: string?,
    title: string?,
    body: string?,
    authors: string[]?,
    fromRelease: string?,
    reviseMode: string?, // revise | regenerate
    reviseGuidance: string?,
    slackChannel: string?,
    slackThreadTs: string?,
    slackUser: string?,
    // queueFields  (status: queued -> generating -> done | failed)
    status: string,
    claimedBy: string?,
    claimedAt: float?,
    leaseExpiresAt: float?,
    attempts: float,
    lastError: string?,
    createdAt: float,
    updatedAt: float,
}

// ---- promoCodes  (not a claimable queue) ----
class PromoCode {
    _id: string?,
    _creationTime: float?,
    code: string,
    position: float,
    status: string, // unused | used
    claimedBy: string?,
    claimedByUserId: string?,
    notes: string?,
    claimedAt: float?,
    createdAt: float,
    updatedAt: float,
}

// ---- workers  (presence / observability) ----
class Worker {
    _id: string?,
    _creationTime: float?,
    workerId: string,
    role: string,
    status: string, // idle | busy
    currentItemId: string?,
    lastHeartbeat: float,
}
