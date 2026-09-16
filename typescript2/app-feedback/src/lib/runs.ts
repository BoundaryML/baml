export interface TranscriptBlock { type: "text" | "tool_use" | "tool_result"; name?: string; text: string }
export interface PlayRun {
  id: number; prompt: string; play_status: string; created_at: string; canary_sha: string | null;
  reason?: string; feedback_ids: string[];
  report: { summary?: string; version?: string; feedback?: Array<{ id: string; title: string; description: string; status: string }> } | null;
  feedback?: Array<{ id: string; title: string; body: string; files: Record<string, string>; issue_ids: string[] }>;
  transcript?: Array<{ role: string; content: TranscriptBlock[] }>;
}
