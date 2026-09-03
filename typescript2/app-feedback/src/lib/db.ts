// The data source: the atb2 store in Supabase (tools/atb2/db/schema.sql),
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

import { ISSUES, findIssue } from "./mock-data";
import type { Comment, HandleOutcome, Issue } from "./types";

const URL = process.env.FEEDBACK_SUPABASE_URL?.replace(/\/$/, "");
const KEY = process.env.FEEDBACK_SUPABASE_ANON_KEY;

export type DataSource = "supabase" | "mock";

export const dataSource: DataSource = URL && KEY ? "supabase" : "mock";

/** Seconds a page result is cached before PostgREST is asked again. */
export const REVALIDATE_S = 30;

async function rest<T>(path: string): Promise<T> {
  if (!URL || !KEY) throw new Error("supabase is not configured");
  const res = await fetch(`${URL}/rest/v1/${path}`, {
    headers: { apikey: KEY, Authorization: `Bearer ${KEY}`, Accept: "application/json" },
    next: { revalidate: REVALIDATE_S },
  });
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

/** Every issue, most recently updated first. */
export async function loadIssues(): Promise<Issue[]> {
  if (dataSource === "mock") return ISSUES.map((i) => ({ ...i, dataset: i.dataset ?? "live" }));
  const rows = await rest<IssueRow[]>(`issues_with_outcome?${COLUMNS}&order=updated_at.desc`);
  return rows.map(issueOf);
}

/** One issue by id, or undefined. */
export async function loadIssue(id: string): Promise<Issue | undefined> {
  if (dataSource === "mock") return findIssue(id);
  const rows = await rest<IssueRow[]>(
    `issues_with_outcome?${COLUMNS}&id=eq.${encodeURIComponent(id)}&limit=1`,
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

export async function loadIssueEvents(id: string): Promise<IssueEvent[]> {
  if (dataSource === "mock") return [];
  return rest<IssueEvent[]>(
    `events?select=id,kind,payload,slack_ts,created_at&issue_id=eq.${encodeURIComponent(id)}&order=created_at`,
  );
}
