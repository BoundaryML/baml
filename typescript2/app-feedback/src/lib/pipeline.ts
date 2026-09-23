import type { Issue } from "./types";

// The pipeline an issue moves through, in order. The first three are the
// triage stages (create_issue / organize_issue / gauge_issue); the rest are
// handle_issue's passes, read from its outcome.json.
export const STAGES = [
  "triaged",
  "investigated",
  "organized",
  "gauged",
  "design",
  "fix",
  "pr",
] as const;

export type Stage = (typeof STAGES)[number];

export type StageState = "done" | "running" | "failed" | "skipped" | "todo";

export interface StageInfo {
  stage: Stage;
  state: StageState;
  /** One line shown in the row's tooltip / detail timeline. */
  detail: string;
}

export const STAGE_LABELS: Record<Stage, string> = {
  triaged: "Triaged",
  investigated: "Investigated",
  organized: "Organized",
  gauged: "Gauged",
  design: "Design pass",
  fix: "Fix pass",
  pr: "PR",
};

export function stageInfo(issue: Issue): StageInfo[] {
  const out: StageInfo[] = [];
  out.push({
    stage: "triaged",
    state: "done",
    detail: `${issue.repros.length} repro${issue.repros.length === 1 ? "" : "s"} from ${issue.feedback_ids.length} report${issue.feedback_ids.length === 1 ? "" : "s"}`,
  });
  out.push({ stage: "investigated", state: /^## (?:Investigation|Where it breaks)\s*$/m.test(issue.description) ? "done" : "todo", detail: "Source investigation" });
  out.push(
    issue.shepherd
      ? { stage: "organized", state: "done", detail: `shepherd: ${issue.shepherd}` }
      : { stage: "organized", state: "todo", detail: "no shepherd yet" },
  );
  out.push(
    issue.difficulty
      ? { stage: "gauged", state: "done", detail: issue.difficulty }
      : { stage: "gauged", state: "todo", detail: "not gauged" },
  );

  const active = issue.status.state === "in_progress" && !issue.status.pr ? issue.pipeline_phase : undefined;
  const o = active ? { ...issue.outcome, running: active, seconds: issue.outcome?.seconds ?? 0, turns: issue.outcome?.turns ?? 0 } as NonNullable<Issue["outcome"]> : issue.outcome;
  const terminal =
    issue.status.state === "cancelled" || issue.status.state === "rejected" || issue.status.state === "deferred";
  const todo = (stage: Stage, detail: string): StageInfo => ({
    stage,
    state: terminal ? "skipped" : "todo",
    detail,
  });

  if (!o && issue.status.state === "in_progress" && issue.status.pr) {
    out.push({ stage: "design", state: "skipped", detail: "No separate design pass recorded" });
    out.push({ stage: "fix", state: "done", detail: "Draft fix published" });
    out.push({ stage: "pr", state: "done", detail: issue.status.pr.replace("https://github.com/", "") });
    return out;
  }
  if (!o) {
    out.push(todo("design", "not started"));
    out.push(todo("fix", "not started"));
    out.push(todo("pr", "none"));
    return out;
  }

  const mins = Math.round(o.seconds / 60);
  const runInfo = `${o.turns} turns, ${mins} min`;

  if (o.running) {
    const order: Stage[] = ["design", "fix", "pr"];
    for (const s of order) {
      if (s === o.running) out.push({ stage: s, state: "running", detail: `running (${runInfo})` });
      else if (order.indexOf(s) < order.indexOf(o.running))
        out.push({ stage: s, state: "done", detail: "done" });
      else out.push({ stage: s, state: "todo", detail: "pending" });
    }
    return out;
  }

  switch (o.kind) {
    case "agent_stopped": {
      const inFix = o.branch !== null && o.design_doc !== null;
      out.push(
        inFix
          ? { stage: "design", state: "done", detail: "plan written" }
          : { stage: "design", state: "failed", detail: o.reason ?? "stopped" },
      );
      out.push(
        inFix
          ? { stage: "fix", state: "failed", detail: o.reason ?? "stopped" }
          : { stage: "fix", state: "skipped", detail: "not reached" },
      );
      out.push({ stage: "pr", state: "skipped", detail: "none" });
      return out;
    }
    case "hard":
      out.push({ stage: "design", state: "done", detail: `design doc written (${runInfo})` });
      out.push({ stage: "fix", state: "skipped", detail: "hard: handed to the shepherd" });
      out.push({ stage: "pr", state: "skipped", detail: "none" });
      return out;
    case "fixed":
      out.push({ stage: "design", state: "done", detail: "plan written" });
      out.push({ stage: "fix", state: "done", detail: runInfo });
      out.push(
        o.pr
          ? { stage: "pr", state: "done", detail: o.pr.replace("https://github.com/", "") }
          : { stage: "pr", state: "todo", detail: "dry run: not pushed" },
      );
      return out;
  }
}

/** 0..1, how far the issue is through the pipeline. */
export function progress(issue: Issue): number {
  const infos = stageInfo(issue);
  const done = infos.filter((s) => s.state === "done").length;
  return done / infos.length;
}

export function statusLabel(issue: Issue): string {
  const s = issue.status;
  switch (s.state) {
    case "open":
      return "Open";
    case "in_progress":
      return s.pr ? "PR open" : "In progress";
    case "merged":
      return "Merged";
    case "shipped":
      return `Shipped ${s.version}`;
    case "deferred":
      return "Deferred";
    case "cancelled":
      return "Cancelled";
    case "rejected":
      return "Rejected";
  }
}

export function formatSeconds(s: number): string {
  if (s < 60) return `${s}s`;
  const m = Math.round(s / 60);
  if (m < 60) return `${m} min`;
  return `${Math.floor(m / 60)}h ${m % 60}m`;
}

export function relativeTime(iso: string, now: number): string {
  const diff = now - new Date(iso).getTime();
  if (diff < 0) return "in the future";
  const minutes = Math.floor(diff / 60000);
  const hours = Math.floor(diff / 3600000);
  const days = Math.floor(diff / 86400000);
  if (minutes < 1) return "just now";
  if (minutes < 60) return `${minutes}m ago`;
  if (hours < 24) return `${hours}h ago`;
  if (days < 30) return `${days}d ago`;
  return new Date(iso).toLocaleDateString();
}

/** Board columns describe the persisted phase, not just the broad open/in_progress status. */
export function boardPhase(issue: Issue): string {
  if (["merged", "shipped", "deferred", "rejected", "cancelled"].includes(issue.status.state)) return issue.status.state;
  if (("pr" in issue.status && issue.status.pr) || issue.outcome?.pr) return "pr";
  if (issue.status.state === "in_progress") return issue.pipeline_phase ?? issue.outcome?.running ?? "design";
  if (issue.outcome?.running) return issue.outcome.running;
  if (issue.outcome?.kind === "agent_stopped") return "attention";
  if (issue.outcome?.kind === "hard") return "attention";
  if (!/^## (?:Investigation|Where it breaks)\s*$/m.test(issue.description)) return "investigating";
  if (!issue.shepherd || !issue.difficulty) return "organizing";
  return "ready";
}
