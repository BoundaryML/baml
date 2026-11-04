'use client';

import { useRef, useState } from 'react';
import { RED } from './story';
import { clamp, easeInOut, type Frame, useCanvas } from './use-canvas';

// An agent's edits arrive as a noisy stream and pass through baml check and baml test. The ones that
// pass line up and ship. The ones that don't go back to the agent with the error. The errors are example
// data.
const RATE = 30;
const REJECT_RATE = 0.13;
const ROW = 15;
const BACK_MS = 1400;
const ERRORS = [
  { gate: 0, text: 'Label has no variant "refund"' },
  { gate: 1, text: 'sarcastic_ticket: expected billing, got other' },
  { gate: 0, text: 'Verdict is missing confidence' },
  { gate: 1, text: 'spanish_ticket: expected bug, got other' },
  { gate: 0, text: 'classify takes a string, got int' },
];

type Dot = {
  amp: number;
  at: number;
  bad: boolean;
  base: number;
  from: { x: number; y: number };
  phase: number;
  speed: number;
  state: 'in' | 'out' | 'back';
  x: number;
  y: number;
};

export default function HeroGate() {
  const [counts, setCounts] = useState({ back: 0, checked: 0 });
  const [error, setError] = useState<{
    gate: number;
    id: number;
    text: string;
    y: number;
  }>();
  const sim = useRef({
    back: 0,
    checked: 0,
    clock: 0,
    dots: [] as Dot[],
    errors: 0,
    lastCount: 0,
    lastError: -9999,
    spawn: 0,
  });

  const draw = ({ ctx, dt, height, width }: Frame) => {
    const s = sim.current;
    if (!width) return;
    s.clock += dt;
    const gate = width * 0.5;
    const top = 90;
    const bottom = height - 70;
    const speed = gate / 2100;

    s.spawn += (dt * RATE) / 1000;
    while (s.spawn >= 1) {
      s.spawn -= 1;
      const base = top + Math.random() * (bottom - top);
      s.dots.push({
        amp: 6 + Math.random() * 18,
        at: 0,
        bad: Math.random() < REJECT_RATE,
        base,
        from: { x: 0, y: 0 },
        phase: Math.random() * Math.PI * 2,
        speed: speed * (0.85 + Math.random() * 0.3),
        state: 'in',
        x: -6,
        y: base,
      });
    }

    const plain: Dot[] = [];
    const sorted: Dot[] = [];
    const rejected: Dot[] = [];
    s.dots = s.dots.filter((d) => {
      if (d.state === 'in') {
        d.x += d.speed * dt;
        const calm = 1 - clamp(d.x / gate) ** 2;
        d.y = d.base + Math.sin(d.phase + d.x / 38) * d.amp * calm;
        if (d.x >= gate) {
          s.checked += 1;
          if (d.bad) {
            s.back += 1;
            d.state = 'back';
            d.at = s.clock;
            d.from = { x: d.x, y: d.y };
            if (s.clock - s.lastError > 1700) {
              s.lastError = s.clock;
              const e = ERRORS[s.errors++ % ERRORS.length];
              setError({ ...e, id: s.errors, y: d.y });
            }
          } else {
            d.state = 'out';
            d.base = top + Math.round((d.y - top) / ROW) * ROW;
          }
        }
        plain.push(d);
        return true;
      }
      if (d.state === 'out') {
        d.x += d.speed * 1.15 * dt;
        d.y += (d.base - d.y) * clamp(dt / 90);
        if (d.x > width + 6) return false;
        sorted.push(d);
        return true;
      }
      const u = clamp((s.clock - d.at) / BACK_MS);
      const e = easeInOut(u);
      const end = { x: 24, y: height - 24 };
      const ctl = { x: gate * 0.6, y: height + 30 };
      d.x = (1 - e) ** 2 * d.from.x + 2 * (1 - e) * e * ctl.x + e * e * end.x;
      d.y = (1 - e) ** 2 * d.from.y + 2 * (1 - e) * e * ctl.y + e * e * end.y;
      if (u >= 1) return false;
      rejected.push(d);
      return true;
    });

    if (s.clock - s.lastCount > 250) {
      s.lastCount = s.clock;
      setCounts({ back: s.back, checked: s.checked });
    }

    ctx.fillStyle = 'rgb(255 255 255 / 0.5)';
    ctx.beginPath();
    for (const d of plain) {
      ctx.moveTo(d.x + 1.4, d.y);
      ctx.arc(d.x, d.y, 1.4, 0, Math.PI * 2);
    }
    ctx.fill();

    ctx.fillStyle = '#E7E1FF';
    ctx.beginPath();
    for (const d of sorted) ctx.rect(d.x - 2.5, d.y - 1, 5, 2);
    ctx.fill();

    ctx.save();
    ctx.fillStyle = RED;
    ctx.shadowColor = RED;
    ctx.shadowBlur = 12;
    ctx.beginPath();
    for (const d of rejected) {
      ctx.moveTo(d.x + 2.4, d.y);
      ctx.arc(d.x, d.y, 2.4, 0, Math.PI * 2);
    }
    ctx.fill();
    ctx.restore();
  };

  const canvas = useCanvas(draw, 0.3);

  return (
    <figure
      aria-label="An agent's edits arrive as a noisy stream and pass through baml check and baml test. Edits that pass line up and ship. Edits that fail go back to the agent with the error."
      className="relative pb-16"
    >
      <div
        aria-hidden
        className="v3-glow left-[10%] right-[10%] -top-16 h-[620px]"
      />
      <div className="relative h-[480px] md:h-[520px]">
        <canvas className="absolute inset-0 size-full" ref={canvas} />
        <p className="absolute left-0 top-0 text-[14px] md:text-base text-white/60">
          Your agent's edits
        </p>
        <p className="absolute right-0 top-0 text-[14px] md:text-base text-white/60">
          Ships
        </p>

        {/* The gate: a beam of light, with the two commands that hold it. */}
        <div
          aria-hidden
          className="absolute left-1/2 top-[78px] bottom-[58px] w-px -translate-x-1/2 bg-clover-100 shadow-[0_0_14px_2px_var(--color-clover-core),0_0_60px_12px_rgb(from_var(--color-clover-core)_r_g_b/0.35)]"
        />
        <div className="absolute left-1/2 top-6 -translate-x-1/2 flex whitespace-nowrap rounded-lg border border-clover/50 bg-black font-mono text-[12.5px] md:text-[14px] overflow-hidden">
          {['baml check', 'baml test'].map((command, i) => (
            <span
              className={`px-3 md:px-4 py-1.5 transition-colors duration-300 ${error && error.gate === i ? 'bg-clover/15 text-white' : 'text-white/55'} ${i ? 'border-l border-clover/30' : ''}`}
              key={command}
            >
              {command}
            </span>
          ))}
        </div>

        {error && (
          <p
            className="v3-enter absolute right-0 md:right-auto md:left-[calc(50%+18px)] max-w-[46%] md:max-w-none px-2.5 py-1 rounded-md border bg-black font-mono text-[11.5px] md:text-[12.5px] whitespace-nowrap overflow-hidden text-ellipsis"
            key={error.id}
            style={{
              borderColor: `${RED}80`,
              color: '#FF9A9E',
              top: error.y - 14,
            }}
          >
            {error.text}
          </p>
        )}

        <dl className="absolute left-0 bottom-0 flex gap-6 md:gap-10 text-[13px] md:text-[14px]">
          <div>
            <dt className="text-white/45">Edits checked</dt>
            <dd className="font-display font-medium text-[24px] md:text-[30px] tracking-[-0.03em] tabular-nums">
              {counts.checked}
            </dd>
          </div>
          <div>
            <dt className="text-white/45">Sent back with the error</dt>
            <dd
              className="font-display font-medium text-[24px] md:text-[30px] tracking-[-0.03em] tabular-nums"
              style={{ color: '#FF9A9E' }}
            >
              {counts.back}
            </dd>
          </div>
        </dl>
      </div>
    </figure>
  );
}
