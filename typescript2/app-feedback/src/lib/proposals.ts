import "server-only";

export interface Proposal {
  id: string; pr: string; head: string; summary: string; plan: string;
  status: "pending" | "approved" | "executing" | "pushed" | "failed" | "stale";
  approved_by: string | null; created_at: string;
}
// Private proposals may contain CI diagnostics. Never expose them through anon RLS.
// This credential is used only after GitHub authorization in server routes/pages.
export async function proposalStore<T>(path: string, init: RequestInit = {}): Promise<T> {
  const base = process.env.FEEDBACK_SUPABASE_URL;
  const key = process.env.FEEDBACK_APPROVAL_SUPABASE_KEY;
  if (!base || !key || new URL(base).protocol !== "https:") throw new Error("Approval store unavailable");
  const response = await fetch(`${base.replace(/\/$/, "")}/rest/v1/babysit_proposals?${path}`, {
    ...init, headers: { apikey: key, Authorization: `Bearer ${key}`, "Content-Type": "application/json", Prefer: "return=representation" },
    cache: "no-store", redirect: "error", signal: AbortSignal.timeout(10000),
  });
  if (!response.ok) throw new Error("Approval store request failed");
  return response.json();
}
export async function loadProposal(id: string): Promise<Proposal | null> {
  const rows = await proposalStore<Proposal[]>(`select=id,pr,head,summary,plan,status,approved_by,created_at&id=eq.${encodeURIComponent(id)}&dataset=eq.live&limit=1`);
  return rows[0] ?? null;
}
