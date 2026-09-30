// The data source: the atb2 store in Supabase,
// read through PostgREST with the anon key, which sees issues, runs and
// events but never a reporter's identity (feedback only via feedback_public).
//
//   FEEDBACK_SUPABASE_URL       https://igraichzcidsylvzkjlc.supabase.co (as in Infisical)
//   FEEDBACK_SUPABASE_ANON_KEY  the anon key (never the service key: this is a public site)
//
// Server-only names on purpose: this module runs in server components, and a
// NEXT_PUBLIC_ variable would be inlined into the browser bundle.
//
// Without them every loader throws, so a misconfigured deploy fails loudly.

import { validProposalId } from './proposals';
import type { Comment, HandleOutcome, Intuition, Issue } from './types';

const URL = process.env.FEEDBACK_SUPABASE_URL?.replace(/\/$/, '');
const KEY = process.env.FEEDBACK_SUPABASE_ANON_KEY;

/** Seconds a page result is cached before PostgREST is asked again. */
export const REVALIDATE_S = 30;

/** Milliseconds a PostgREST request may take before the page fails instead of hanging. */
const TIMEOUT_MS = 10_000;

async function rest<T>(path: string): Promise<T> {
  if (!URL || !KEY) throw new Error('supabase is not configured');
  let res: Response;
  try {
    res = await fetch(`${URL}/rest/v1/${path}`, {
      headers: {
        Accept: 'application/json',
        Authorization: `Bearer ${KEY}`,
        apikey: KEY,
      },
      next: { revalidate: REVALIDATE_S },
      signal: AbortSignal.timeout(TIMEOUT_MS),
    });
  } catch (err) {
    if (err instanceof Error && err.name === 'TimeoutError') {
      throw new Error(
        `supabase: no answer within ${TIMEOUT_MS / 1000}s for ${path}`,
      );
    }
    throw err;
  }
  if (!res.ok) {
    throw new Error(
      `supabase: ${res.status} for ${path}: ${(await res.text()).slice(0, 200)}`,
    );
  }
  return (await res.json()) as T;
}

/** A row of the `issues_with_outcome` view: an issues row plus its latest run. */
interface IssueRow {
  id: string;
  kind?: string | null;
  title: string;
  description: string;
  shepherd: string | null;
  subsystem: Issue['subsystem'];
  repros: Issue['repros'];
  version: string;
  feedback_ids: string[];
  status: Issue['status'];
  comments: Array<{
    author: string;
    body: string;
    at?: string;
    source?: Comment['source'];
    url?: string | null;
  }>;
  resolution_plan: string | null;
  difficulty: Issue['difficulty'];
  design_doc: string | null;
  dataset: 'live' | 'eval';
  created_at: string;
  updated_at: string;
  outcome:
    | (Partial<HandleOutcome> & {
        id?: number;
        mode?: string;
        created_at?: string;
      })
    | null;
}

/** Older deployed outcome views predate kind; read the authoritative field without DDL. */
async function issueRows(path: string): Promise<IssueRow[]> {
  const rows = await rest<IssueRow[]>(path);
  const missing = rows.filter(
    (row) => row.kind == null && /^[A-Za-z0-9_-]+$/.test(row.id),
  );
  const batches: IssueRow[][] = [];
  for (let i = 0; i < missing.length; i += 30)
    batches.push(missing.slice(i, i + 30));
  const kinds = await Promise.all(
    batches.map((batch) =>
      rest<Array<{ id: string; kind: string }>>(
        `issues?select=id,kind&id=in.(${batch.map((row) => encodeURIComponent(row.id)).join(',')})`,
      ),
    ),
  );
  const byId = new Map(kinds.flat().map((row) => [row.id, row.kind]));
  return rows.map((row) => ({ ...row, kind: row.kind ?? byId.get(row.id) }));
}

function outcomeOf(row: IssueRow): HandleOutcome | null {
  const o = row.outcome;
  if (!o || !o.kind) return null;
  return {
    branch: o.branch ?? null,
    design_doc: o.design_doc ?? null,
    kind:
      o.kind === 'fixed' || o.kind === 'hard' || o.kind === 'agent_stopped'
        ? o.kind
        : 'agent_stopped',
    pr: o.pr ?? null,
    reason: o.reason ?? null,
    running:
      o.running === 'design' || o.running === 'fix' || o.running === 'pr'
        ? o.running
        : undefined,
    seconds: o.seconds ?? 0,
    timed_out: o.timed_out ?? false,
    turns: o.turns ?? 0,
  };
}

function issueOf(row: IssueRow): Issue {
  const comments: Comment[] = (row.comments ?? []).map((c) => ({
    at: c.at ?? row.updated_at,
    author: c.author,
    body: c.body,
    source: c.source ?? null,
    url:
      typeof c.url === 'string' &&
      /^https:\/\/github\.com\/BoundaryML\/baml\/issues\/[1-9][0-9]*#issuecomment-[0-9]+$/.test(
        c.url,
      )
        ? c.url
        : null,
  }));
  return {
    comments,
    created_at: row.created_at,
    dataset: row.dataset ?? 'live',
    description: row.description,
    design_doc: row.design_doc,
    difficulty: row.difficulty,
    feedback_ids: row.feedback_ids ?? [],
    id: row.id,
    kind: row.kind === 'feature' ? 'feature' : 'bug',
    outcome: outcomeOf(row),
    repros: row.repros ?? [],
    resolution_plan: row.resolution_plan,
    shepherd: row.shepherd,
    status: row.status,
    subsystem: row.subsystem,
    title: row.title,
    updated_at: row.updated_at,
    version: row.version,
  };
}

const COLUMNS = 'select=*';

/** Visible issues, most recently updated first. Cancelled rows remain in the store. */
export async function loadIssues(): Promise<Issue[]> {
  const rows = await issueRows(
    `issues_with_outcome?${COLUMNS}&status->>state=neq.cancelled&order=updated_at.desc`,
  );
  const phases = await rest<
    Array<{ issue_id: string; dataset: string; kind: string }>
  >(
    'events?select=issue_id,dataset,kind&kind=in.(design_started,implementation_started)&order=created_at.desc,id.desc&limit=1000',
  );
  const latest = new Map<string, 'design' | 'fix'>();
  for (const event of phases) {
    const key = `${event.dataset}:${event.issue_id}`;
    if (!latest.has(key))
      latest.set(key, event.kind === 'design_started' ? 'design' : 'fix');
  }
  return rows.map((row) => ({
    ...issueOf(row),
    pipeline_phase: latest.get(`${row.dataset}:${row.id}`),
  }));
}

/** One issue by id, or undefined. */
export async function loadIssue(id: string): Promise<Issue | undefined> {
  const rows = await issueRows(
    `issues_with_outcome?${COLUMNS}&status->>state=neq.cancelled&id=eq.${encodeURIComponent(id)}&limit=1`,
  );
  return rows[0] ? issueOf(rows[0]) : undefined;
}

/** The pipeline's audit trail for one issue, oldest first. */
export interface IssueEvent {
  id: number;
  kind: string;
  payload: Record<string, unknown>;
  slack_ts: string | null;
  created_at: string;
}

export async function loadIssueEvents(
  id: string,
  dataset: 'live' | 'eval' = 'live',
  feedbackIds: string[] = [],
): Promise<IssueEvent[]> {
  const ids = feedbackIds
    .filter((id) => /^[A-Za-z0-9_-]+$/.test(id))
    .slice(0, 100);
  const [issueEvents, feedbackEvents, receipts] = await Promise.all([
    rest<IssueEvent[]>(
      `events?select=id,kind,payload,slack_ts,created_at&issue_id=eq.${encodeURIComponent(id)}&dataset=eq.${dataset}&order=created_at,id`,
    ),
    ids.length
      ? rest<IssueEvent[]>(
          `events?select=id,kind,payload,slack_ts,created_at&feedback_id=in.(${ids.join(',')})&dataset=eq.${dataset}&order=created_at,id`,
        )
      : [],
    ids.length
      ? rest<Array<{ id: string; received_at: string; source: string }>>(
          `feedback_public?select=id,received_at,source&id=in.(${ids.join(',')})`,
        )
      : [],
  ]);
  const ingested: IssueEvent[] = receipts.map((receipt, i) => ({
    created_at: receipt.received_at,
    id: -(i + 1),
    kind: 'ingested',
    payload: {
      source: receipt.source,
      summary: `Report ${receipt.id} received.`,
    },
    slack_ts: null,
  }));
  return [
    ...new Map(
      [...issueEvents, ...feedbackEvents]
        .filter((event) => !ingested.length || event.kind !== 'ingested')
        .map((event) => [event.id, event]),
    ).values(),
    ...ingested,
  ].sort((a, b) => a.created_at.localeCompare(b.created_at) || a.id - b.id);
}

/** Public lifecycle and proposed-fix summaries; raw diagnostics stay private. */
export async function loadPrEvents(
  number: string,
  dataset: 'live' | 'eval' = 'live',
): Promise<IssueEvent[]> {
  if (!/^[1-9][0-9]{0,9}$/.test(number)) throw new Error('Invalid PR number');
  const pr = `https://github.com/BoundaryML/baml/pull/${number}`;
  return rest<IssueEvent[]>(
    `events?select=id,kind,payload,slack_ts,created_at&payload->>pr=eq.${encodeURIComponent(pr)}&dataset=eq.${dataset}&order=created_at,id`,
  );
}

/** Only the deliberately public summary and lifecycle, never the private proposal table. */
export async function loadProposalEvents(
  id: string,
  dataset: 'live' | 'eval' = 'live',
): Promise<IssueEvent[]> {
  if (!validProposalId(id)) throw new Error('Invalid proposal ID');
  return rest<IssueEvent[]>(
    `events?select=id,kind,payload,slack_ts,created_at&payload->>proposal_id=eq.${encodeURIComponent(id)}&dataset=eq.${dataset}&order=created_at,id&limit=100`,
  );
}

/** Other issues filed from the same reports (a split bug + feature request, or a
 * report merged into several tickets). Cancelled rows are excluded. */
export async function loadSiblingIssues(
  issue: Pick<Issue, 'id' | 'feedback_ids'>,
): Promise<Issue[]> {
  const ids = issue.feedback_ids
    .filter((id) => /^[A-Za-z0-9_-]+$/.test(id))
    .slice(0, 20);
  if (!ids.length) return [];
  const seen = new Map<string, Issue>();
  for (const id of ids) {
    // feedback_ids is a Postgres array on the live table and JSON on older rows: try both shapes.
    for (const filter of [
      `feedback_ids=cs.${encodeURIComponent(`{${id}}`)}`,
      `feedback_ids=cs.${encodeURIComponent(JSON.stringify([id]))}`,
    ]) {
      try {
        const rows = await issueRows(
          `issues_with_outcome?${COLUMNS}&status->>state=neq.cancelled&${filter}&limit=20`,
        );
        for (const row of rows)
          if (row.id !== issue.id) seen.set(row.id, issueOf(row));
        break;
      } catch {
        /* try the other shape */
      }
    }
  }
  return [...seen.values()];
}

const INTUITION_COLUMNS =
  'select=id,title,kind,insight,evidence,issue_ids,subsystem,confidence,suggested_action,generated_at';

function intuitionOf(row: Intuition): Intuition {
  return {
    ...row,
    issue_ids: Array.isArray(row.issue_ids)
      ? row.issue_ids.filter((id) => typeof id === 'string')
      : [],
  };
}

/** The current set of cross-issue intuitions, strongest first. Empty until the
 * `intuitions` table exists (deploy/sql/intuitions.sql) or the pass has run. */
export async function loadIntuitions(
  dataset: 'live' | 'eval' = 'live',
): Promise<Intuition[]> {
  try {
    const rows = await rest<Intuition[]>(
      `intuitions?${INTUITION_COLUMNS}&dataset=eq.${dataset}&active=is.true&order=generated_at.desc,confidence.desc`,
    );
    const rank = { high: 0, low: 2, medium: 1 };
    return rows
      .map(intuitionOf)
      .sort((a, b) => rank[a.confidence] - rank[b.confidence]);
  } catch {
    return [];
  }
}

/** The active intuitions that cite one issue. */
export async function loadIssueIntuitions(
  id: string,
  dataset: 'live' | 'eval' = 'live',
): Promise<Intuition[]> {
  if (!/^[A-Za-z0-9_-]+$/.test(id)) return [];
  try {
    const rows = await rest<Intuition[]>(
      `intuitions?${INTUITION_COLUMNS}&dataset=eq.${dataset}&active=is.true&issue_ids=cs.${encodeURIComponent(JSON.stringify([id]))}&order=generated_at.desc`,
    );
    return rows.map(intuitionOf);
  } catch {
    return [];
  }
}

/** A report as the public view exposes it: no reporter identity. */
export interface PublicFeedback {
  id: string;
  title: string;
  body: string;
  source: string;
  toolchain: string | null;
  received_at: string;
  issue_ids: string[];
  dataset: 'live' | 'eval';
}

export async function loadFeedback(
  id: string,
): Promise<PublicFeedback | undefined> {
  if (!/^[A-Za-z0-9_-]{1,120}$/.test(id)) return undefined;
  const rows = await rest<PublicFeedback[]>(
    `feedback_public?select=id,title,body,source,toolchain,received_at,issue_ids,dataset&id=eq.${encodeURIComponent(id)}&limit=1`,
  );
  const row = rows[0];
  return row
    ? {
        ...row,
        dataset: row.dataset ?? 'live',
        issue_ids: Array.isArray(row.issue_ids)
          ? row.issue_ids.filter((x) => typeof x === 'string')
          : [],
      }
    : undefined;
}

/** Why a report made no issue, as the pipeline recorded it, or null while it is untriaged. */
export async function loadNoIssueReason(
  id: string,
  dataset: 'live' | 'eval' = 'live',
): Promise<string | null> {
  if (!/^[A-Za-z0-9_-]{1,120}$/.test(id)) return null;
  const rows = await rest<Array<{ payload: Record<string, unknown> }>>(
    `events?select=payload&kind=eq.no_issue&feedback_id=eq.${encodeURIComponent(id)}&dataset=eq.${dataset}&order=created_at.desc&limit=1`,
  );
  return noIssueReason(rows[0]?.payload);
}

function noIssueReason(
  payload: Record<string, unknown> | undefined,
): string | null {
  if (!payload) return null;
  const rawReason = typeof payload.reason === 'string' ? payload.reason : null;
  const labels: Record<string, string> = {
    invalid_repro: 'Invalid reproduction',
    unsupported_verification: 'Verification unavailable',
    verification_error: 'Verification failed to run',
  };
  const reason = rawReason ? (labels[rawReason] ?? rawReason) : null;
  const summary = typeof payload.summary === 'string' ? payload.summary : null;
  return summary && reason && summary !== reason
    ? `${reason}: ${summary}`
    : (reason ?? summary ?? 'no issue');
}

/** A report on the reports page: what was written, and what the pipeline made of it. */
export interface FeedbackListing extends PublicFeedback {
  issues: Issue[];
  /** The recorded reason the report made no issue; null while nothing was decided. */
  no_issue: string | null;
}

/** Every visible report, newest first, with the issues it became or why it made none. */
export async function loadFeedbackList(
  limit = 200,
): Promise<FeedbackListing[]> {
  const reports = await rest<PublicFeedback[]>(
    `feedback_public?select=id,title,body,source,toolchain,received_at,issue_ids,dataset&order=received_at.desc&limit=${limit}`,
  );
  const clean = reports.map((r) => ({
    ...r,
    dataset: r.dataset ?? 'live',
    issue_ids: Array.isArray(r.issue_ids)
      ? r.issue_ids.filter((x) => typeof x === 'string')
      : [],
  }));
  const issueIds = [...new Set(clean.flatMap((r) => r.issue_ids))];
  const orphanIds = clean
    .filter((r) => r.issue_ids.length === 0)
    .map((r) => r.id)
    .filter((id) => /^[A-Za-z0-9_-]{1,120}$/.test(id));
  const chunks: string[][] = [];
  for (let i = 0; i < issueIds.length; i += 20)
    chunks.push(issueIds.slice(i, i + 20));
  const [issues, decided] = await Promise.all([
    Promise.all(chunks.map(loadIssuesById)).then((all) => all.flat()),
    orphanIds.length
      ? rest<
          Array<{
            feedback_id: string;
            payload: Record<string, unknown>;
            dataset: string;
          }>
        >(
          `events?select=feedback_id,payload,dataset&kind=eq.no_issue&feedback_id=in.(${orphanIds.map(encodeURIComponent).join(',')})&order=created_at.desc`,
        )
      : Promise.resolve([]),
  ]);
  const byId = new Map(issues.map((i) => [i.id, i]));
  const reasons = new Map<string, string | null>();
  for (const e of decided) {
    const key = `${e.dataset ?? 'live'}:${e.feedback_id}`;
    if (!reasons.has(key)) reasons.set(key, noIssueReason(e.payload));
  }
  return clean.map((r) => ({
    ...r,
    issues: r.issue_ids
      .map((id) => byId.get(id))
      .filter((i): i is Issue => Boolean(i)),
    no_issue: r.issue_ids.length
      ? null
      : (reasons.get(`${r.dataset}:${r.id}`) ?? null),
  }));
}

/** Issues by id, for a report's page. */
export async function loadIssuesById(ids: string[]): Promise<Issue[]> {
  const valid = ids
    .filter((id) => /^[A-Za-z0-9_-]{1,120}$/.test(id))
    .slice(0, 20);
  if (!valid.length) return [];
  const rows = await issueRows(
    `issues_with_outcome?${COLUMNS}&status->>state=neq.cancelled&id=in.(${valid.map(encodeURIComponent).join(',')})`,
  );
  return rows.map(issueOf);
}

export interface PendingReport {
  id: string;
  title: string;
  dataset: 'live' | 'eval';
  phase: 'ingested' | 'enriching' | 'investigating';
}

/** Reports are visible before an issue exists; each card disappears when linked or declined. */
export async function loadPendingReports(): Promise<PendingReport[]> {
  const reports = await rest<PublicFeedback[]>(
    'feedback_public?select=id,title,issue_ids,dataset&order=received_at.desc&limit=200',
  );
  const pending = reports.filter(
    (r) => !r.issue_ids?.length && /^[A-Za-z0-9_-]{1,120}$/.test(r.id),
  );
  const groups: PublicFeedback[][] = [];
  for (let i = 0; i < pending.length; i += 30)
    groups.push(pending.slice(i, i + 30));
  const histories = await Promise.all(
    groups.map((group) =>
      rest<Array<IssueEvent & { feedback_id: string; dataset: string }>>(
        `events?select=id,kind,payload,created_at,feedback_id,dataset&feedback_id=in.(${group.map((r) => encodeURIComponent(r.id)).join(',')})&order=created_at.desc,id.desc&limit=1000`,
      ),
    ),
  );
  const latest = new Map<string, string>();
  for (const event of histories.flat()) {
    const key = `${event.dataset ?? 'live'}:${event.feedback_id}`;
    if (!latest.has(key)) latest.set(key, event.kind);
  }
  return pending.flatMap((report) => {
    const dataset = report.dataset ?? 'live';
    const kind = latest.get(`${dataset}:${report.id}`);
    if (kind === 'no_issue') return [];
    const phase =
      kind === 'investigation_started'
        ? 'investigating'
        : kind && kind !== 'ingested'
          ? 'enriching'
          : 'ingested';
    return [
      {
        dataset,
        id: report.id,
        phase,
        title: report.title,
      } satisfies PendingReport,
    ];
  });
}
