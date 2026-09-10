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
// Without them the pages render the mock dataset, so the UI can be built
// and previewed with nothing provisioned; the header says which it is.

import { validProposalId } from "./proposals";
import { ISSUES, findIssue } from "./mock-data";
import type { Comment, HandleOutcome, Issue } from "./types";

const URL = process.env.FEEDBACK_SUPABASE_URL?.replace(/\/$/, "");
const KEY = process.env.FEEDBACK_SUPABASE_ANON_KEY;

export type DataSource = "supabase" | "mock";

export const dataSource: DataSource = URL && KEY ? "supabase" : "mock";

/** Seconds a page result is cached before PostgREST is asked again. */
export const REVALIDATE_S = 30;

/** Milliseconds a PostgREST request may take before the page fails instead of hanging. */
const TIMEOUT_MS = 10_000;

async function rest<T>(path: string): Promise<T> {
  if (!URL || !KEY) throw new Error("supabase is not configured");
  let res: Response;
  try {
    res = await fetch(`${URL}/rest/v1/${path}`, {
      headers: { apikey: KEY, Authorization: `Bearer ${KEY}`, Accept: "application/json" },
      next: { revalidate: REVALIDATE_S },
      signal: AbortSignal.timeout(TIMEOUT_MS),
    });
  } catch (err) {
    if (err instanceof Error && err.name === "TimeoutError") {
      throw new Error(`supabase: no answer within ${TIMEOUT_MS / 1000}s for ${path}`);
    }
    throw err;
  }
  if (!res.ok) {
    throw new Error(`supabase: ${res.status} for ${path}: ${(await res.text()).slice(0, 200)}`);
  }
  return (await res.json()) as T;
}

/** A row of the `issues_with_outcome` view: an issues row plus its latest run. */
interface IssueRow {
  id: string;
  title: string;
  description: string;
  shepherd: string | null;
  subsystem: Issue["subsystem"];
  repros: Issue["repros"];
  version: string;
  feedback_ids: string[];
  status: Issue["status"];
  comments: Array<{ author: string; body: string; at?: string }>;
  resolution_plan: string | null;
  difficulty: Issue["difficulty"];
  design_doc: string | null;
  dataset: "live" | "eval";
  created_at: string;
  updated_at: string;
  outcome: (Partial<HandleOutcome> & { id?: number; mode?: string; created_at?: string }) | null;
}

function outcomeOf(row: IssueRow): HandleOutcome | null {
  const o = row.outcome;
  if (!o || !o.kind) return null;
  return {
    kind: o.kind,
    branch: o.branch ?? null,
    pr: o.pr ?? null,
    turns: o.turns ?? 0,
    seconds: o.seconds ?? 0,
    timed_out: o.timed_out ?? false,
    gate: o.gate ?? null,
    design_doc: o.design_doc ?? null,
    reason: o.reason ?? null,
  };
}

function issueOf(row: IssueRow): Issue {
  const comments: Comment[] = (row.comments ?? []).map((c) => ({
    author: c.author,
    body: c.body,
    at: c.at ?? row.updated_at,
  }));
  return {
    id: row.id,
    title: row.title,
    description: row.description,
    shepherd: row.shepherd,
    subsystem: row.subsystem,
    repros: row.repros ?? [],
    version: row.version,
    feedback_ids: row.feedback_ids ?? [],
    status: row.status,
    comments,
    resolution_plan: row.resolution_plan,
    difficulty: row.difficulty,
    design_doc: row.design_doc,
    outcome: outcomeOf(row),
    dataset: row.dataset ?? "live",
    created_at: row.created_at,
    updated_at: row.updated_at,
  };
}

const COLUMNS = "select=*";

/** Visible issues, most recently updated first. Cancelled rows remain in the store. */
export async function loadIssues(): Promise<Issue[]> {
  if (dataSource === "mock") return ISSUES.filter((i) => i.status.state !== "cancelled").map((i) => ({ ...i, dataset: i.dataset ?? "live" }));
  const rows = await rest<IssueRow[]>(`issues_with_outcome?${COLUMNS}&status->>state=neq.cancelled&order=updated_at.desc`);
  return rows.map(issueOf);
}

/** One issue by id, or undefined. */
export async function loadIssue(id: string): Promise<Issue | undefined> {
  if (dataSource === "mock") {
    const issue = findIssue(id);
    return issue?.status.state === "cancelled" ? undefined : issue;
  }
  const rows = await rest<IssueRow[]>(
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

export async function loadIssueEvents(id: string, dataset: "live" | "eval" = "live", feedbackIds: string[] = []): Promise<IssueEvent[]> {
  if (dataSource === "mock") return [];
  const ids = feedbackIds.filter(id => /^[A-Za-z0-9_-]+$/.test(id)).slice(0, 100);
  const [issueEvents, feedbackEvents, receipts] = await Promise.all([
    rest<IssueEvent[]>(`events?select=id,kind,payload,slack_ts,created_at&issue_id=eq.${encodeURIComponent(id)}&dataset=eq.${dataset}&order=created_at,id`),
    ids.length ? rest<IssueEvent[]>(`events?select=id,kind,payload,slack_ts,created_at&feedback_id=in.(${ids.join(",")})&dataset=eq.${dataset}&order=created_at,id`) : [],
    ids.length ? rest<Array<{id:string;received_at:string;source:string}>>(`feedback_public?select=id,received_at,source&id=in.(${ids.join(",")})`) : [],
  ]);
  const ingested: IssueEvent[] = receipts.map((receipt,i) => ({id:-(i+1),kind:"ingested",created_at:receipt.received_at,slack_ts:null,payload:{source:receipt.source,summary:`Report ${receipt.id} received.`}}));
  return [...new Map([...issueEvents, ...feedbackEvents].filter(event => !ingested.length || event.kind !== "ingested").map(event => [event.id, event])).values(), ...ingested]
    .sort((a,b) => a.created_at.localeCompare(b.created_at) || a.id-b.id);
}

/** Public lifecycle and proposed-fix summaries; raw diagnostics stay private. */
export async function loadPrEvents(number: string, dataset: "live" | "eval" = "live"): Promise<IssueEvent[]> {
  if (!/^[1-9][0-9]{0,9}$/.test(number)) throw new Error("Invalid PR number");
  if (dataSource === "mock") return [];
  const pr = `https://github.com/BoundaryML/baml/pull/${number}`;
  return rest<IssueEvent[]>(
    `events?select=id,kind,payload,slack_ts,created_at&payload->>pr=eq.${encodeURIComponent(pr)}&dataset=eq.${dataset}&order=created_at,id`,
  );
}

/** Only the deliberately public summary and lifecycle, never the private proposal table. */
export async function loadProposalEvents(id: string, dataset: "live" | "eval" = "live"): Promise<IssueEvent[]> {
  if (!validProposalId(id)) throw new Error("Invalid proposal ID");
  if (dataSource === "mock") return [];
  return rest<IssueEvent[]>(
    `events?select=id,kind,payload,slack_ts,created_at&payload->>proposal_id=eq.${encodeURIComponent(id)}&dataset=eq.${dataset}&order=created_at,id&limit=100`,
  );
}
