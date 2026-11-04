'use client';

import { useEffect, useRef, useState } from 'react';
import LoopLine from './loop-line';
import { easeInOut } from './use-canvas';

// Two agents doing the same work. One can build, test, and ship, then can't see production and waits for
// a person to bring back what happened. The other goes around on its own. The lap counts show the gap.
const LABELS = ['Build', 'Test', 'Deploy', 'Monitor'];
const STATIONS = [0.125, 0.375, 0.625, 0.875];
const TOP = 52;
const INSET = 16;
const CORNER = 24;
const BAML_LAP = 2600;
const FORWARD = 1000;
const WAIT = 4200;
const BACK = 3400;
const LAP = FORWARD + WAIT + BACK;

function Stations({ active, dim }: { active: number; dim?: number }) {
  return LABELS.map((label, i) => (
    <p
      className={`absolute top-0 -translate-x-1/2 font-display font-medium text-[15px] md:text-[22px] tracking-[-0.03em] transition-colors duration-300 ${i === dim ? 'text-white/20' : active === i ? 'text-white' : 'text-white/45'}`}
      key={label}
      style={{ left: `${STATIONS[i] * 100}%` }}
    >
      {label}
    </p>
  ));
}

function Header({ laps, title }: { laps: number; title: string }) {
  return (
    <div className="flex items-end justify-between gap-6 mb-4">
      <p className="text-[15px] md:text-lg text-white/70">{title}</p>
      <p className="flex items-baseline gap-2.5">
        <span className="text-[13px] md:text-[14px] text-white/45">Laps</span>
        <span
          className="v3-enter font-display font-medium text-[32px] md:text-[44px] leading-none tracking-[-0.04em] tabular-nums"
          key={laps}
        >
          {laps}
        </span>
      </p>
    </div>
  );
}

const CAPTIONS = {
  back: 'You paste what you found back to your agent',
  forward: '',
  wait: 'Waiting for you to read the logs',
};

function StuckLane() {
  const box = useRef<SVGSVGElement>(null);
  const back = useRef<SVGPathElement>(null);
  const agent = useRef<SVGCircleElement>(null);
  const [size, setSize] = useState({ height: 0, width: 0 });
  const [phase, setPhase] = useState<keyof typeof CAPTIONS>('forward');
  const [laps, setLaps] = useState(0);

  useEffect(() => {
    const el = box.current;
    if (!el) return;
    const observer = new ResizeObserver(() => {
      const { height, width } = el.getBoundingClientRect();
      setSize({ height, width });
    });
    observer.observe(el);
    return () => observer.disconnect();
  }, []);

  const [x0, x1, x2, x3] = STATIONS.map((s) => s * size.width);
  const bottom = size.height - INSET;
  const wall = x2 + (x3 - x2) * 0.45;

  useEffect(() => {
    const dot = agent.current;
    const path = back.current;
    if (!dot || !path || !size.width) return;
    const place = (x: number, y: number) => {
      dot.setAttribute('cx', String(x));
      dot.setAttribute('cy', String(y));
    };
    if (window.matchMedia('(prefers-reduced-motion: reduce)').matches) {
      place(x2, TOP);
      setPhase('wait');
      return;
    }
    const total = path.getTotalLength();
    let clock = 0;
    let last = performance.now();
    let shown = '';
    let lap = 0;
    let frame = 0;
    const tick = (now: number) => {
      clock += Math.min(now - last, 50);
      last = now;
      const t = clock % LAP;
      const n = Math.floor(clock / LAP);
      if (n !== lap) {
        lap = n;
        setLaps(n);
      }
      const next =
        t < FORWARD ? 'forward' : t < FORWARD + WAIT ? 'wait' : 'back';
      if (next !== shown) {
        shown = next;
        setPhase(next);
      }
      if (t < FORWARD) place(x0 + (x2 - x0) * easeInOut(t / FORWARD), TOP);
      else if (t < FORWARD + WAIT) place(x2, TOP);
      else {
        const p = path.getPointAtLength(((t - FORWARD - WAIT) / BACK) * total);
        place(p.x, p.y);
      }
      frame = requestAnimationFrame(tick);
    };
    frame = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(frame);
  }, [size.width, x0, x2]);

  const active = phase === 'forward' ? -1 : phase === 'wait' ? 2 : 0;

  return (
    <div>
      <Header laps={laps} title="Your agent, in any other language" />
      <div className="relative h-[150px] md:h-[170px]">
        <Stations active={active} dim={3} />
        <svg
          aria-hidden="true"
          className="absolute inset-0 size-full overflow-visible"
          ref={box}
        >
          {size.width > 0 && (
            <>
              <path
                d={`M${x0} ${TOP}H${x2}`}
                stroke="rgb(255 255 255 / 0.17)"
                strokeWidth={1.5}
              />
              <path
                d={`M${x2} ${TOP}V${bottom - CORNER}Q${x2} ${bottom} ${x2 - CORNER} ${bottom}H${x0 + CORNER}Q${x0} ${bottom} ${x0} ${bottom - CORNER}V${TOP}`}
                fill="none"
                ref={back}
                stroke="rgb(255 255 255 / 0.2)"
                strokeDasharray="3 6"
                strokeWidth={1.5}
              />
              <path
                d={`M${wall} ${TOP - 20}V${TOP + 20}`}
                stroke="rgb(255 255 255 / 0.45)"
                strokeWidth={2}
              />
              {[x0, x1, x2].map((x) => (
                <circle className="v3-node" cx={x} cy={TOP} key={x} r={6} />
              ))}
              <circle
                cx={x3}
                cy={TOP}
                fill="none"
                r={6}
                stroke="rgb(255 255 255 / 0.15)"
                strokeDasharray="2 3"
                strokeWidth={1.5}
              />
              {phase === 'wait' && (
                <circle
                  className="v3-pulse"
                  cx={x2}
                  cy={TOP}
                  fill="none"
                  r={8}
                  stroke="var(--color-clover-core)"
                  style={{
                    transformBox: 'fill-box',
                    transformOrigin: 'center',
                  }}
                />
              )}
              <circle
                fill="var(--color-clover-100)"
                r={5}
                ref={agent}
                style={{
                  filter: 'drop-shadow(0 0 8px var(--color-clover-core))',
                }}
              />
            </>
          )}
        </svg>
        <p
          className="absolute -translate-x-1/2 text-[12px] md:text-[13px] text-white/40 whitespace-nowrap"
          style={{
            left: `${((STATIONS[2] + STATIONS[3]) / 2) * 100}%`,
            top: TOP + 28,
          }}
        >
          Can't see production
        </p>
        {CAPTIONS[phase] && (
          <p
            className="v3-enter absolute left-1/2 -translate-x-1/2 translate-y-1/2 px-3 md:px-4 bg-black text-[12.5px] md:text-[15px] text-white/75 whitespace-nowrap"
            key={phase}
            style={{ bottom: INSET }}
          >
            {CAPTIONS[phase]}
          </p>
        )}
      </div>
    </div>
  );
}

function BamlLane() {
  const [active, setActive] = useState(-1);
  const [laps, setLaps] = useState(0);
  const prev = useRef(-1);

  const onStage = (stage: number) => {
    if (stage === 0 && prev.current === LABELS.length) setLaps((n) => n + 1);
    prev.current = stage;
    setActive(stage);
  };

  return (
    <div>
      <Header laps={laps} title="Your agent, in BAML" />
      <div className="relative h-[150px] md:h-[170px]">
        <Stations active={active} />
        <LoopLine
          active={active}
          inset={INSET}
          lapMs={BAML_LAP}
          onStage={onStage}
          stations={STATIONS}
          top={TOP}
        />
        <p
          className={`absolute left-1/2 -translate-x-1/2 translate-y-1/2 px-3 md:px-4 bg-black text-[12.5px] md:text-[15px] whitespace-nowrap transition-colors duration-300 ${active === LABELS.length ? 'text-clover' : 'text-white/60'}`}
          style={{ bottom: INSET }}
        >
          Reads production itself and goes again
        </p>
      </div>
    </div>
  );
}

export default function HeroLanes() {
  return (
    <figure
      aria-label="Two agents. In any other language, the agent builds, tests, and deploys, then can't see production and waits for a person. In BAML, the agent goes around the whole loop on its own, and finishes several laps in the same time."
      className="relative pb-16"
    >
      <div
        aria-hidden
        className="v3-glow left-[-10%] right-[-10%] top-[30%] h-[640px]"
      />
      <div className="relative flex flex-col gap-14 md:gap-20">
        <StuckLane />
        <BamlLane />
      </div>
    </figure>
  );
}
