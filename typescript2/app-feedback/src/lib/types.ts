// Mirrors tools/atb2/baml_src/models.baml (Issue) and handle_issue.baml
// (HandleOutcome). Keep in sync when the BAML models change.

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
  files: Record<string, string>;
  command: string;
  setup: string | null;
  expectation: Expectation;
}

export interface Comment {
  author: string;
  body: string;
  at: string;
}

export interface GateStep {
  name: string;
  ok: boolean;
  seconds: number;
  exit_code: number;
}

export interface GateResult {
  steps: GateStep[];
  ok: boolean;
  changed_crates: string[];
}

/** ~/.atb2/runs/<branch>/outcome.json, as handle_issue writes it. */
export interface HandleOutcome {
  kind: "fixed" | "hard" | "gate_failed" | "agent_stopped";
  branch: string | null;
  pr: string | null;
  turns: number;
  seconds: number;
  timed_out: boolean;
  gate: GateResult | null;
  design_doc: string | null;
  reason: string | null;
  /** Which pass the run was in when it stopped (mock-only). */
  running?: "design" | "fix" | "gate" | "pr";
}

export interface Issue {
  id: string;
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
  created_at: string;
  updated_at: string;
}
