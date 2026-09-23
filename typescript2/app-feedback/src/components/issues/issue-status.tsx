import { Badge } from "@/components/ui/badge";
import type { Difficulty, Issue, Subsystem } from "@/lib/types";
import { statusLabel } from "@/lib/pipeline";

/** Leads every issue view: a bug, or a feature request. */
export function KindBadge({ kind, className }: { kind: Issue["kind"]; className?: string }) {
  return kind === "feature"
    ? <Badge variant="outline" className={"border-violet-300 bg-violet-50 text-violet-800 dark:border-violet-700 dark:bg-violet-950/50 dark:text-violet-200 " + (className ?? "")}>Feature request</Badge>
    : <Badge variant="outline" className={"border-slate-300 text-slate-700 dark:border-slate-600 dark:text-slate-300 " + (className ?? "")}>Bug</Badge>;
}

export function StatusBadge({ issue }: { issue: Issue }) {
  return <Badge variant={issue.status.state}>{statusLabel(issue)}</Badge>;
}

const DIFFICULTY_VARIANT: Record<Difficulty, "trivial" | "easy" | "medium" | "hard"> = {
  Trivial: "trivial",
  Easy: "easy",
  Medium: "medium",
  Hard: "hard",
};

export function DifficultyBadge({ difficulty }: { difficulty: Difficulty | null }) {
  if (!difficulty) {
    return (
      <Badge variant="outline" className="text-muted-foreground font-normal">
        not gauged
      </Badge>
    );
  }
  return <Badge variant={DIFFICULTY_VARIANT[difficulty]}>{difficulty}</Badge>;
}

const SUBSYSTEM_LABEL: Record<Subsystem, string> = {
  Syntax: "syntax",
  Compiler: "compiler",
  Runtime: "runtime",
  StdLibrary: "stdlib",
  Tooling: "tooling",
  Unknown: "unknown",
};

export function SubsystemBadge({ subsystem }: { subsystem: Subsystem }) {
  return (
    <Badge variant="subsystem" className="font-mono">
      {SUBSYSTEM_LABEL[subsystem]}
    </Badge>
  );
}
