// Server-side data loader for the baml-bench dashboard.
// Fetches tasks / trophies / issues from the api service (the sole
// Convex gateway) and reshapes them for the dashboard + run-detail pages.

const BASE = process.env.SERVICE_URL;
const TOKEN = process.env.SERVICE_TOKEN ?? "";

/** One Claude Code transcript turn (a single API call) in a trophy's turn log. */
export type Turn = {
  i: number;
  thinking_chars?: number;
  thinking_preview?: string;
  text_chars?: number;
  text_preview?: string;
  tools?: Array<{
    name?: string;
    input?: unknown;
    result_preview?: string | null;
    result_chars?: number;
    is_error?: boolean;
  }>;
};

/** Aggregate run metrics (turns, tokens, cost, wall clock) recorded on a trophy. */
export type Metrics = {
  turns?: number | null;
  tool_calls?: number | null;
  api_calls?: number | null;
  input_tokens?: number | null;
  output_tokens?: number | null;
  total_tokens?: number | null;
  estimated_cost_usd?: number | null;
  wall_clock_ms?: number | null;
};

/** A single skill- or language-level finding surfaced by a run, optionally anchored to a transcript call. */
export type Finding = {
  kind: "skill" | "language";
  title: string;
  description: string;
  anchor?: { call_index?: number | null; turn_index?: number | null };
  repro?: string | null;
  reproVerified?: boolean | null;
};

/** A completed run record: its outcome, metrics, report, findings, and full turn log. */
export type Trophy = {
  _id: string;
  taskId: string;
  outcome: string;
  bamlVersion?: string | null;
  metrics?: Metrics;
  summary?: string | null;
  whatWentWell?: string[];
  whatFailed?: string[];
  reportMd?: string | null;
  findings?: Finding[];
  turnLog?: Turn[];
  transcriptStorageId?: string | null;
  createdAt: number;
  status: string;
};

/** A benchmark task queued for an agent to attempt (from Slack, cron, etc.). */
export type Task = {
  _id: string;
  source: string;
  prompt: string;
  slackUser?: string | null;
  bamlVersion?: string | null;
  status: string;
  createdAt: number;
};

/** A deduplicated issue aggregated from run findings, tracked through to Notion/Cursor. */
export type Issue = {
  _id: string;
  kind: "skill" | "language";
  title: string;
  description: string;
  status: string;
  evidence?: Array<{ trophyId?: string; call_index?: number | null }>;
  notionSyncStatus?: "dirty" | "syncing" | "synced";
  notionPageId?: string | null;
  fixSlackTs?: string | null; // set to the Cursor agent ref once dispatched
  firstSeenAt: number;
  lastSeenAt: number;
};

// Once an issue is handed to Cursor (a cloud agent was launched and its ref
// stored in fixSlackTs) the outcome is out of our hands, so we surface
// "cursor" rather than whatever internal queue state the row drifted into.
/**
 * Maps an issue's raw status to the label shown in the UI.
 * @param i - the issue's status and Cursor dispatch ref
 * @returns "cursor" once handed to a Cursor agent, otherwise the raw status
 */
export function issueStatusLabel(i: Pick<Issue, "status" | "fixSlackTs">): string {
  return i.fixSlackTs ? "cursor" : i.status;
}

/** A flattened run row joining a trophy to its task, sized for the dashboard runs table. */
export type RunRow = {
  trophyId: string;
  prompt: string;
  source: string;
  outcome: string;
  turns?: number | null;
  apiCalls?: number | null;
  outputTokens?: number | null;
  costUsd?: number | null;
  wallS?: number | null;
  bamlVersion?: string | null;
  findings: number;
  createdAt: number;
};

/** The full payload rendered by the static dashboard: runs, open issues, and headline totals. */
export type DashboardData = {
  configured: boolean;
  generatedAt: string;
  runs: RunRow[];
  issues: Issue[];
  totals: { runs: number; costUsd: number; openIssues: number };
};

/**
 * Fetches a JSON resource from the api service with the server-side bearer token.
 * @param path - request path appended to the configured SERVICE_URL base
 * @returns the parsed body, or null if unconfigured, non-OK, or on any error
 */
async function get<T>(path: string): Promise<T | null> {
  if (!BASE) return null;
  try {
    const res = await fetch(BASE.replace(/\/$/, "") + path, {
      headers: { Authorization: `Bearer ${TOKEN}` },
      cache: "no-store",
    });
    if (!res.ok) return null;
    return (await res.json()) as T;
  } catch {
    return null;
  }
}

const OPEN_ISSUE_STATUSES = new Set(["open", "confirmed", "approved", "fixing"]);

/**
 * Loads and reshapes trophies, tasks, and issues into the static dashboard payload.
 * @returns dashboard data with joined run rows, open issues, and totals; an unconfigured shell if SERVICE_URL is unset
 */
export async function loadDashboardData(): Promise<DashboardData> {
  const generatedAt = new Date().toISOString().replace(/\.\d+Z$/, "Z");
  if (!BASE) {
    return { configured: false, generatedAt, runs: [], issues: [],
             totals: { runs: 0, costUsd: 0, openIssues: 0 } };
  }
  const [trophies, tasks, issues] = await Promise.all([
    get<Trophy[]>("/trophies?limit=60"),
    get<Task[]>("/tasks?limit=200"),
    get<Issue[]>("/issues?limit=100"),
  ]);
  const taskById = new Map((tasks ?? []).map((t) => [t._id, t]));
  const runs: RunRow[] = (trophies ?? []).map((tr) => {
    const m = tr.metrics ?? {};
    const task = taskById.get(tr.taskId);
    return {
      trophyId: tr._id,
      prompt: task?.prompt ?? "(task not found)",
      source: task?.source ?? "?",
      outcome: tr.outcome,
      turns: m.turns ?? null,
      apiCalls: m.api_calls ?? null,
      outputTokens: m.output_tokens ?? null,
      costUsd: m.estimated_cost_usd ?? null,
      wallS: m.wall_clock_ms != null ? Math.round((m.wall_clock_ms / 1000) * 10) / 10 : null,
      bamlVersion: tr.bamlVersion ?? null,
      findings: (tr.findings ?? []).length,
      createdAt: tr.createdAt,
    };
  });
  const openIssues = (issues ?? []).filter((i) => OPEN_ISSUE_STATUSES.has(i.status));
  const costUsd =
    Math.round(runs.reduce((a, r) => a + (r.costUsd ?? 0), 0) * 10000) / 10000;
  return {
    configured: true,
    generatedAt,
    runs,
    issues: openIssues,
    totals: { runs: runs.length, costUsd, openIssues: openIssues.length },
  };
}

// ---------- live dashboard state ----------

/** A BAML canary build row from the bamlBuilds registry (sha, channel ref, status). */
export type Build = {
  _id: string; sha: string; ref?: string; status: string;
  sizeBytes?: number | null; createdAt: number; claimedBy?: string | null;
};

/** A unit of work an agent is actively processing right now (task, trophy, issue, or build). */
export type Inflight = {
  kind: string; stage: string; id: string; label: string;
  claimedBy?: string | null; sinceMs: number;
};

/** A task row enriched for the live view, including its claiming worker and resolved report id. */
export type TaskRow = {
  _id: string; source: string; prompt: string; status: string;
  createdAt: number; claimedBy?: string | null; claimedAt?: number | null;
  bamlVersion?: string | null; reportId?: string | null;
  slackUser?: string | null;
};

/** The full live-polled snapshot driving the graph, db tables, and live dashboard. */
export type LiveState = {
  configured: boolean;
  generatedAt: string;
  counts: {
    tasks: Record<string, number>;
    trophies: Record<string, number>;
    issues: Record<string, number>;
    builds: Record<string, number>;
  };
  inflight: Inflight[];
  runs: RunRow[];
  issues: Issue[];
  builds: Build[];
  tasks: TaskRow[];
  agents: { activeTasks: number; workers: number; dedupers: number; fixers: number };
  totals: { tasks: number; trophies: number; openIssues: number; costUsd: number };
};

/**
 * Counts rows grouped by a derived key, for the per-table status breakdowns.
 * @param rows - the rows to bucket
 * @param key - derives the grouping key for a row (blank keys fall back to "?")
 * @returns a map of key to count
 */
function tally<T extends { status?: string }>(rows: T[], key: (r: T) => string): Record<string, number> {
  const out: Record<string, number> = {};
  for (const r of rows) {
    const k = key(r) || "?";
    out[k] = (out[k] ?? 0) + 1;
  }
  return out;
}

// statuses that mean "a processor is actively working on this right now"
const INFLIGHT: Array<[string, string, string, (r: any) => boolean]> = [
  ["task", "worker", "running", (r) => r.status === "running"],
  ["trophy", "dedup", "deduping", (r) => r.status === "deduping"],
  ["issue", "notion-sync", "syncing", (r) => r.notionSyncStatus === "syncing"],
  ["build", "baml-build", "building", (r) => r.status === "building"],
];

/**
 * Loads the live snapshot: status tallies, in-flight work, runs, issues, builds, and agent counts.
 * @returns the live state polled by the dashboard; an unconfigured empty shell if SERVICE_URL is unset
 */
export async function loadState(): Promise<LiveState> {
  const generatedAt = new Date().toISOString().replace(/\.\d+Z$/, "Z");
  const empty = {
    configured: false, generatedAt,
    counts: { tasks: {}, trophies: {}, issues: {}, builds: {} },
    inflight: [], runs: [], issues: [], builds: [], tasks: [],
    agents: { activeTasks: 0, workers: 0, dedupers: 0, fixers: 0 },
    totals: { tasks: 0, trophies: 0, openIssues: 0, costUsd: 0 },
  };
  if (!BASE) return empty;
  const [trophies, tasks, issues, builds] = await Promise.all([
    get<Trophy[]>("/trophies?limit=60"),
    get<TaskRow[]>("/tasks?limit=200"),
    get<Issue[]>("/issues?limit=100"),
    get<Build[]>("/bamlBuilds?limit=20"),
  ]);
  const T = tasks ?? [], TR = trophies ?? [], IS = issues ?? [], BD = builds ?? [];
  const now = Date.now();
  const taskById = new Map(T.map((t) => [t._id, t]));
  const reportByTask = new Map(TR.map((tr) => [tr.taskId, tr._id]));

  const runs: RunRow[] = TR.map((tr) => {
    const m = tr.metrics ?? {};
    const task = taskById.get(tr.taskId);
    return {
      trophyId: tr._id, prompt: task?.prompt ?? "(task not found)",
      source: task?.source ?? "?", outcome: tr.outcome,
      turns: m.turns ?? null, apiCalls: m.api_calls ?? null,
      outputTokens: m.output_tokens ?? null, costUsd: m.estimated_cost_usd ?? null,
      wallS: m.wall_clock_ms != null ? Math.round((m.wall_clock_ms / 1000) * 10) / 10 : null,
      bamlVersion: tr.bamlVersion ?? null, findings: (tr.findings ?? []).length,
      createdAt: tr.createdAt,
    };
  });

  const inflight: Inflight[] = [];
  const pools: Record<string, any[]> = { task: T, trophy: TR, issue: IS, build: BD };
  for (const [kind, stage, , pred] of INFLIGHT) {
    for (const r of pools[kind]) {
      if (!pred(r)) continue;
      const label =
        kind === "task" ? (taskById.get(r._id)?.prompt ?? r.prompt ?? "")
        : kind === "trophy" ? (taskById.get(r.taskId)?.prompt ?? "dedup batch")
        : kind === "issue" ? r.title
        : `baml ${String(r.sha).slice(0, 8)}`;
      inflight.push({
        kind, stage, id: r._id, label: String(label).slice(0, 80),
        claimedBy: r.claimedBy ?? null,
        sinceMs: now - (r.claimedAt ?? r.updatedAt ?? r.createdAt ?? now),
      });
    }
  }

  const openIssues = IS.filter((i) => OPEN_ISSUE_STATUSES.has(i.status));
  // The /db/issues view shows every issue with its status (incl. closed /
  // rejected); open ones first, then most-recently-seen. `openIssues` still
  // drives the totals badge below.
  const allIssues = [...IS].sort((a, b) =>
    (OPEN_ISSUE_STATUSES.has(b.status) ? 1 : 0) - (OPEN_ISSUE_STATUSES.has(a.status) ? 1 : 0)
    || b.lastSeenAt - a.lastSeenAt);
  const costUsd = Math.round(runs.reduce((a, r) => a + (r.costUsd ?? 0), 0) * 10000) / 10000;
  const byStage = (st: string) => inflight.filter((f) => f.stage === st).length;
  const agents = {
    // "active" = an agent is currently working on it (derived from in-flight
    // claims, so it scales naturally if more workers are added later)
    activeTasks: T.filter((t) => !["done", "failed"].includes(t.status)).length,
    workers: byStage("worker"),
    dedupers: byStage("dedup"),
    fixers: byStage("notion-sync"),
  };
  return {
    configured: true, generatedAt,
    counts: {
      tasks: tally(T, (r) => r.status),
      trophies: tally(TR, (r) => r.status),
      issues: tally(IS, issueStatusLabel),
      builds: tally(BD, (r) => r.status),
    },
    inflight,
    runs,
    issues: allIssues,
    builds: BD,
    tasks: T.slice(0, 40).map((t) => ({ ...t, reportId: reportByTask.get(t._id) ?? null })),
    agents,
    totals: { tasks: T.length, trophies: TR.length, openIssues: openIssues.length, costUsd },
  };
}

/** A run-detail bundle for the run page: the trophy, its task, and a readable baml version. */
export type RunDetail = {
  trophy: Trophy;
  task: Task | null;
  bamlLabel: string | null; // readable alpha version (e.g. "0.11.0-alpha.4166")
};

// Resolve a baml sha to its human-readable channel version via the bamlBuilds
// registry, so the UI can show "0.11.0-alpha.4166" instead of an opaque sha.
/**
 * Resolves a baml build sha to its human-readable channel version via the bamlBuilds registry.
 * @param sha - the build sha to look up
 * @returns the alpha version (e.g. "0.11.0-alpha.4166"), or null if absent/unresolvable
 */
async function bamlLabelForSha(sha?: string | null): Promise<string | null> {
  if (!sha) return null;
  const builds = await get<Build[]>(`/bamlBuilds?field=sha&value=${sha}&index=by_sha&limit=1`);
  const ref = builds?.[0]?.ref;
  return ref ? ref.replace(/^baml-language-/, "") : null;
}

/**
 * Loads a single run's detail: its trophy, the originating task, and a readable baml label.
 * @param trophyId - the trophy id to load
 * @returns the run detail, or null if unconfigured or the trophy is not found
 */
export async function loadRun(trophyId: string): Promise<RunDetail | null> {
  if (!BASE) return null;
  const trophy = await get<Trophy>(`/trophies/${trophyId}`);
  if (!trophy) return null;
  const task = await get<Task>(`/tasks/${trophy.taskId}`);
  const bamlLabel = await bamlLabelForSha(trophy.bamlVersion);
  return { trophy, task: task ?? null, bamlLabel };
}

/**
 * Loads a task plus the id of any trophy it has produced (used to redirect to the result).
 * @param id - the task id to load
 * @returns the task, its trophy id (if any), and a readable baml label; or null if unconfigured/not found
 */
export async function loadTask(
  id: string,
): Promise<{ task: TaskRow; trophyId: string | null; bamlLabel: string | null } | null> {
  if (!BASE) return null;
  const task = await get<TaskRow>(`/tasks/${id}`);
  if (!task) return null;
  const trophies = await get<Trophy[]>(`/trophies?field=taskId&value=${id}&index=by_task&limit=3`);
  const trophyId = trophies && trophies[0] ? trophies[0]._id : null;
  const bamlLabel = await bamlLabelForSha(task.bamlVersion ?? trophies?.[0]?.bamlVersion);
  return { task, trophyId, bamlLabel };
}
