import { useState } from "react";
import type { TimingResult } from "../types";

export interface Series {
  label: string;
  color: string;
  data: TimingResult | null | undefined;
}

interface Props {
  series: Series[];
}

const W = 500;
const H = 100;
const PAD = { top: 12, right: 60, bottom: 4, left: 50 };

function fmtMs(s: number): string {
  const ms = s * 1000;
  if (ms >= 100) return `${ms.toFixed(0)}`;
  if (ms >= 10) return `${ms.toFixed(1)}`;
  return `${ms.toFixed(2)}`;
}

function shortLabel(s: string): string {
  return s.length > 20 ? s.slice(0, 18) + "…" : s;
}

export function TimingChart({ series }: Props) {
  const [hidden, setHidden] = useState<Set<number>>(new Set());

  const toggle = (idx: number) =>
    setHidden((prev) => {
      const next = new Set(prev);
      next.has(idx) ? next.delete(idx) : next.add(idx);
      return next;
    });

  const allSeries = series.filter((s) => s.data && s.data.times.length > 0);
  const visibleSeries = allSeries.filter((_, i) => !hidden.has(i));
  if (allSeries.length === 0) return null;

  // Scale from visible series only (so toggling zooms in)
  const allTimes = visibleSeries.flatMap((s) => s.data!.times);
  if (allTimes.length === 0) return null;
  const minT = Math.min(...allTimes);
  const maxT = Math.max(...allTimes);
  const pad = (maxT - minT) * 0.15 || 1e-6;
  const lo = minT - pad;
  const hi = maxT + pad;
  const range = hi - lo;

  const plotW = W - PAD.left - PAD.right;
  const plotH = H - PAD.top - PAD.bottom;

  const yScale = (t: number) => PAD.top + plotH - ((t - lo) / range) * plotH;
  const xScale = (i: number, total: number) =>
    PAD.left + (total <= 1 ? plotW / 2 : (i / (total - 1)) * plotW);

  const yTicks = [lo, (lo + hi) / 2, hi];

  return (
    <div className="my-2">
      {/* Clickable legend at top */}
      <div className="flex gap-3 text-[10px] mb-2 flex-wrap">
        {allSeries.map((s, i) => {
          const d = s.data!;
          const isHidden = hidden.has(i);
          const cv = d.sd && d.med > 0 ? ((d.sd / d.med) * 100).toFixed(1) : "0";
          return (
            <button
              key={i}
              onClick={() => toggle(i)}
              className={`flex items-center gap-1.5 px-1.5 py-0.5 rounded transition-opacity ${isHidden ? "opacity-30" : "opacity-100"}`}
            >
              <span className="inline-block w-2.5 h-2.5 rounded-full border-2" style={{
                backgroundColor: isHidden ? "transparent" : s.color,
                borderColor: s.color,
              }} />
              <span className="font-semibold" style={{ color: s.color }}>{shortLabel(s.label)}</span>
              <span className="text-muted-foreground">{fmtMs(d.med)}ms ±{cv}% ({d.times.length})</span>
            </button>
          );
        })}
      </div>

      <svg width={W} height={H} viewBox={`0 0 ${W} ${H}`}>
        {/* Y axis */}
        {yTicks.map((t, i) => (
          <g key={i}>
            <line x1={PAD.left} x2={PAD.left + plotW} y1={yScale(t)} y2={yScale(t)} stroke="#ffffff" strokeWidth={0.5} opacity={0.05} />
            <text x={PAD.left - 6} y={yScale(t) + 3} textAnchor="end" fill="#6b7280" fontSize={9}>{fmtMs(t)}ms</text>
          </g>
        ))}

        {/* Each visible series */}
        {allSeries.map((s, si) => {
          if (hidden.has(si)) return null;
          const d = s.data!;
          return (
            <g key={si}>
              {d.sd > 0 && (
                <rect x={PAD.left} y={yScale(d.med + d.sd)} width={plotW}
                  height={Math.max(0, yScale(d.med - d.sd) - yScale(d.med + d.sd))}
                  fill={s.color} opacity={0.06} />
              )}
              <line x1={PAD.left} x2={PAD.left + plotW} y1={yScale(d.med)} y2={yScale(d.med)}
                stroke={s.color} strokeWidth={1.5} strokeDasharray="6 4" opacity={0.7} />
              <text x={PAD.left + plotW + 4} y={yScale(d.med) + 3} fontSize={9} fill={s.color} fontWeight="600">
                {fmtMs(d.med)}ms
              </text>
              {d.times.map((t, i) => (
                <circle key={i} cx={xScale(i, d.times.length)} cy={yScale(t)} r={3} fill={s.color} opacity={0.7} />
              ))}
            </g>
          );
        })}
      </svg>
    </div>
  );
}
