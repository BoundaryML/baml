import 'server-only';
import type { IssueStatus } from './types';
export interface ApprovalIssue {
  id: string;
  shepherd: string | null;
  status: IssueStatus;
}
export function isShepherd(
  login: string | null,
  issue: ApprovalIssue,
): boolean {
  return (
    !!login &&
    !!issue.shepherd &&
    login.toLowerCase() === issue.shepherd.toLowerCase()
  );
}
export async function issueStore<T>(
  query: string,
  init: RequestInit = {},
): Promise<T> {
  const origin = new URL(process.env.FEEDBACK_SUPABASE_URL ?? '');
  const key = process.env.FEEDBACK_APPROVAL_SUPABASE_KEY;
  if (
    !key ||
    origin.protocol !== 'https:' ||
    origin.username ||
    origin.password ||
    origin.pathname !== '/' ||
    origin.search ||
    origin.hash
  )
    throw new Error('Approval store unavailable');
  const response = await fetch(`${origin.origin}/rest/v1/issues?${query}`, {
    ...init,
    cache: 'no-store',
    headers: {
      Authorization: `Bearer ${key}`,
      apikey: key,
      'Content-Type': 'application/json',
      Prefer: 'return=representation',
    },
    redirect: 'error',
    signal: AbortSignal.timeout(5000),
  });
  if (!response.ok) throw new Error('Approval store failed');
  return response.json();
}
