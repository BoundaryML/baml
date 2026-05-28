import { useEffect, useRef, useState, useCallback } from "react";
import { Badge } from "@/components/ui/badge";
import { TimingChart, type Series } from "@/components/TimingChart";
import { toPng } from "html-to-image";
import type { ApiResponse, RunData, RefEntry, TimingResult } from "./types";

// Trim the top percentile of samples (outlier removal)
function trimTimes(times: number[], trimPct: number): number[] {
  if (trimPct <= 0 || times.length <= 2) return times;
  // Find the cutoff value, then filter — preserves chronological order
  const sorted = [...times].sort((a, b) => a - b);
  const keep = Math.max(2, Math.ceil(sorted.length * (1 - trimPct / 100)));
  const cutoff = sorted[keep - 1];
  return times.filter((t) => t <= cutoff);
}

function recomputeStats(res: TimingResult | null | undefined, trimPct: number): TimingResult | null {
  if (!res || !res.times || res.times.length === 0) return res ?? null;
  const t = trimTimes(res.times, trimPct);
  const sorted = [...t].sort((a, b) => a - b);
  const med = sorted[Math.floor(sorted.length / 2)];
  const mean = t.reduce((a, b) => a + b, 0) / t.length;
  const sd = Math.sqrt(t.reduce((s, x) => s + (x - mean) ** 2, 0) / t.length);
  return { med, sd, times: t };
}

function fmtMs(seconds: number | undefined | null): string {
  if (seconds == null) return "—";
  const ms = seconds * 1000;
  if (ms >= 100) return `${ms.toFixed(0)}ms`;
  if (ms >= 10) return `${ms.toFixed(1)}ms`;
  return `${ms.toFixed(2)}ms`;
}

function deltaPct(old: number | null, cur: number | null): number | null {
  if (old == null || cur == null || old <= 0) return null;
  return ((cur - old) / old) * 100;
}

function isSignificant(a: TimingResult | null | undefined, b: TimingResult | null | undefined): boolean {
  if (!a || !b || !a.med || !b.med) return false;
  const aCov = a.sd && a.med > 0 ? a.sd / a.med : 0;
  const bCov = b.sd && b.med > 0 ? b.sd / b.med : 0;
  const noise = 2.0 * Math.sqrt(aCov * aCov + bCov * bCov);
  return Math.abs(a.med / b.med - 1.0) > noise;
}

function timeAgo(iso: string): string {
  const ms = Date.now() - new Date(iso).getTime();
  const mins = Math.floor(ms / 60000);
  if (mins < 1) return "just now";
  if (mins < 60) return `${mins}m ago`;
  const hrs = Math.floor(mins / 60);
  if (hrs < 24) return `${hrs}h ago`;
  return `${Math.floor(hrs / 24)}d ago`;
}

function shortenPath(p: string | null | undefined): string {
  if (!p) return "unknown";
  return p.replace(/^\/Users\/[^/]+\//, "~/");
}

export function App() {
  const [data, setData] = useState<ApiResponse>({ runs: [], refs: [] });
  const [beforeSel, setBeforeSel] = useState<string | null>(null);
  const [afterSel, setAfterSel] = useState<string | null>(null);
  const [selectedWorkload, setSelectedWorkload] = useState<string | null>(null);
  const [trimPct, setTrimPct] = useState(0);
  const [copying, setCopying] = useState<string | null>(null);
  const tableRef = useRef<HTMLDivElement>(null);
  const detailRef = useRef<HTMLDivElement>(null);

  const copyAsImage = useCallback(async (el: HTMLElement | null, label: string) => {
    if (!el) return;
    setCopying(label);
    try {
      const dataUrl = await toPng(el, {
        backgroundColor: "#0d1117",
        pixelRatio: 2,
        style: { padding: "16px", overflow: "visible", maxHeight: "none" },
        width: el.scrollWidth + 32,
        height: el.scrollHeight + 32,
      });
      const res = await fetch(dataUrl);
      const blob = await res.blob();
      await navigator.clipboard.write([
        new ClipboardItem({ "image/png": blob }),
      ]);
      setCopying(`${label} ✓`);
      setTimeout(() => setCopying(null), 1500);
    } catch (e) {
      console.error("Copy failed:", e);
      setCopying(null);
    }
  }, []);
  // Extra language references: map of label -> {runId, runner}
  const [extraRunners, setExtraRunners] = useState<Map<string, { sel: string; runId: string; runner: string }>>(new Map());
  const didAutoSelect = useRef(false);

  const toggleExtra = (sel: string, runId: string, runner: string) => {
    const key = `${runner}`;
    setExtraRunners((prev) => {
      const next = new Map(prev);
      if (next.has(key) && next.get(key)!.sel === sel) {
        next.delete(key);
      } else {
        next.set(key, { sel, runId, runner });
      }
      return next;
    });
  };

  const resolve = (sel: string | null): RunData | undefined => {
    if (!sel) return undefined;
    const ref = data.refs.find((r) => r.ref === sel);
    if (ref) return data.runs.find((r) => r.run_id === ref.run_id);
    return data.runs.find((r) => r.run_id === sel);
  };

  const resolvedRunId = (sel: string | null): string | null => {
    if (!sel) return null;
    const ref = data.refs.find((r) => r.ref === sel);
    return ref?.run_id ?? (data.runs.find((r) => r.run_id === sel) ? sel : null);
  };

  const fetchRuns = async () => {
    const res = await fetch("/api/runs");
    const newData: ApiResponse = await res.json();
    setData(newData);
    if (!didAutoSelect.current && newData.refs.length >= 2) {
      didAutoSelect.current = true;
      const latestRefs = newData.refs.filter((r) => r.tag === "latest");
      if (latestRefs.length >= 2) {
        setBeforeSel(latestRefs[0].ref);
        setAfterSel(latestRefs[1].ref);
      }
    }
  };

  useEffect(() => {
    fetchRuns();
    const iv = setInterval(fetchRuns, 5000);
    return () => clearInterval(iv);
  }, []);

  // Group refs by branch
  const branches = new Map<string, RefEntry[]>();
  for (const ref of data.refs) {
    if (!branches.has(ref.branch)) branches.set(ref.branch, []);
    branches.get(ref.branch)!.push(ref);
  }
  const referencedRunIds = new Set(data.refs.map((r) => r.run_id));
  const orphanRuns = data.runs.filter((r) => !referencedRunIds.has(r.run_id));

  // Group orphans by their branch (from cli.git.branch metadata)
  const orphansByBranch = new Map<string, RunData[]>();
  for (const run of orphanRuns) {
    const branch = run.cli?.git?.branch ?? "NO_BRANCH";
    if (!orphansByBranch.has(branch)) orphansByBranch.set(branch, []);
    orphansByBranch.get(branch)!.push(run);
  }

  const beforeRunId = resolvedRunId(beforeSel);
  const afterRunId = resolvedRunId(afterSel);
  const before = resolve(beforeSel);
  const after = resolve(afterSel);
  const hasComparison = before && after;
  const selected = after ?? before;

  const beforeByName = new Map(before?.workloads?.map((w) => [w.name, w]) ?? []);
  const afterByName = new Map(after?.workloads?.map((w) => [w.name, w]) ?? []);
  const categories = new Map<string, { name: string; category: string }[]>();
  const wseen = new Set<string>();
  for (const w of [...(before?.workloads ?? []), ...(after?.workloads ?? [])]) {
    if (wseen.has(w.name)) continue;
    wseen.add(w.name);
    if (!categories.has(w.category)) categories.set(w.category, []);
    categories.get(w.category)!.push({ name: w.name, category: w.category });
  }

  // Pre-compute summary stats so we can show legend at the top
  let faster = 0, slower = 0, unchanged = 0;
  if (hasComparison) {
    for (const [, workloads] of categories) {
      for (const entry of workloads) {
        const aR = recomputeStats(beforeByName.get(entry.name)?.results?.baml, trimPct);
        const bR = recomputeStats(afterByName.get(entry.name)?.results?.baml, trimPct);
        const p = deltaPct(aR?.med ?? null, bR?.med ?? null);
        if (p !== null) {
          const notable = isSignificant(aR, bR) || Math.abs(p) > 5;
          if (notable) { if (p < 0) faster++; else slower++; }
          else { unchanged++; }
        }
      }
    }
  }

  const RUNNER_COLORS: Record<string, { bg: string; hover: string; text: string; hex: string }> = {
    python: { bg: "bg-blue-400/80", hover: "hover:bg-blue-500/20 hover:text-blue-400", text: "text-blue-400/60", hex: "#60a5fa" },
    node: { bg: "bg-violet-400/80", hover: "hover:bg-violet-500/20 hover:text-violet-400", text: "text-violet-400/60", hex: "#a78bfa" },
    bun: { bg: "bg-pink-400/80", hover: "hover:bg-pink-500/20 hover:text-pink-400", text: "text-pink-400/60", hex: "#f472b6" },
  };

  const SelectButtons = ({ refStr, runId, run }: { refStr: string; runId: string; run: RunData }) => {
    const isBefore = beforeSel === refStr || beforeRunId === runId;
    const isAfter = afterSel === refStr || afterRunId === runId;
    // Check which extra runners this run has
    const hasRunner = (r: string) => run.workloads?.some((w) => w.results[r as keyof typeof w.results] != null);

    return (
      <div className="flex gap-1 flex-wrap">
        <button onClick={() => setBeforeSel(isBefore ? null : refStr)}
          className={`px-2 py-0.5 rounded text-[10px] font-medium transition-colors ${isBefore ? "bg-amber-500 text-black" : "bg-muted text-muted-foreground hover:bg-amber-500/20 hover:text-amber-500"}`}
        >before</button>
        <button onClick={() => setAfterSel(isAfter ? null : refStr)}
          className={`px-2 py-0.5 rounded text-[10px] font-medium transition-colors ${isAfter ? "bg-emerald-500 text-black" : "bg-muted text-muted-foreground hover:bg-emerald-500/20 hover:text-emerald-500"}`}
        >after</button>
        {(["python", "node", "bun"] as const).filter(hasRunner).map((runner) => {
          const c = RUNNER_COLORS[runner];
          const isActive = extraRunners.has(runner) && extraRunners.get(runner)!.sel === refStr;
          return (
            <button key={runner} onClick={() => toggleExtra(refStr, runId, runner)}
              className={`px-2 py-0.5 rounded text-[10px] font-medium transition-colors ${isActive ? `${c.bg} text-black` : `bg-muted text-muted-foreground ${c.hover}`}`}
            >{runner}</button>
          );
        })}
      </div>
    );
  };

  const runHighlight = (runId: string): "before" | "after" | null => {
    if (beforeRunId === runId) return "before";
    if (afterRunId === runId) return "after";
    return null;
  };

  // Detail panel data
  const selWA = selectedWorkload ? beforeByName.get(selectedWorkload) : null;
  const selWB = selectedWorkload ? afterByName.get(selectedWorkload) : null;
  const selSource = selWB?.source ?? selWA?.source;

  return (
    <div className="max-w-[1400px] mx-auto py-6 px-6">
      <h1 className="text-xl font-bold tracking-tight mb-4 font-mono">speedtest</h1>

      {/* Run picker */}
      <div className="space-y-2 mb-6">
        {Array.from(branches.entries()).map(([branch, branchRefs]) => {
          const branchRunIds = [...new Set(branchRefs.map((r) => r.run_id))];
          const branchRuns = branchRunIds.map((id) => data.runs.find((r) => r.run_id === id)).filter(Boolean) as RunData[];
          branchRuns.sort((a, b) => (b.timestamp ?? "").localeCompare(a.timestamp ?? ""));
          const runTags = new Map<string, string[]>();
          for (const ref of branchRefs) {
            if (!runTags.has(ref.run_id)) runTags.set(ref.run_id, []);
            runTags.get(ref.run_id)!.push(ref.tag);
          }

          return (
            <div key={branch} className="border border-border rounded-lg overflow-hidden">
              <div className="bg-muted/30 px-3 py-1.5 border-b border-border">
                <span className="text-xs font-semibold font-mono">{branch}</span>
              </div>
              <div className="divide-y divide-border">
                {[...branchRuns, ...(orphansByBranch.get(branch) ?? [])].sort((a, b) => (b.timestamp ?? "").localeCompare(a.timestamp ?? "")).map((run) => {
                  const tags = runTags.get(run.run_id) ?? [];
                  const latestRef = branchRefs.find((r) => r.run_id === run.run_id && r.tag === "latest");
                  const selKey = latestRef?.ref ?? run.run_id;
                  const hl = runHighlight(run.run_id);
                  const git = run.cli?.git;
                  return (
                    <div key={run.run_id} className={`flex items-center gap-2 px-3 py-1.5 text-xs ${
                      hl === "before" ? "bg-amber-500/10 border-l-2 border-amber-500" : hl === "after" ? "bg-emerald-500/10 border-l-2 border-emerald-500" : "border-l-2 border-transparent"
                    }`}>
                      <div className="flex-1 min-w-0">
                        <div className="flex items-center gap-1 mb-0.5">
                          {tags.map((t) => <Badge key={t} variant={t === "latest" ? "default" : "outline"} className="text-[9px] px-1 py-0">{t}</Badge>)}
                          <span className="text-muted-foreground/60 font-mono truncate">{shortenPath(run.cli?.path)}</span>
                        </div>
                        <div className="text-muted-foreground truncate">
                          {git?.in_repo ? (
                            <><span className="font-mono">{git.commit?.slice(0, 7)}</span> {git.commit_date && timeAgo(git.commit_date)} {git.message && <span className="italic">{git.message}</span>}</>
                          ) : <span className="italic">not in a git repo</span>}
                          <span className="ml-2 text-muted-foreground/50">{run.workloads?.length ?? 0} workloads · ran {run.timestamp ? timeAgo(run.timestamp) : "?"}</span>
                        </div>
                      </div>
                      <SelectButtons refStr={selKey} runId={run.run_id} run={run} />
                    </div>
                  );
                })}
              </div>
            </div>
          );
        })}
        {/* Orphan runs on branches that have no refs at all */}
        {Array.from(orphansByBranch.entries())
          .filter(([branch]) => !branches.has(branch))
          .map(([branch, runs]) => (
            <div key={`orphan-${branch}`} className="border border-border rounded-lg overflow-hidden">
              <div className="bg-muted/30 px-3 py-1.5 border-b border-border">
                <span className="text-xs font-semibold font-mono text-muted-foreground">{branch}</span>
              </div>
              <div className="divide-y divide-border">
                {runs.sort((a, b) => (b.timestamp ?? "").localeCompare(a.timestamp ?? "")).map((run) => {
                  const hl = runHighlight(run.run_id);
                  const git = run.cli?.git;
                  return (
                    <div key={run.run_id} className={`flex items-center gap-2 px-3 py-1.5 text-xs ${
                      hl === "before" ? "bg-amber-500/10 border-l-2 border-amber-500" : hl === "after" ? "bg-emerald-500/10 border-l-2 border-emerald-500" : "border-l-2 border-transparent"
                    }`}>
                      <div className="flex-1 min-w-0 text-muted-foreground truncate">
                        <span className="font-mono">{git?.commit?.slice(0, 7) ?? run.run_id}</span>
                        {git?.message && <span className="ml-2 italic">{git.message}</span>}
                        <span className="ml-2 text-muted-foreground/50">{run.workloads?.length} workloads · ran {run.timestamp ? timeAgo(run.timestamp) : "?"}</span>
                      </div>
                      <SelectButtons refStr={run.run_id} runId={run.run_id} run={run} />
                    </div>
                  );
                })}
              </div>
            </div>
          ))}
      </div>

      {/* Main content: table left, detail panel right */}
      {selected && (
        <div className="flex gap-6">
          {/* Left: workload table */}
          <div className={selectedWorkload ? "w-[55%] flex-shrink-0" : "flex-1"} ref={tableRef}>
            {/* Info line */}
            <div className="text-[10px] text-muted-foreground mb-3 font-mono space-y-0.5">
              {before && <div><span className="text-amber-500 font-semibold">before</span>: {beforeSel}{before.cli?.built_at && <> · built {before.cli.built_at.slice(0, 16).replace("T", " ")}</>}</div>}
              {after && <div><span className="text-emerald-500 font-semibold">after</span>: {afterSel}{after.cli?.built_at && <> · built {after.cli.built_at.slice(0, 16).replace("T", " ")}</>}</div>}
            </div>

            {/* Summary + legend at top */}
            {hasComparison && (
              <div className="mb-3 font-mono space-y-1">
                <div className="text-xs text-muted-foreground flex gap-3">
                  {faster > 0 && <span className="text-green-500 font-semibold">{faster} faster</span>}
                  {slower > 0 && <span className="text-red-500 font-semibold">{slower} slower</span>}
                  {unchanged > 0 && <span>{unchanged} unchanged</span>}
                </div>
                <div className="text-[9px] text-muted-foreground/50 flex gap-3">
                  <span><span className="text-green-500">green</span> = faster (&gt;5% or significant)</span>
                  <span><span className="text-red-500">red</span> = slower</span>
                  <span><span className="text-muted-foreground">gray</span> = noise</span>
                </div>
              </div>
            )}

            {/* Controls */}
            <div className="flex items-center gap-3 mb-2 text-xs text-muted-foreground">
              <label className="flex items-center gap-2">
                <span>Trim outliers:</span>
                <input type="range" min={0} max={40} step={5} value={trimPct}
                  onChange={(e) => setTrimPct(Number(e.target.value))}
                  className="w-24 accent-emerald-500" />
                <span className="font-mono w-10">{trimPct}%</span>
              </label>
              {trimPct > 0 && <span className="text-muted-foreground/50">removing top {trimPct}%</span>}
              <button
                onClick={() => copyAsImage(tableRef.current, "table")}
                className="ml-auto px-2.5 py-1 rounded-md border border-border bg-muted hover:bg-muted/80 text-foreground text-xs font-medium transition-colors flex items-center gap-1.5"
              >{copying === "table ✓" ? "✓ Copied!" : "📋 Copy table as image"}</button>
            </div>

            {/* Column headers */}
            <div className="flex items-baseline text-[9px] uppercase tracking-widest mb-1 px-2 font-mono">
              <span className="flex-1 text-muted-foreground">Workload</span>
              {hasComparison ? (
                <>
                  <span className="w-16 text-right text-amber-500">before</span>
                  <span className="w-6" />
                  <span className="w-16 text-right text-emerald-500">after</span>
                  <span className="w-20 text-right text-muted-foreground">change</span>
                </>
              ) : (
                <span className="w-16 text-right text-emerald-500">time</span>
              )}
              {Array.from(extraRunners.entries()).map(([key, { runner }]) => {
                const c = RUNNER_COLORS[runner];
                return (
                  <span key={key} className="contents">
                    <span className={`w-16 text-right ${c.text}`}>{runner}</span>
                    <span className="w-14 text-right text-muted-foreground">vs {hasComparison ? "after" : "baml"}</span>
                  </span>
                );
              })}
            </div>

            {/* Results by category */}
            {Array.from(categories.entries()).map(([cat, workloads]) => (
              <div key={cat} className="mb-4 font-mono">
                <div className="text-[9px] text-muted-foreground uppercase tracking-widest mb-1 border-b border-border pb-0.5">{cat}</div>
                {workloads.map((entry) => {
                  const aResRaw = beforeByName.get(entry.name)?.results?.baml;
                  const bResRaw = afterByName.get(entry.name)?.results?.baml;
                  const aRes = recomputeStats(aResRaw, trimPct);
                  const bRes = recomputeStats(bResRaw, trimPct);
                  const singleRes = bRes ?? aRes;
                  const pct = hasComparison ? deltaPct(aRes?.med ?? null, bRes?.med ?? null) : null;
                  const sig = hasComparison ? isSignificant(aRes, bRes) : false;
                  const notable = sig || (pct !== null && Math.abs(pct) > 5);
                  const displayName = entry.name.includes("::") ? entry.name.split("::")[1] : entry.name;
                  const isActive = selectedWorkload === entry.name;

                  return (
                    <div
                      key={entry.name}
                      className={`flex items-baseline py-1 text-xs hover:bg-muted/30 px-2 -mx-2 rounded cursor-pointer select-none ${isActive ? "bg-muted/50 ring-1 ring-border" : ""}`}
                      onClick={() => setSelectedWorkload(isActive ? null : entry.name)}
                    >
                      <span className="flex-1 truncate">{displayName}</span>
                      {hasComparison ? (
                        <>
                          <span className="w-16 text-right text-amber-500">{fmtMs(aRes?.med)}</span>
                          <span className="w-6 text-center text-amber-700">→</span>
                          <span className="w-16 text-right text-emerald-400">{fmtMs(bRes?.med)}</span>
                          <span className={`w-20 text-right font-semibold ${
                            pct === null ? "" : !notable ? "text-muted-foreground" : pct < 0 ? "text-green-500" : "text-red-500"
                          }`}>{pct !== null ? `${pct >= 0 ? "+" : ""}${pct.toFixed(1)}%` : "—"}</span>
                        </>
                      ) : (
                        <span className="w-16 text-right text-emerald-400">{fmtMs(singleRes?.med)}</span>
                      )}
                      {Array.from(extraRunners.entries()).map(([key, { runner }]) => {
                        const extraRun = resolve(extraRunners.get(key)!.sel);
                        const extraW = extraRun?.workloads?.find((w) => w.name === entry.name);
                        const extraRes = recomputeStats(extraW?.results?.[runner as keyof typeof extraW.results] as TimingResult | null, trimPct);
                        const c = RUNNER_COLORS[runner];
                        const bamlRes = bRes ?? singleRes;
                        const ratio = bamlRes?.med && extraRes?.med ? bamlRes.med / extraRes.med : null;
                        const sig = isSignificant(bamlRes, extraRes);
                        const notableRatio = sig || (ratio !== null && Math.abs(ratio - 1) > 0.05);
                        return (
                          <span key={key} className="contents">
                            <span className={`w-16 text-right ${c.text}`}>{fmtMs(extraRes?.med)}</span>
                            <span className={`w-14 text-right ${
                              ratio === null ? "" : !notableRatio ? "text-muted-foreground" : ratio > 1 ? "text-red-400" : "text-green-400"
                            }`}>
                              {ratio !== null ? `${ratio >= 10 ? ratio.toFixed(0) : ratio.toFixed(1)}x` : "—"}
                            </span>
                          </span>
                        );
                      })}
                    </div>
                  );
                })}
              </div>
            ))}

          </div>

          {/* Right: detail panel */}
          {selectedWorkload && (
            <div className="flex-1 border-l border-border pl-6 min-w-0 sticky top-6 self-start max-h-[calc(100vh-80px)] overflow-y-auto" ref={detailRef}>
              <div className="flex items-center justify-between mb-3">
                <h2 className="text-sm font-semibold font-mono truncate">
                  {selectedWorkload.includes("::") ? selectedWorkload.split("::")[1] : selectedWorkload}
                </h2>
                <div className="flex items-center gap-1">
                  <button
                    onClick={() => copyAsImage(detailRef.current, "detail")}
                    className="px-2.5 py-1 rounded-md border border-border bg-muted hover:bg-muted/80 text-foreground text-xs font-medium transition-colors"
                  >{copying === "detail ✓" ? "✓ Copied!" : "📋 Copy"}</button>
                  <button onClick={() => setSelectedWorkload(null)} className="text-muted-foreground hover:text-foreground text-xs px-1">✕</button>
                </div>
              </div>

              {/* Chart — all active runners */}
              {(() => {
                const chartSeries: Series[] = [];
                if (hasComparison && selWA?.results?.baml) {
                  chartSeries.push({ label: `before`, color: "#f59e0b", data: recomputeStats(selWA.results.baml, trimPct) });
                }
                const afterBaml = selWB?.results?.baml ?? selWA?.results?.baml;
                if (afterBaml) {
                  chartSeries.push({ label: hasComparison ? `after` : `baml`, color: "#10b981", data: recomputeStats(afterBaml, trimPct) });
                }
                for (const [key, { sel, runner }] of extraRunners) {
                  const extraRun = resolve(sel);
                  const extraW = extraRun?.workloads?.find((w) => w.name === selectedWorkload);
                  const extraRes = extraW?.results?.[runner as keyof typeof extraW.results] as TimingResult | null;
                  if (extraRes) {
                    chartSeries.push({ label: runner, color: RUNNER_COLORS[runner]?.hex ?? "#888", data: recomputeStats(extraRes, trimPct) });
                  }
                }
                return <TimingChart series={chartSeries} />;
              })()}

              {/* Source code — show diff if before/after differ */}
              {(selWA?.source || selWB?.source) && (() => {
                const srcA = selWA?.source ?? {};
                const srcB = selWB?.source ?? {};
                const langs: { key: "baml" | "python" | "js"; label: string }[] = [
                  { key: "baml", label: "BAML" },
                  { key: "python", label: "Python" },
                  { key: "js", label: "JavaScript" },
                ];
                return (
                  <div className="mt-4 space-y-3">
                    {langs.map(({ key, label }) => {
                      const a = srcA[key]?.trim();
                      const b = srcB[key]?.trim();
                      if (!a && !b) return null;
                      const changed = hasComparison && a && b && a !== b;
                      const onlyOne = a || b;

                      return (
                        <div key={key}>
                          <div className="flex items-center gap-2 mb-1">
                            <span className="text-[10px] text-muted-foreground uppercase tracking-widest">{label}</span>
                            {changed && <Badge variant="outline" className="text-[9px] px-1 py-0 text-yellow-500 border-yellow-500/30">changed</Badge>}
                          </div>
                          {changed ? (
                            <div className="grid grid-cols-2 gap-2">
                              <div>
                                <div className="text-[9px] text-amber-500 mb-0.5">before</div>
                                <pre className="p-2 bg-muted/30 rounded text-[10px] overflow-x-auto whitespace-pre font-mono max-h-48 overflow-y-auto border border-amber-500/20">{a}</pre>
                              </div>
                              <div>
                                <div className="text-[9px] text-emerald-500 mb-0.5">after</div>
                                <pre className="p-2 bg-muted/30 rounded text-[10px] overflow-x-auto whitespace-pre font-mono max-h-48 overflow-y-auto border border-emerald-500/20">{b}</pre>
                              </div>
                            </div>
                          ) : (
                            <pre className="p-2 bg-muted/30 rounded text-[11px] overflow-x-auto whitespace-pre font-mono max-h-48 overflow-y-auto">{onlyOne}</pre>
                          )}
                        </div>
                      );
                    })}
                  </div>
                );
              })()}
            </div>
          )}
        </div>
      )}

      {!selected && (
        <div className="text-sm text-muted-foreground text-center py-8">
          Select <span className="text-amber-500">before</span> + <span className="text-emerald-500">after</span> above to compare
        </div>
      )}
    </div>
  );
}
