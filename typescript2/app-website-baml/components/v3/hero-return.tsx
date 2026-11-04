'use client';

import { useRef, useState } from 'react';
import { Code } from './code';
import { classifySource, FIXES, RED } from './story';
import { clamp, easeInOut, easeOut, type Frame, useCanvas } from './use-canvas';

// Production calls stream out of a function. Some fail, fly back into it, and come out as a new line in
// the prompt and a new test. Then the failures stop, and the next lap finds a different one.
const LAP_MS = 10_000;
const FAILING_MS = 5200;
const FIX_MS = 7000;
const RATE = 34;
const FAIL_RATE = 0.15;
const HOLD_MS = 360;
const RETURN_MS = 1800;

type Dot = {
  at: number;
  fails: boolean;
  from: { x: number; y: number };
  lane: number;
  speed: number;
  state: 'out' | 'failed' | 'back';
  turn: number;
  x: number;
  y: number;
  y0: number;
};

export default function HeroReturn() {
  const stage = useRef<HTMLDivElement>(null);
  const card = useRef<HTMLDivElement>(null);
  const [lap, setLap] = useState(0);
  const [caught, setCaught] = useState(0);
  const [fixed, setFixed] = useState(false);
  const sim = useRef({
    clock: 0,
    dots: [] as Dot[],
    fixed: false,
    lap: 0,
    spawn: 0,
  });

  const draw = ({ ctx, dt, height, width }: Frame) => {
    const s = sim.current;
    const sr = stage.current?.getBoundingClientRect();
    const cr = card.current?.getBoundingClientRect();
    if (!sr || !cr || !width) return;
    s.clock += dt;
    const c = {
      bottom: cr.bottom - sr.top,
      left: cr.left - sr.left,
      right: cr.right - sr.left,
      top: cr.top - sr.top,
    };
    // Beside the card on a wide screen, and under it on a phone.
    const side = c.right < width * 0.7;
    const field = side
      ? { bottom: height - 120, top: 40, x: c.right }
      : { bottom: height - 80, top: c.bottom + 36, x: 0 };
    const reach = width - field.x;
    const origin = side
      ? { x: c.right - 2, y: (c.top + c.bottom) / 2 }
      : { x: 0, y: (field.top + field.bottom) / 2 };
    const home = { x: (c.left + c.right) / 2, y: c.bottom - 2 };

    const t = s.clock % LAP_MS;
    const lapNow = Math.floor(s.clock / LAP_MS);
    if (lapNow !== s.lap) {
      s.lap = lapNow;
      s.fixed = false;
      setLap(lapNow);
      setCaught(0);
      setFixed(false);
    }
    if (t >= FIX_MS && !s.fixed) {
      s.fixed = true;
      setFixed(true);
    }

    const failing = t > 500 && t < FAILING_MS;
    s.spawn += (dt * RATE) / 1000;
    while (s.spawn >= 1) {
      s.spawn -= 1;
      const y0 = origin.y + (Math.random() - 0.5) * (side ? 60 : 30);
      s.dots.push({
        at: 0,
        fails: failing && Math.random() < FAIL_RATE,
        from: { x: 0, y: 0 },
        lane: field.top + Math.random() * (field.bottom - field.top),
        speed: reach / (2300 + Math.random() * 900),
        state: 'out',
        turn: field.x + reach * (0.32 + Math.random() * 0.48),
        x: origin.x,
        y: y0,
        y0,
      });
    }

    const calm: Dot[] = [];
    const hot: Dot[] = [];
    s.dots = s.dots.filter((d) => {
      if (d.state === 'out') {
        d.x += d.speed * dt;
        const p = easeOut(clamp((d.x - origin.x) / (reach * 0.3)));
        d.y = d.y0 + (d.lane - d.y0) * p;
        if (d.fails && d.x >= d.turn) {
          d.state = 'failed';
          d.at = s.clock;
          d.from = { x: d.x, y: d.y };
        }
        if (d.x > width + 8) return false;
        calm.push(d);
        return true;
      }
      if (d.state === 'failed') {
        if (s.clock - d.at > HOLD_MS) {
          d.state = 'back';
          d.at = s.clock;
        }
        hot.push(d);
        return true;
      }
      const u = easeInOut(clamp((s.clock - d.at) / RETURN_MS));
      const ctl = { x: (d.from.x + home.x) / 2, y: height - 10 };
      d.x = (1 - u) ** 2 * d.from.x + 2 * (1 - u) * u * ctl.x + u * u * home.x;
      d.y = (1 - u) ** 2 * d.from.y + 2 * (1 - u) * u * ctl.y + u * u * home.y;
      if (u >= 1) {
        setCaught((n) => n + 1);
        return false;
      }
      hot.push(d);
      return true;
    });

    ctx.fillStyle = 'rgb(255 255 255 / 0.55)';
    ctx.beginPath();
    for (const d of calm) {
      ctx.moveTo(d.x + 1.4, d.y);
      ctx.arc(d.x, d.y, 1.4, 0, Math.PI * 2);
    }
    ctx.fill();

    ctx.save();
    ctx.fillStyle = RED;
    ctx.shadowColor = RED;
    ctx.shadowBlur = 14;
    ctx.beginPath();
    for (const d of hot) {
      const r = d.state === 'failed' ? 3.2 : 2.4;
      ctx.moveTo(d.x + r, d.y);
      ctx.arc(d.x, d.y, r, 0, Math.PI * 2);
    }
    ctx.fill();
    ctx.restore();
  };

  const canvas = useCanvas(draw, 0.22);

  const round = lap % FIXES.length;
  const done = round + (fixed ? 1 : 0);
  const rules = FIXES.slice(0, done).map((f) => f.rule);
  const tests = FIXES.slice(0, done).map((f) => f.test);
  const { lines, prose } = classifySource(rules);
  const fix = FIXES[round];

  return (
    <figure
      aria-label="Production calls stream out of a BAML function. A few fail, fly back into the function, and become a new line in its prompt and a new test. Then the failures stop."
      className="relative pb-16"
    >
      <div
        aria-hidden
        className="v3-glow left-[-10%] right-[-10%] -top-24 h-[720px]"
      />
      <div className="relative h-[660px] md:h-[560px]" ref={stage}>
        <canvas className="absolute inset-0 size-full" ref={canvas} />

        <div
          className="v3-card absolute inset-x-0 top-0 md:inset-x-auto md:left-0 md:top-[42%] md:-translate-y-1/2 md:w-[400px] lg:w-[470px] overflow-hidden"
          ref={card}
        >
          {caught > 0 && !fixed && (
            <span
              aria-hidden
              className="v3-hit absolute inset-0 rounded-[inherit] pointer-events-none"
              key={caught}
            />
          )}
          <div className="flex items-center justify-between h-10 px-4 border-b border-white/10 text-[13px]">
            <span className="text-white/70">triage.baml</span>
            <span
              className="v3-enter font-mono text-white/45"
              key={`${lap}-${fixed}`}
            >
              v{41 + lap + (fixed ? 1 : 0)}
            </span>
          </div>
          <div className="py-3 overflow-hidden">
            <Code
              lines={lines}
              marks={fixed ? { [3 + rules.length]: 'added' } : {}}
              prose={prose}
            />
          </div>
          <div className="px-4 py-3 border-t border-white/10 font-mono text-[12.5px] leading-[21px] text-[#BDBDC4]">
            <p className="text-white">
              <span className="text-clover">$ </span>baml test classify
            </p>
            {tests.map((name, i) => (
              <p
                className={`flex justify-between ${fixed && i === tests.length - 1 ? 'v3-land' : ''}`}
                key={name}
              >
                <span>+ {name}</span>
                <span className="text-clover">passed</span>
              </p>
            ))}
            <p className="text-white/45">
              {3 + tests.length} tests, {3 + tests.length} passed
            </p>
          </div>
        </div>

        <p
          aria-live="polite"
          className="absolute left-0 bottom-12 md:left-auto md:right-0 md:top-0 md:bottom-auto flex items-center gap-2.5 text-[14px] md:text-base"
        >
          {fixed ? (
            <span className="v3-enter text-clover" key="fixed">
              Fixed in v{42 + lap}. 0 failing.
            </span>
          ) : (
            <span
              className="flex items-center gap-2.5 text-white/80"
              key="failing"
            >
              <span
                className="size-2 rounded-full"
                style={{ background: caught ? RED : 'rgb(255 255 255 / 0.4)' }}
              />
              {caught ? `${caught} ${fix.failure}` : 'Watching production'}
            </span>
          )}
        </p>
        <p className="absolute inset-x-0 bottom-0 text-center text-[14px] md:text-lg text-white/60">
          Failed calls come back to your agent as tests
        </p>
      </div>
    </figure>
  );
}
