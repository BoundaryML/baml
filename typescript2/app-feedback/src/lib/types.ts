// Supabase issue and run records shared with MiniATB.
// Includes historical records written before MiniATB.

export type Subsystem =
  | "Syntax"
  | "Compiler"
  | "Runtime"
  | "StdLibrary"
  | "Tooling"
  | "Unknown";

export type Difficulty = "Trivial" | "Easy" | "Medium" | "Hard";

export type IssueStatus =
  | { state: "open" }
  | { state: "in_progress"; pr: string | null }
  | { state: "cancelled"; reason: string; by: string }
  | { state: "rejected"; reason: string }
  | { state: "deferred"; reason: string; workaround: string | null }
  | { state: "merged"; pr: string }
  | { state: "shipped"; version: string; date: string };

export type StatusState = IssueStatus["state"];

export type Expectation =
  | { check: "should_compile" }
  | { check: "should_not_compile"; diagnostic_contains: string | null }
  | { check: "should_evaluate_to"; expected: unknown }
  | { check: "requires_inspection"; instructions: string };

export interface Repro {
  /** Why this command and expectation; written by the authoring model. */
  rationale?: string | null;
  observed?: string | null;
  verified_version?: string | null;
  source_files?: Record<string, string> | null;
  source_observed?: string | null;
  verification?: "python_unittest" | null;
  result?: "passes" | "fails" | "inconclusive" | "invalid_repro" | "unsupported_verification" | "verification_error" | null;
  files: Record<string, string>;
  command: string;
  setup: string | null;
  expectation: Expectation;
}

export interface Comment {
  author: string;
  body: string;
  at: string;
  /** "github" (synced from the reported issue), "website", "slack"; undefined on old rows. */
  source?: "github" | "website" | "slack" | null;
  /** Permalink to the original comment, when synced. */
  url?: string | null;
}

/** A historical cross-issue finding retained for existing records. */
export interface Intuition {
  id: string;
  title: string;
  kind: "Pattern" | "SharedCause" | "Hotspot" | "Process";
  insight: string;
  evidence: string;
  issue_ids: string[];
  subsystem: Subsystem;
  confidence: "low" | "medium" | "high";
  suggested_action: string;
  generated_at: string;
}

/** ~/.atb2/runs/<branch>/outcome.json, as handle_issue writes it. */
export interface HandleOutcome {
  kind: "fixed" | "hard" | "agent_stopped";
  branch: string | null;
  pr: string | null;
  turns: number;
  seconds: number;
  timed_out: boolean;
  design_doc: string | null;
  reason: string | null;
  /** Which pass the run was in when it stopped (mock-only). */
  running?: "design" | "fix" | "pr";
}

export type IssueKind = "bug" | "feature";

export interface Issue {
  pipeline_phase?: "design" | "fix";
  id: string;
  /** A bug, or a feature request whose `resolution_plan` is the proposed feature. */
  kind: IssueKind;
  title: string;
  description: string;
  shepherd: string | null;
  subsystem: Subsystem;
  repros: Repro[];
  version: string;
  feedback_ids: string[];
  status: IssueStatus;
  comments: Comment[];
  resolution_plan: string | null;
  difficulty: Difficulty | null;
  design_doc: string | null;
  /** Last handle_issue run for this issue, if any. */
  outcome: HandleOutcome | null;
  /** live: a real report and what the pipeline did with it. eval: written by the evals. */
  dataset?: Dataset;
  created_at: string;
  updated_at: string;
}

export type Dataset = "live" | "eval";
