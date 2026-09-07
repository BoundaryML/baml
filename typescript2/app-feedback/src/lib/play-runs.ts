import "server-only";
import { currentApprover, runPath } from "./approval-auth";

export interface Turn { role: "assistant" | "user"; content: { type: string; name?: string; text: string }[] }
export interface PlayRun {
  id: number; prompt: string; play_status: string; created_at: string;
  report: { summary?: string; worked?: boolean } | null;
  turns: number; tokens: number | null; seconds: number; canary_sha: string | null;
  feedback_ids: string[]; transcript?: Turn[] | null;
  feedback_issues?: { id: string; issue_ids: string[] }[];
}
const fields = "id,prompt,play_status,created_at,report,turns,tokens,seconds,canary_sha,feedback_ids";

// Authorization lives beside the privileged read so every caller must pass it.
export async function playRuns(id?: string): Promise<PlayRun[] | null> {
  if (id !== undefined) runPath(id);
  if (!await currentApprover()) return null;
  const base = new URL(process.env.FEEDBACK_SUPABASE_URL ?? "");
  const key = process.env.FEEDBACK_APPROVAL_SUPABASE_KEY;
  if (base.protocol !== "https:" || base.username || base.password || base.pathname !== "/" || base.search || base.hash || !key) {
    throw new Error("Run store unavailable");
  }
  const url = new URL("/rest/v1/runs", base);
  url.search = new URLSearchParams({ select: fields + (id ? ",transcript" : ""), dataset: "eq.live", kind: "eq.play",
    order: "created_at.desc,id.desc", limit: id ? "1" : "50", ...(id ? { id: `eq.${id}` } : {}) }).toString();
  const response = await fetch(url, { headers: { apikey: key, Authorization: `Bearer ${key}` },
    cache: "no-store", redirect: "error", signal: AbortSignal.timeout(10000) });
  if (!response.ok) throw new Error("Run store unavailable");
  const rows: PlayRun[] = await response.json();
  const ids = id && rows[0] ? rows[0].feedback_ids.filter(value => /^[a-zA-Z0-9-]{1,100}$/.test(value)).slice(0, 3) : [];
  if (ids.length) {
    const feedback = new URL("/rest/v1/feedback_public", base);
    feedback.search = new URLSearchParams({ select: "id,issue_ids", dataset: "eq.live", id: `in.(${ids.join(",")})`, limit: "3" }).toString();
    const links = await fetch(feedback, { headers: { apikey: key, Authorization: `Bearer ${key}` },
      cache: "no-store", redirect: "error", signal: AbortSignal.timeout(10000) });
    // A temporary triage-read failure does not hide an otherwise available run.
    if (links.ok) rows[0].feedback_issues = await links.json();
  }
  return rows;
}
