"use client";
import { useState } from "react";
import type { Repro } from "@/lib/types";
import { cn } from "@/lib/utils";
import { ReproCard } from "./repro-card";

function label(r: Repro, i: number): string {
  const what = i === 0 ? "Repro" : `Comparison ${i}`;
  const result = r.result === "fails" ? "fails" : r.result === "passes" ? "passes" : r.result === "inconclusive" ? "inconclusive" : null;
  return result ? `${what} · ${result}` : what;
}

/** One repro at a time: the primary first, comparisons behind tabs. */
export function ReproTabs({ repros }: { repros: Repro[] }) {
  const [active, setActive] = useState(0);
  const current = repros[Math.min(active, repros.length - 1)];
  if (!current) return null;
  return (
    <div>
      {repros.length > 1 && (
        <div role="tablist" className="mb-2 flex flex-wrap gap-1">
          {repros.map((r, i) => (
            <button key={i} role="tab" type="button" aria-selected={i === active} onClick={() => setActive(i)}
              className={cn("rounded-md border px-2.5 py-1 text-xs", i === active ? "bg-foreground text-background" : "bg-card text-muted-foreground hover:text-foreground")}>
              {label(r, i)}
              <span className={cn("ml-1.5 inline-block h-1.5 w-1.5 rounded-full align-middle", r.result === "fails" ? "bg-rose-500" : r.result === "passes" ? "bg-emerald-500" : "bg-slate-400")} aria-hidden />
            </button>
          ))}
        </div>
      )}
      <ReproCard repro={current} comparison={active > 0} />
    </div>
  );
}
