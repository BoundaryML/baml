import type { Issue } from "@/lib/types";

function Tile({ label, value, hint }: { label: string; value: string | number; hint?: string }) {
  return (
    <div className="rounded-lg border bg-card px-4 py-3">
      <div className="text-xs text-muted-foreground">{label}</div>
      <div className="mt-1 text-2xl font-semibold tabular-nums">{value}</div>
      {hint && <div className="mt-0.5 text-xs text-muted-foreground">{hint}</div>}
    </div>
  );
}

export function StatTiles({ issues }: { issues: Issue[] }) {
  const open = issues.filter((i) => i.status.state === "open").length;
  const running = issues.filter((i) => i.outcome?.running).length;
  const prs = issues.filter((i) => i.status.state === "in_progress" && i.status.pr).length;
  const landed = issues.filter((i) => i.status.state === "merged" || i.status.state === "shipped").length;
  const needsHuman = issues.filter(
    (i) => i.outcome && (i.outcome.kind === "gate_failed" || i.outcome.kind === "agent_stopped" || i.outcome.kind === "hard"),
  ).length;
  const feedback = new Set(issues.flatMap((i) => i.feedback_ids)).size;

  return (
    <div className="grid grid-cols-2 sm:grid-cols-3 lg:grid-cols-6 gap-3">
      <Tile label="Open" value={open} hint={`${feedback} reports`} />
      <Tile label="Agent running" value={running} />
      <Tile label="Draft PRs" value={prs} />
      <Tile label="Needs a human" value={needsHuman} hint="gate failed / stopped / hard" />
      <Tile label="Landed" value={landed} hint="merged or shipped" />
      <Tile label="Total" value={issues.length} />
    </div>
  );
}
