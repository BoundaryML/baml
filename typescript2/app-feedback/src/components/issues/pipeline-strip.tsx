import { cn } from "@/lib/utils";
import { STAGE_LABELS, type StageInfo, type StageState } from "@/lib/pipeline";

const STATE_CLASS: Record<StageState, string> = {
  done: "bg-stage-done",
  running: "bg-stage-running stage-running",
  failed: "bg-stage-failed",
  skipped: "bg-stage-todo opacity-40",
  todo: "bg-stage-todo",
};

/** Seven segments, one per pipeline stage. Compact enough for a list row. */
export function PipelineStrip({ stages, className }: { stages: StageInfo[]; className?: string }) {
  return (
    <div className={cn("flex items-center gap-1", className)} aria-label="pipeline progress">
      {stages.map((s) => (
        <div
          key={s.stage}
          title={`${STAGE_LABELS[s.stage]}: ${s.detail}`}
          className={cn("h-1.5 w-5 rounded-sm", STATE_CLASS[s.state])}
        />
      ))}
    </div>
  );
}

/** The strip with labels under it, for a detail page or a wide row. */
export function PipelineStripLabeled({ stages }: { stages: StageInfo[] }) {
  return (
    <div className="grid grid-cols-7 gap-1">
      {stages.map((s) => (
        <div key={s.stage} className="min-w-0">
          <div className={cn("h-2 rounded-sm", STATE_CLASS[s.state])} />
          <div className="mt-1 text-[11px] leading-tight text-muted-foreground truncate">
            {STAGE_LABELS[s.stage]}
          </div>
        </div>
      ))}
    </div>
  );
}
