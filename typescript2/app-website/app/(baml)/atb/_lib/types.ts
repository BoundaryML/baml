// Row types mirroring the agent-tries-baml Convex deployment
// (see tools/agent-tries-baml/docs/data-model.md in the baml monorepo).

// trophies embed turnLog/filesCreated inline (unlike the other queue tables,
// which only carry blob pointers), so a trophies:list limit of 500 exceeds
// Convex's per-query bytes-read ceiling once recent runs get long enough.
// Capped here as a stopgap; the real fix is externalizing those fields to
// blob storage like transcriptStorageId already does. Trade-off: an issue
// whose only evidence trophy has aged out of this window loses its
// bamlVersion/brokeIn enrichment in the feed (falls back to null, not a
// crash) rather than the whole endpoint erroring out.
export const ATB_TROPHIES_QUERY_LIMIT = 100;

export type QueueFields = {
  _id: string;
  _creationTime: number;
  status: string;
  claimedBy?: string | null;
  claimedAt?: number | null;
  leaseExpiresAt?: number | null;
  attempts: number;
  lastError?: string | null;
  createdAt: number;
  updatedAt: number;
};

export type Task = QueueFields & {
  source: string; // slack | cron | bug_report
  prompt: string;
  repo?: string | null;
  ref?: string | null;
  sha?: string | null;
  bamlVersion?: string | null;
  slackUser?: string | null;
  cohortId?: string | null;
  skillRef?: string | null;
  skillStorageId?: string | null;
  transcriptStorageId?: string | null;
};

export type TurnTool = {
  name?: string;
  input?: unknown;
  result_preview?: string | null;
  result_chars?: number;
  is_error?: boolean;
};

/** One Claude Code API call in a trophy's turn log. */
export type Turn = {
  i: number;
  ts?: string;
  thinking_chars?: number;
  thinking_preview?: string;
  text_chars?: number;
  text_preview?: string;
  tools?: TurnTool[];
};

export type Metrics = {
  turns?: number | null;
  tool_calls?: number | null;
  api_calls?: number | null;
  input_tokens?: number | null;
  output_tokens?: number | null;
  cache_read_tokens?: number | null;
  cache_write_tokens?: number | null;
  total_tokens?: number | null;
  estimated_cost_usd?: number | null;
  wall_clock_ms?: number | null;
  files_touched?: number | null;
  loc_changed?: number | null;
};

export type Finding = {
  kind: 'skill' | 'language';
  title: string;
  description: string;
  anchor?: { call_index?: number | null; turn_index?: number | null };
  suggestion?: string | null;
  repro?: string | null;
};

export type Suggestion = {
  target: string;
  suggestion: string;
  rationale?: string | null;
};

export type Trophy = QueueFields & {
  taskId: string;
  outcome: string; // success | partial | failed | feedback | quota_skipped
  compileOk?: boolean | null;
  compileStderr?: string | null;
  bamlVersion?: string | null;
  metrics?: Metrics;
  summary?: string | null;
  whatWentWell?: string[];
  whatFailed?: string[];
  reportMd?: string | null;
  findings?: Finding[];
  suggestions?: Suggestion[];
  filesCreated?: Record<string, string> | null;
  turnLog?: Turn[];
  transcriptStorageId?: string | null;
  cohortId?: string | null;
  isCohortReport?: boolean;
};

export type Issue = QueueFields & {
  kind: 'skill' | 'language';
  category?: string | null; // bug | suggestion
  title: string;
  description: string;
  suggestion?: string | null;
  repro?: string | null;
  evidence?: Array<{
    trophyId?: string;
    turn_index?: number | null;
    call_index?: number | null;
    note?: string | null;
  }>;
  notionPageId?: string | null;
  notionSyncStatus?: string;
  linearIssueId?: string | null;
  linearSyncStatus?: string;
  // environment of the run(s) behind the issue (dedup-stamped)
  bamlVersion?: string | null;
  skillUsed?: string | null;
  skillVersion?: string | null;
  // fix pipeline (Cursor -> PR -> CI/CodeRabbit)
  prUrl?: string | null;
  prNumber?: number | null;
  checkState?: string | null; // pending | passing | failing
  coderabbitState?: string | null; // none | blocking | clear
  fixAttempts?: number | null;
  fixSlackTs?: string | null;
  firstSeenAt: number;
  lastSeenAt: number;
  // written by the bug verifier agent
  brokeIn?: string | null;
  fixedIn?: string | null;
  verifiedAt?: number | null;
  verifyBamlVersion?: string | null;
};

export type Cohort = QueueFields & {
  prompt: string;
  source?: string;
  skillRefs: string[];
  memberTaskIds: string[];
  reportTrophyId?: string | null;
  slackUser?: string | null;
};

export type Build = QueueFields & {
  sha: string;
  ref: string;
  channel?: string;
  binaryStorageId?: string | null;
  sizeBytes?: number | null;
  contentHash?: string | null;
  buildLogTail?: string | null;
  builtAt?: number | null;
};

export type Worker = {
  _id: string;
  _creationTime: number;
  workerId: string;
  role: string;
  status: string; // idle | busy
  currentItemId?: string | null;
  lastHeartbeat: number;
};

// ---- slim list shapes served by /api/state ----
// The full trophies:list payload is ~5 MB (turn logs, reports, file
// contents). List views only need these fields; detail pages subscribe to
// the full doc by id.

export type SlimTask = {
  _id: string;
  source: string;
  prompt: string; // truncated
  status: string;
  createdAt: number;
  updatedAt: number;
  claimedBy?: string | null;
  claimedAt?: number | null;
  cohortId?: string | null;
  skillRef?: string | null;
  skillStorageId?: string | null;
  bamlVersion?: string | null;
};

export type SlimTrophy = {
  _id: string;
  taskId: string;
  outcome: string;
  status: string;
  metrics?: Metrics;
  findingsCount: number;
  createdAt: number;
  updatedAt: number;
  claimedBy?: string | null;
  claimedAt?: number | null;
  cohortId?: string | null;
  isCohortReport?: boolean;
  bamlVersion?: string | null;
};

export type SlimIssue = {
  _id: string;
  kind: 'skill' | 'language';
  category?: string | null;
  title: string;
  status: string;
  fixSlackTs?: string | null;
  notionSyncStatus?: string;
  linearSyncStatus?: string;
  evidenceCount: number;
  // Slim evidence refs (trophy + cited call) so list/run views can link an issue
  // back to the runs that produced it without fetching the full issue doc.
  evidence?: Array<{ trophyId?: string; call_index?: number | null }>;
  // Environment + verify facts so the list view can show them without the full doc.
  bamlVersion?: string | null;
  skillUsed?: string | null;
  skillVersion?: string | null;
  brokeIn?: string | null;
  fixedIn?: string | null;
  verifiedAt?: number | null;
  firstSeenAt: number;
  lastSeenAt: number;
  createdAt: number;
};

export type AtbState = {
  generatedAt: number;
  tasks: SlimTask[];
  trophies: SlimTrophy[];
  issues: SlimIssue[];
  cohorts: Cohort[];
  builds: Build[];
  workers: Worker[];
};
