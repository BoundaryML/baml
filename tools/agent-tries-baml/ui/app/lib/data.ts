// Server-side data loader for the agent-tries-baml dashboard.
// Fetches tasks / trophies / issues from the api service (the sole
// Convex gateway) and reshapes them for the dashboard + run-detail pages.

const BASE = process.env.SERVICE_URL;
const TOKEN = process.env.SERVICE_TOKEN ?? '';

/** One Claude Code transcript turn (a single API call) in a trophy's turn log. */
export type Turn = {
  i: number;
  ts?: string; // ISO wall-clock timestamp of the assistant message (newer runs)
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
  files_touched?: number | null;
  loc_changed?: number | null;
};

/** A single skill- or language-level finding surfaced by a run, optionally anchored to a transcript call. */
export type Finding = {
  kind: 'skill' | 'language';
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
  filesCreated?: Record<string, string> | null;
  turnLog?: Turn[];
  transcriptStorageId?: string | null;
  // skill-arena: a held member trophy carries cohortId; the synthesized comparison
  // is a trophy with isCohortReport=true that enters dedup like any other.
  cohortId?: string | null;
  isCohortReport?: boolean;
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
  // skill-arena: set on a cohort member task (which variant it runs).
  cohortId?: string | null;
  skillRef?: string | null;
};

/** A skill-arena cohort: one task run across N baml-skill branches, then compared. */
export type Cohort = {
  _id: string;
  prompt: string;
  skillRefs: string[];
  memberTaskIds: string[];
  status: string; // pending | queued | comparing | done | failed
  reportTrophyId?: string | null;
  slackUser?: string | null;
  createdAt: number;
};

/** A deduplicated issue aggregated from run findings, tracked through to Notion/Cursor. */
export type Issue = {
  _id: string;
  kind: 'skill' | 'language';
  title: string;
  description: string;
  status: string;
  evidence?: Array<{ trophyId?: string; call_index?: number | null }>;
  notionSyncStatus?: 'dirty' | 'syncing' | 'synced';
  notionPageId?: string | null;
  fixSlackTs?: string | null; // set to the Cursor agent ref once dispatched
  // cursor-tracker PR phase (tocursor -> prprep -> pr_ready)
  prUrl?: string | null;
  prNumber?: number | null;
  fixAttempts?: number | null;
  checkState?: string | null; // pending | passing | failing
  coderabbitState?: string | null; // none | blocking | clear
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
export function issueStatusLabel(
  i: Pick<Issue, 'status' | 'fixSlackTs'>,
): string {
  return i.fixSlackTs ? 'cursor' : i.status;
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
    const res = await fetch(BASE.replace(/\/$/, '') + path, {
      headers: { Authorization: `Bearer ${TOKEN}` },
      cache: 'no-store',
    });
    if (!res.ok) return null;
    return (await res.json()) as T;
  } catch {
    return null;
  }
}

/**
 * Fetches a plain-text resource (e.g. a transcript blob) with the bearer token.
 * @param path - request path appended to the configured SERVICE_URL base
 * @returns the response body as text, or null if unconfigured, non-OK, or on any error
 */
async function getText(path: string): Promise<string | null> {
  if (!BASE) return null;
  try {
    const res = await fetch(BASE.replace(/\/$/, '') + path, {
      headers: { Authorization: `Bearer ${TOKEN}` },
      cache: 'no-store',
    });
    if (!res.ok) return null;
    return await res.text();
  } catch {
    return null;
  }
}

const OPEN_ISSUE_STATUSES = new Set([
  'open',
  'confirmed',
  'approved',
  'fixing',
]);

/**
 * Loads and reshapes trophies, tasks, and issues into the static dashboard payload.
 * @returns dashboard data with joined run rows, open issues, and totals; an unconfigured shell if SERVICE_URL is unset
 */
export async function loadDashboardData(): Promise<DashboardData> {
  const generatedAt = new Date().toISOString().replace(/\.\d+Z$/, 'Z');
  if (!BASE) {
    return {
      configured: false,
      generatedAt,
      runs: [],
      issues: [],
      totals: { runs: 0, costUsd: 0, openIssues: 0 },
    };
  }
  const [trophies, tasks, issues] = await Promise.all([
    get<Trophy[]>('/trophies?limit=60'),
    get<Task[]>('/tasks?limit=200'),
    get<Issue[]>('/issues?limit=100'),
  ]);
  const taskById = new Map((tasks ?? []).map((t) => [t._id, t]));
  const runs: RunRow[] = (trophies ?? []).map((tr) => {
    const m = tr.metrics ?? {};
    const task = taskById.get(tr.taskId);
    return {
      trophyId: tr._id,
      prompt: task?.prompt ?? '(task not found)',
      source: task?.source ?? '?',
      outcome: tr.outcome,
      turns: m.turns ?? null,
      apiCalls: m.api_calls ?? null,
      outputTokens: m.output_tokens ?? null,
      costUsd: m.estimated_cost_usd ?? null,
      wallS:
        m.wall_clock_ms != null
          ? Math.round((m.wall_clock_ms / 1000) * 10) / 10
          : null,
      bamlVersion: tr.bamlVersion ?? null,
      findings: (tr.findings ?? []).length,
      createdAt: tr.createdAt,
    };
  });
  const openIssues = (issues ?? []).filter((i) =>
    OPEN_ISSUE_STATUSES.has(i.status),
  );
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
  _id: string;
  sha: string;
  ref?: string;
  status: string;
  sizeBytes?: number | null;
  createdAt: number;
  claimedBy?: string | null;
};

/** A unit of work an agent is actively processing right now (task, trophy, issue, or build). */
export type Inflight = {
  kind: string;
  stage: string;
  id: string;
  label: string;
  claimedBy?: string | null;
  sinceMs: number;
};

/** A task row enriched for the live view, including its claiming worker and resolved report id. */
export type TaskRow = {
  _id: string;
  source: string;
  prompt: string;
  status: string;
  createdAt: number;
  claimedBy?: string | null;
  claimedAt?: number | null;
  bamlVersion?: string | null;
  reportId?: string | null;
  slackUser?: string | null;
};

/** A worker presence row (heartbeated by every long-lived processor). */
export type Worker = {
  _id: string;
  workerId: string;
  role: string;
  status: 'idle' | 'busy' | string;
  currentItemId?: string | null;
  lastHeartbeat: number;
};

/** A roster row: a presence record joined to whatever it's working on. */
export type WorkerRow = Worker & {
  label?: string | null; // human label of currentItemId (prompt, title, sha)
  href?: string | null; // dashboard link for currentItemId
  sinceMs?: number | null; // how long the current item has been claimed
  inferred?: boolean; // derived from a live claim, no presence heartbeat yet
};

/** A changelog entry row (the absorbed baml-changelog2 data). */
export type ChangelogEntry = {
  _id: string;
  version: string;
  channel: string;
  date?: string | null;
  title?: string | null;
  status: string; // queued | generating | done | failed
  createdAt: number;
  updatedAt?: number;
  claimedAt?: number | null;
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
    cohorts: Record<string, number>;
  };
  inflight: Inflight[];
  runs: RunRow[];
  issues: Issue[];
  builds: Build[];
  tasks: TaskRow[];
  cohorts: Cohort[];
  workers: WorkerRow[];
  changelog: ChangelogEntry[];
  agents: {
    activeTasks: number;
    workers: number;
    dedupers: number;
    fixers: number;
    online: number; // presence rows with a fresh heartbeat
    busy: number; // presence rows currently claiming an item
  };
  totals: {
    tasks: number;
    trophies: number;
    openIssues: number;
    costUsd: number;
  };
};

/**
 * Counts rows grouped by a derived key, for the per-table status breakdowns.
 * @param rows - the rows to bucket
 * @param key - derives the grouping key for a row (blank keys fall back to "?")
 * @returns a map of key to count
 */
function tally<T extends { status?: string }>(
  rows: T[],
  key: (r: T) => string,
): Record<string, number> {
  const out: Record<string, number> = {};
  for (const r of rows) {
    const k = key(r) || '?';
    out[k] = (out[k] ?? 0) + 1;
  }
  return out;
}

// statuses that mean "a processor is actively working on this right now"
const INFLIGHT: Array<[string, string, string, (r: any) => boolean]> = [
  ['task', 'worker', 'running', (r) => r.status === 'running'],
  ['trophy', 'dedup', 'deduping', (r) => r.status === 'deduping'],
  ['issue', 'notion-sync', 'syncing', (r) => r.notionSyncStatus === 'syncing'],
  ['build', 'baml-build', 'building', (r) => r.status === 'building'],
  ['cohort', 'cohort-compare', 'comparing', (r) => r.status === 'comparing'],
];

/**
 * Loads the live snapshot: status tallies, in-flight work, runs, issues, builds, and agent counts.
 * @returns the live state polled by the dashboard; an unconfigured empty shell if SERVICE_URL is unset
 */
export async function loadState(): Promise<LiveState> {
  const generatedAt = new Date().toISOString().replace(/\.\d+Z$/, 'Z');
  const empty = {
    configured: false,
    generatedAt,
    counts: { tasks: {}, trophies: {}, issues: {}, builds: {}, cohorts: {} },
    inflight: [],
    runs: [],
    issues: [],
    builds: [],
    tasks: [],
    cohorts: [],
    workers: [],
    changelog: [],
    agents: { activeTasks: 0, workers: 0, dedupers: 0, fixers: 0, online: 0, busy: 0 },
    totals: { tasks: 0, trophies: 0, openIssues: 0, costUsd: 0 },
  };
  if (!BASE) return empty;
  const [trophies, tasks, issues, builds, cohorts, workers, changelog] =
    await Promise.all([
      get<Trophy[]>('/trophies?limit=60'),
      get<TaskRow[]>('/tasks?limit=200'),
      get<Issue[]>('/issues?limit=100'),
      get<Build[]>('/bamlBuilds?limit=20'),
      get<Cohort[]>('/cohorts?limit=60'),
      // Null-tolerated: these endpoints may not exist until the monolith
      // backend pieces are deployed; the UI sections just render empty.
      get<Worker[]>('/workers?limit=100'),
      get<ChangelogEntry[]>('/changelogEntries?limit=30'),
    ]);
  const T = tasks ?? [],
    TR = trophies ?? [],
    IS = issues ?? [],
    BD = builds ?? [],
    CO = cohorts ?? [],
    WK = workers ?? [],
    CL = changelog ?? [];
  const now = Date.now();
  const taskById = new Map(T.map((t) => [t._id, t]));
  const reportByTask = new Map(TR.map((tr) => [tr.taskId, tr._id]));

  const runs: RunRow[] = TR.map((tr) => {
    const m = tr.metrics ?? {};
    const task = taskById.get(tr.taskId);
    return {
      trophyId: tr._id,
      prompt: task?.prompt ?? '(task not found)',
      source: task?.source ?? '?',
      outcome: tr.outcome,
      turns: m.turns ?? null,
      apiCalls: m.api_calls ?? null,
      outputTokens: m.output_tokens ?? null,
      costUsd: m.estimated_cost_usd ?? null,
      wallS:
        m.wall_clock_ms != null
          ? Math.round((m.wall_clock_ms / 1000) * 10) / 10
          : null,
      bamlVersion: tr.bamlVersion ?? null,
      findings: (tr.findings ?? []).length,
      createdAt: tr.createdAt,
    };
  });

  const inflight: Inflight[] = [];
  const pools: Record<string, any[]> = {
    task: T,
    trophy: TR,
    issue: IS,
    build: BD,
    cohort: CO,
  };
  for (const [kind, stage, , pred] of INFLIGHT) {
    for (const r of pools[kind]) {
      if (!pred(r)) continue;
      const label =
        kind === 'task'
          ? (taskById.get(r._id)?.prompt ?? r.prompt ?? '')
          : kind === 'trophy'
            ? (taskById.get(r.taskId)?.prompt ?? 'dedup batch')
            : kind === 'issue'
              ? r.title
              : kind === 'cohort'
                ? (r.prompt ?? 'skill arena')
                : `baml ${String(r.sha).slice(0, 8)}`;
      inflight.push({
        kind,
        stage,
        id: r._id,
        label: String(label).slice(0, 80),
        claimedBy: r.claimedBy ?? null,
        sinceMs: now - (r.claimedAt ?? r.updatedAt ?? r.createdAt ?? now),
      });
    }
  }

  // ---- agents roster: presence rows joined to whatever they're working on ----
  const itemRef = (id?: string | null): { label: string; href: string } | null => {
    if (!id) return null;
    const task = taskById.get(id);
    if (task) {
      const rid = reportByTask.get(id);
      return {
        label: (task.prompt ?? '').slice(0, 80),
        href: rid ? `/runs/${rid}` : `/tasks/${id}`,
      };
    }
    const trophy = TR.find((t) => t._id === id);
    if (trophy) {
      return {
        label: (taskById.get(trophy.taskId)?.prompt ?? 'dedup batch').slice(0, 80),
        href: `/runs/${id}`,
      };
    }
    const issue = IS.find((i) => i._id === id);
    if (issue) return { label: issue.title.slice(0, 80), href: `/issues/${id}` };
    const cohort = CO.find((c) => c._id === id);
    if (cohort) return { label: (cohort.prompt ?? 'skill arena').slice(0, 80), href: `/cohorts/${id}` };
    const build = BD.find((b) => b._id === id);
    if (build) return { label: `baml ${String(build.sha).slice(0, 8)}`, href: `/db/bamlBuilds` };
    const entry = CL.find((e) => e._id === id);
    if (entry) return { label: `changelog ${entry.version}`, href: `/changelog` };
    return null;
  };
  const claimSince = (id?: string | null): number | null => {
    if (!id) return null;
    const pools: Array<Array<{ _id: string; claimedAt?: number | null }>> = [
      T as any, TR as any, IS as any, BD as any, CO as any, CL as any,
    ];
    for (const pool of pools) {
      const row = pool.find((r) => r._id === id);
      if (row?.claimedAt) return now - row.claimedAt;
    }
    return null;
  };
  const workerRows: WorkerRow[] = WK.map((w) => {
    const ref = itemRef(w.currentItemId);
    return {
      ...w,
      label: ref?.label ?? null,
      href: ref?.href ?? null,
      sinceMs: claimSince(w.currentItemId),
    };
  });
  // Workers implied by live claims but absent from the presence table render
  // as "(inferred)" rows, so the roster works before heartbeats roll out.
  const presenceIds = new Set(WK.map((w) => w.workerId));
  for (const f of inflight) {
    if (!f.claimedBy || presenceIds.has(f.claimedBy)) continue;
    presenceIds.add(f.claimedBy);
    const ref = itemRef(f.id);
    workerRows.push({
      _id: `inferred-${f.claimedBy}`,
      workerId: f.claimedBy,
      role: f.claimedBy.split('-')[0] ?? f.stage,
      status: 'busy',
      currentItemId: f.id,
      lastHeartbeat: now,
      label: ref?.label ?? f.label,
      href: ref?.href ?? null,
      sinceMs: f.sinceMs,
      inferred: true,
    });
  }
  workerRows.sort(
    (a, b) =>
      (b.status === 'busy' ? 1 : 0) - (a.status === 'busy' ? 1 : 0) ||
      a.role.localeCompare(b.role) ||
      b.lastHeartbeat - a.lastHeartbeat,
  );
  const FRESH_MS = 3 * 60 * 1000;
  const online = workerRows.filter((w) => now - w.lastHeartbeat < FRESH_MS).length;
  const busy = workerRows.filter(
    (w) => w.status === 'busy' && now - w.lastHeartbeat < FRESH_MS,
  ).length;

  const openIssues = IS.filter((i) => OPEN_ISSUE_STATUSES.has(i.status));
  // The /db/issues view shows every issue with its status (incl. closed /
  // rejected); open ones first, then most-recently-seen. `openIssues` still
  // drives the totals badge below.
  const allIssues = [...IS].sort(
    (a, b) =>
      (OPEN_ISSUE_STATUSES.has(b.status) ? 1 : 0) -
        (OPEN_ISSUE_STATUSES.has(a.status) ? 1 : 0) ||
      b.lastSeenAt - a.lastSeenAt,
  );
  const costUsd =
    Math.round(runs.reduce((a, r) => a + (r.costUsd ?? 0), 0) * 10000) / 10000;
  const byStage = (st: string) => inflight.filter((f) => f.stage === st).length;
  const agents = {
    // "active" = an agent is currently working on it (derived from in-flight
    // claims, so it scales naturally if more workers are added later)
    activeTasks: T.filter((t) => !['done', 'failed'].includes(t.status)).length,
    workers: byStage('worker'),
    dedupers: byStage('dedup'),
    fixers: byStage('notion-sync'),
    online,
    busy,
  };
  return {
    configured: true,
    generatedAt,
    counts: {
      tasks: tally(T, (r) => r.status),
      trophies: tally(TR, (r) => r.status),
      issues: tally(IS, issueStatusLabel),
      builds: tally(BD, (r) => r.status),
      cohorts: tally(CO, (r) => r.status),
    },
    inflight,
    runs,
    issues: allIssues,
    builds: BD,
    tasks: T.slice(0, 40).map((t) => ({
      ...t,
      reportId: reportByTask.get(t._id) ?? null,
    })),
    cohorts: CO,
    workers: workerRows,
    changelog: CL,
    agents,
    totals: {
      tasks: T.length,
      trophies: TR.length,
      openIssues: openIssues.length,
      costUsd,
    },
  };
}

/** A run-detail bundle for the run page: the trophy, its task, and a readable baml version. */
export type RunDetail = {
  trophy: Trophy;
  task: Task | null;
  bamlLabel: string | null; // readable alpha version (e.g. "0.11.0-alpha.4166")
  transcriptText: string | null; // raw Claude Code transcript blob, for the Raw view
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
  if (sha === 'coldstart') return 'cold start'; // not a real sha — a mode marker
  const builds = await get<Build[]>(
    `/bamlBuilds?field=sha&value=${sha}&index=by_sha&limit=1`,
  );
  const ref = builds?.[0]?.ref;
  return ref ? ref.replace(/^baml-language-/, '') : null;
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
  // The raw transcript blob backs the Ctrl-F-able "Raw" terminal view.
  const transcriptText = trophy.transcriptStorageId
    ? await getText(`/transcripts/${trophy.transcriptStorageId}`)
    : null;
  return { trophy, task: task ?? null, bamlLabel, transcriptText };
}

/** An issue-detail bundle: the issue plus its evidence runs joined to task prompts. */
export type IssueDetail = {
  issue: Issue;
  evidenceRuns: Array<{
    trophyId: string;
    callIndex: number | null;
    prompt: string | null;
    outcome: string | null;
    turns: number | null;
    costUsd: number | null;
    createdAt: number | null;
  }>;
};

/**
 * Loads a single issue plus its evidence trophies (joined to task prompts for labels).
 * @param id - the issue id to load
 * @returns the issue detail, or null if unconfigured or the issue is not found
 */
export async function loadIssue(id: string): Promise<IssueDetail | null> {
  if (!BASE) return null;
  const issue = await get<Issue>(`/issues/${id}`);
  if (!issue) return null;
  const evidence = issue.evidence ?? [];
  const trophyIds = [
    ...new Set(evidence.map((e) => e.trophyId).filter(Boolean)),
  ] as string[];
  const trophies = await Promise.all(
    trophyIds.map((tid) => get<Trophy>(`/trophies/${tid}`)),
  );
  const trophyById = new Map(
    trophies.filter(Boolean).map((t) => [t!._id, t!]),
  );
  const taskIds = [
    ...new Set([...trophyById.values()].map((t) => t.taskId).filter(Boolean)),
  ];
  const tasks = await Promise.all(
    taskIds.map((tid) => get<Task>(`/tasks/${tid}`)),
  );
  const taskById = new Map(tasks.filter(Boolean).map((t) => [t!._id, t!]));
  const evidenceRuns = evidence
    .filter((e) => e.trophyId)
    .map((e) => {
      const trophy = trophyById.get(e.trophyId!);
      const task = trophy ? taskById.get(trophy.taskId) : undefined;
      return {
        trophyId: e.trophyId!,
        callIndex: e.call_index ?? null,
        prompt: task?.prompt ?? null,
        outcome: trophy?.outcome ?? null,
        turns: trophy?.metrics?.turns ?? null,
        costUsd: trophy?.metrics?.estimated_cost_usd ?? null,
        createdAt: trophy?.createdAt ?? null,
      };
    });
  return { issue, evidenceRuns };
}

/**
 * Loads a task plus the id of any trophy it has produced (used to redirect to the result).
 * @param id - the task id to load
 * @returns the task, its trophy id (if any), and a readable baml label; or null if unconfigured/not found
 */
export async function loadTask(
  id: string,
): Promise<{
  task: TaskRow;
  trophyId: string | null;
  bamlLabel: string | null;
} | null> {
  if (!BASE) return null;
  const task = await get<TaskRow>(`/tasks/${id}`);
  if (!task) return null;
  const trophies = await get<Trophy[]>(
    `/trophies?field=taskId&value=${id}&index=by_task&limit=3`,
  );
  const trophyId = trophies && trophies[0] ? trophies[0]._id : null;
  const bamlLabel = await bamlLabelForSha(
    task.bamlVersion ?? trophies?.[0]?.bamlVersion,
  );
  return { task, trophyId, bamlLabel };
}

/** One skill-arena variant: a member task, its held trophy, and that run's headline stats. */
export type CohortVariant = {
  skillRef: string | null;
  taskId: string;
  trophyId: string | null;
  outcome: string | null;
  status: string; // member task status
  turns: number | null;
  costUsd: number | null;
  findings: number;
  /** The exact skill text this variant onboarded from (worker snapshot), when stored. */
  skillText: string | null;
};

/** A cohort-detail bundle: the cohort, its per-branch variants, and the comparison report. */
export type CohortDetail = {
  cohort: Cohort;
  variants: CohortVariant[];
  reportTrophyId: string | null;
  /** The comparison trophy's narrative, inlined so the cohort page tells the whole story. */
  report: { summary: string | null; reportMd: string | null } | null;
};

/**
 * Loads a single cohort plus its variant member runs (joined to their held trophies).
 * @param id - the cohort id to load
 * @returns the cohort detail, or null if unconfigured or the cohort is not found
 */
export async function loadCohort(id: string): Promise<CohortDetail | null> {
  if (!BASE) return null;
  const cohort = await get<Cohort>(`/cohorts/${id}`);
  if (!cohort) return null;
  const members =
    (await get<Task[]>(`/tasks?field=cohortId&value=${id}&index=by_cohort&limit=50`)) ??
    [];
  const variants: CohortVariant[] = await Promise.all(
    members.map(async (m) => {
      const trophies = await get<Trophy[]>(
        `/trophies?field=taskId&value=${m._id}&index=by_task&limit=3`,
      );
      // The member's own held trophy (never the cohort report, which is anchored
      // to a representative member id).
      const tr = (trophies ?? []).find((t) => !t.isCohortReport) ?? null;
      const skillId = (m as Task & { skillStorageId?: string | null }).skillStorageId;
      return {
        skillRef: m.skillRef ?? null,
        taskId: m._id,
        trophyId: tr?._id ?? null,
        outcome: tr?.outcome ?? null,
        status: m.status,
        turns: tr?.metrics?.turns ?? null,
        costUsd: tr?.metrics?.estimated_cost_usd ?? null,
        findings: (tr?.findings ?? []).length,
        skillText: skillId ? await getText(`/transcripts/${skillId}`) : null,
      };
    }),
  );
  const reportTrophy = cohort.reportTrophyId
    ? await get<Trophy>(`/trophies/${cohort.reportTrophyId}`)
    : null;
  return {
    cohort,
    variants,
    reportTrophyId: cohort.reportTrophyId ?? null,
    report: reportTrophy
      ? {
          summary: reportTrophy.summary ?? null,
          reportMd: reportTrophy.reportMd ?? null,
        }
      : null,
  };
}
