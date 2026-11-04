'use client';

import {
  type CSSProperties,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
} from 'react';

// The toolchain as an exploded hardware assembly: six plates, from the language down to observability,
// float apart with data running between them while the camera orbits. Then they descend and lock
// together, and one pulse runs down the stack and back up. Built from CSS 3D transforms, in two looks: a
// keynote product shot, and a blueprint.
const LAYERS = [
  { name: 'Language', note: 'Types, functions, prompts' },
  { name: 'Compiler', note: 'baml check' },
  { name: 'Tests', note: 'baml test' },
  { name: 'Runtime', note: 'baml pack' },
  { name: 'Tracing', note: 'Every call recorded' },
  { name: 'Observability', note: 'baml query, dashboards, alerts' },
];
const GLASS = [0, 4];
const N = LAYERS.length;

// Plate size and the gaps between plates, in pixels at full scale.
const W = 440;
const D = 300;
const T = 16;
const OPEN = 62;
const SHUT = 2;
const TILT = 60;
const PERSPECTIVE = 2400;
const DEG_PER_MS = 0.006;

// One lap, in milliseconds.
const DESCEND = 4800;
const LOCKED = 6200;
const PULSE = 6500;
const STEP = 230;
const RISE = 12200;
const OPENED = 13600;
const LAP = 14800;

// Where data runs between each pair of plates, as fractions of a plate's width and depth.
const LINKS = [
  [0.2, 0.3],
  [0.58, 0.62],
  [0.82, 0.24],
];

const easeInOut = (t: number) =>
  t < 0.5 ? 4 * t * t * t : 1 - (-2 * t + 2) ** 3 / 2;
const clamp = (n: number) => Math.min(1, Math.max(0, n));

function gapAt(t: number) {
  if (t < DESCEND) return OPEN;
  if (t < LOCKED)
    return OPEN + (SHUT - OPEN) * easeInOut((t - DESCEND) / (LOCKED - DESCEND));
  if (t < RISE) return SHUT;
  if (t < OPENED)
    return SHUT + (OPEN - SHUT) * easeInOut((t - RISE) / (OPENED - RISE));
  return OPEN;
}

/** How far above the stack's center a plate sits, in plate thicknesses plus gaps. */
const level = (i: number) => (N - 1) / 2 - i;

/** A height in the stack, projected to a screen offset from the stack's center. */
function project(z: number) {
  const a = (TILT * Math.PI) / 180;
  return (-z * Math.sin(a) * PERSPECTIVE) / (PERSPECTIVE - z * Math.cos(a));
}

function Part({
  className = '',
  style,
}: {
  className?: string;
  style?: CSSProperties;
}) {
  return <span className={`v3-part absolute ${className}`} style={style} />;
}

// What's machined into each plate's top face.
function Details({ index }: { index: number }) {
  switch (index) {
    case 0:
      return (
        <>
          {[62, 44, 54, 30, 48].map((w, i) => (
            <Part
              className="h-2.5 rounded-full"
              // biome-ignore lint/suspicious/noArrayIndexKey: fixed decoration
              key={i}
              style={{ left: 40, top: 54 + i * 26, width: `${w}%` }}
            />
          ))}
          <Part
            className="rounded-lg"
            style={{ height: 70, right: 36, top: 54, width: 70 }}
          />
        </>
      );
    case 1:
      return (
        <>
          <Part
            className="rounded-xl"
            style={{ height: 132, left: 154, top: 70, width: 132 }}
          />
          <Part
            className="rounded-md"
            style={{ height: 72, left: 184, top: 100, width: 72 }}
          />
          {[0, 1, 2, 3].map((side) => (
            <span
              className={`v3-pins absolute ${side % 2 ? 'v3-pins-v' : ''}`}
              key={side}
              style={
                side % 2
                  ? {
                      height: 112,
                      left: side === 1 ? 134 : 294,
                      top: 80,
                      width: 12,
                    }
                  : {
                      height: 12,
                      left: 164,
                      top: side === 0 ? 50 : 210,
                      width: 112,
                    }
              }
            />
          ))}
        </>
      );
    case 2:
      return (
        <>
          {Array.from({ length: 12 }, (_, i) => (
            <Part
              className={`rounded-md ${i % 5 === 3 ? '' : 'v3-part-on'}`}
              // biome-ignore lint/suspicious/noArrayIndexKey: fixed decoration
              key={i}
              style={{
                height: 34,
                left: 52 + (i % 6) * 58,
                top: 72 + Math.floor(i / 6) * 58,
                width: 34,
              }}
            />
          ))}
        </>
      );
    case 3:
      return (
        <>
          <Part
            className="rounded-2xl"
            style={{ height: 150, left: 46, top: 56, width: 230 }}
          />
          <Part
            className="rounded-full"
            style={{ height: 64, left: 312, top: 64, width: 64 }}
          />
          <Part
            className="rounded-full"
            style={{ height: 40, left: 324, top: 150, width: 40 }}
          />
        </>
      );
    case 4:
      return (
        <>
          {[110, 76, 42].map((size) => (
            <Part
              className="rounded-full"
              key={size}
              style={{
                height: size,
                left: 100 - size / 2,
                top: 130 - size / 2,
                width: size,
              }}
            />
          ))}
          <svg
            aria-hidden="true"
            className="v3-wave absolute"
            height="80"
            style={{ left: 190, top: 90 }}
            viewBox="0 0 200 80"
            width="200"
          >
            <path
              d="M0 40h30l10-26 14 52 12-40 10 14h34l8-20 12 34 8-14h62"
              fill="none"
              strokeWidth="1.5"
            />
          </svg>
        </>
      );
    default:
      return (
        <>
          <Part
            className="rounded-lg"
            style={{ height: 140, left: 44, top: 52, width: 250 }}
          />
          {[40, 64, 52, 88, 70, 100, 82].map((h, i) => (
            <Part
              className="v3-part-on rounded-[3px]"
              // biome-ignore lint/suspicious/noArrayIndexKey: fixed decoration
              key={i}
              style={{ height: h, left: 64 + i * 32, top: 176 - h, width: 18 }}
            />
          ))}
          {[0, 1, 2].map((i) => (
            <Part
              className="rounded-full"
              key={i}
              style={{ height: 22, left: 330, top: 64 + i * 40, width: 22 }}
            />
          ))}
        </>
      );
  }
}

function Plate({ index, lit }: { index: number; lit: boolean }) {
  return (
    <div
      className={`absolute ${GLASS.includes(index) ? 'v3-glass' : ''} ${lit ? 'v3-plate-lit' : ''}`}
      style={{
        height: D,
        left: -W / 2,
        top: -D / 2,
        transform: `translateZ(calc(${level(index)} * (${T}px + var(--gap) * 1px)))`,
        transformStyle: 'preserve-3d',
        width: W,
      }}
    >
      {/* Four edges hang below the top face and give the plate its thickness. */}
      <span
        className="v3-plate-side absolute left-0 top-0 w-full origin-top"
        style={{ height: T, transform: 'rotateX(-90deg)' }}
      />
      <span
        className="v3-plate-side absolute left-0 top-full w-full origin-top"
        style={{ height: T, transform: 'rotateX(-90deg)' }}
      />
      <span
        className="v3-plate-side absolute left-0 top-0 h-full origin-left"
        style={{ transform: 'rotateY(90deg)', width: T }}
      />
      <span
        className="v3-plate-side absolute left-full top-0 h-full origin-left"
        style={{ transform: 'rotateY(90deg)', width: T }}
      />
      <div className="v3-plate-top absolute inset-0 rounded-[4px]">
        <Details index={index} />
        <span className="v3-engrave absolute left-6 bottom-4 font-mono text-[13px]">
          {LAYERS[index].name}
        </span>
      </div>
    </div>
  );
}

// Data running up and down between two plates: a pad on the lower plate, and a line standing on it.
function Link({ below, flow, i }: { below: number; flow: boolean; i: number }) {
  const [x, y] = LINKS[(i + below) % LINKS.length];
  const line =
    'v3-link absolute left-0 top-0 w-[2px] origin-top overflow-hidden transition-opacity duration-500';
  const height = 'calc(var(--gap) * 1px)';
  const packet = (
    <span
      className="v3-packet absolute inset-x-0 h-[34%]"
      style={
        {
          '--i': i + below * 3,
          animationDirection: (i + below) % 2 ? 'reverse' : 'normal',
        } as CSSProperties
      }
    />
  );
  return (
    <div
      className="absolute"
      style={{
        left: -W / 2 + x * W,
        top: -D / 2 + y * D,
        transform: `translateZ(calc(${level(below)} * (${T}px + var(--gap) * 1px) + 1px))`,
        transformStyle: 'preserve-3d',
      }}
    >
      <span className="v3-pad absolute -left-[5px] -top-[5px] size-3 rounded-full" />
      <span
        className={line}
        style={{ height, opacity: flow ? 1 : 0, transform: 'rotateX(90deg)' }}
      >
        {packet}
      </span>
      <span
        className={line}
        style={{
          height,
          opacity: flow ? 1 : 0,
          transform: 'rotateZ(90deg) rotateX(90deg)',
        }}
      >
        {packet}
      </span>
    </div>
  );
}

function Stack({ blueprint }: { blueprint?: boolean }) {
  const figure = useRef<HTMLElement>(null);
  const assembly = useRef<HTMLDivElement>(null);
  const callouts = useRef<(HTMLLIElement | null)[]>([]);
  const dimension = useRef<HTMLDivElement>(null);
  const [frame, setFrame] = useState({ cx: 0, cy: 0, s: 1, wide: true });
  const [phase, setPhase] = useState<'open' | 'locked'>('open');
  const [lit, setLit] = useState(-1);
  const box = useRef(frame);
  box.current = frame;

  useLayoutEffect(() => {
    const el = figure.current;
    if (!el) return;
    const measure = () => {
      const { width, height } = el.getBoundingClientRect();
      const wide = width >= 768;
      setFrame({
        cx: width * (wide ? 0.6 : 0.5),
        cy: height * 0.46,
        s: Math.min(1, width / (wide ? 1100 : 640)),
        wide,
      });
    };
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(el);
    return () => observer.disconnect();
  }, []);

  useEffect(() => {
    const still = window.matchMedia('(prefers-reduced-motion: reduce)').matches;
    const begin = performance.now();
    let id = 0;
    let lastPhase = '';
    let lastLit = -2;

    const paint = (now: number) => {
      const t = still ? 0 : (now - begin) % LAP;
      const gap = gapAt(t);
      const orbit = -38 + (still ? 0 : (now - begin) * DEG_PER_MS);
      assembly.current?.style.setProperty('--gap', String(gap));
      assembly.current?.style.setProperty('--orbit', `${orbit}deg`);

      // Callouts and the dimension line follow the plates up and down. The plates' centers sit on the
      // orbit's axis, so their height on screen doesn't change as the camera goes around.
      const { s } = box.current;
      callouts.current.forEach((el, i) => {
        if (el)
          el.style.translate = `0 ${project(level(i) * (T + gap) - T / 2) * s}px`;
      });
      if (dimension.current) {
        const top = project(level(0) * (T + gap)) * s;
        const bottom = project(level(N - 1) * (T + gap) - T) * s;
        dimension.current.style.translate = `0 ${top}px`;
        dimension.current.style.height = `${bottom - top}px`;
      }

      const p = t >= LOCKED && t < RISE ? 'locked' : 'open';
      if (p !== lastPhase) {
        lastPhase = p;
        setPhase(p);
      }
      // The pulse runs from the language down to observability, and back up.
      const step = Math.floor((t - PULSE) / STEP);
      const next =
        t >= PULSE && step < 2 * N ? (step < N ? step : 2 * N - 1 - step) : -1;
      if (next !== lastLit) {
        lastLit = next;
        setLit(next);
      }
      if (!still) id = requestAnimationFrame(paint);
    };
    id = requestAnimationFrame(paint);
    return () => cancelAnimationFrame(id);
  }, []);

  const { cx, cy, s, wide } = frame;
  const locked = phase === 'locked';
  const dot = cx - 130 * s;

  return (
    <figure
      aria-label="The BAML toolchain as an exploded assembly of six plates: language, compiler, tests, runtime, tracing, and observability. Data runs between them while the camera orbits. The plates lock together, and a pulse runs from the language to observability and back."
      className={`relative overflow-hidden ${blueprint ? 'v3-blueprint v3-blueprint-sheet rounded-2xl' : 'v3-keynote'}`}
      ref={figure}
    >
      <div className="relative h-[440px] sm:h-[560px] md:h-[680px]">
        {blueprint ? (
          <div
            aria-hidden
            className="absolute inset-3 md:inset-4 rounded-[10px] border border-[rgb(225_238_255/0.35)] pointer-events-none"
          />
        ) : (
          <>
            <div
              aria-hidden
              className="absolute left-1/2 top-0 h-full w-[90%] -translate-x-1/2 bg-[radial-gradient(ellipse_50%_55%_at_50%_0%,rgb(255_255_255/0.07),transparent)]"
            />
            <div
              aria-hidden
              className={`absolute h-24 w-[46%] -translate-x-1/2 rounded-[50%] blur-2xl transition-opacity duration-700 ${locked ? 'opacity-100' : 'opacity-50'}`}
              style={{
                background: 'rgb(from var(--color-clover-core) r g b / 0.28)',
                left: cx,
                top: cy + 230 * s,
              }}
            />
          </>
        )}

        {/* The stack's center, and the camera looking at it. */}
        <div
          className="absolute size-0"
          style={{
            left: cx,
            perspective: PERSPECTIVE,
            top: cy,
            transform: `scale(${s})`,
          }}
        >
          <div
            className="size-0"
            ref={assembly}
            style={{
              transform: `rotateX(${TILT}deg) rotateZ(var(--orbit))`,
              transformStyle: 'preserve-3d',
            }}
          >
            {LAYERS.map((layer, i) => (
              <Plate index={i} key={layer.name} lit={lit === i} />
            ))}
            {LAYERS.slice(1).map((layer, b) =>
              LINKS.map((_, i) => (
                <Link
                  below={b + 1}
                  flow={!locked}
                  i={i}
                  key={`${layer.name}-${i}`}
                />
              )),
            )}
          </div>
        </div>

        {/* Callouts, each level with its plate. */}
        {wide && (
          <ol className="absolute inset-0 pointer-events-none">
            {LAYERS.map((layer, i) => (
              <li
                className="absolute flex items-center transition-opacity duration-300"
                key={layer.name}
                ref={(el) => {
                  callouts.current[i] = el;
                }}
                // Assembled, the plates are too close for six callouts, so only the lit plate keeps one.
                style={{
                  left: 0,
                  opacity: locked && lit !== i ? 0 : 1,
                  top: cy,
                  width: dot,
                }}
              >
                <span className="flex-1 min-w-0 -translate-y-1/2 flex items-center justify-end gap-3">
                  {blueprint && (
                    <span
                      className={`grid place-items-center size-6 shrink-0 rounded-full border font-mono text-[11px] transition-colors duration-200 ${lit === i ? 'border-white bg-white text-[#0c2d57]' : 'border-[rgb(225_238_255/0.7)] text-[rgb(225_238_255)]'}`}
                    >
                      {i + 1}
                    </span>
                  )}
                  <span className="text-right">
                    <span
                      className={`block text-[15px] lg:text-[17px] transition-colors duration-200 ${blueprint ? 'font-mono' : 'font-display font-medium tracking-[-0.02em]'} ${lit === i ? (blueprint ? 'text-white' : 'text-clover-100') : 'text-white/85'}`}
                    >
                      {layer.name}
                    </span>
                    <span
                      className={`block font-mono text-[12px] lg:text-[13px] ${blueprint ? 'text-[rgb(225_238_255/0.65)]' : 'text-white/45'}`}
                    >
                      {layer.note}
                    </span>
                  </span>
                  <span
                    className={`h-px shrink-0 ${blueprint ? 'bg-[rgb(225_238_255/0.7)]' : 'bg-white/30'}`}
                    style={{ width: 170 * s }}
                  />
                  <span
                    className={`size-1.5 shrink-0 rounded-full ${blueprint ? 'bg-white' : 'bg-white/70'}`}
                  />
                </span>
              </li>
            ))}
          </ol>
        )}

        {/* The blueprint's dimension line, from the top plate to the bottom one. */}
        {blueprint && (
          <div
            aria-hidden
            className="absolute w-3 border-y border-[rgb(225_238_255/0.7)]"
            ref={dimension}
            style={{ left: cx + 270 * s, top: cy }}
          >
            <span className="absolute left-1/2 top-0 bottom-0 w-px -translate-x-1/2 bg-[rgb(225_238_255/0.7)]" />
            <span className="absolute left-5 top-1/2 -translate-y-1/2 font-mono text-[11px] md:text-[12px] whitespace-nowrap text-[rgb(225_238_255/0.85)]">
              {locked ? 'Assembled' : 'Exploded view'}
            </span>
          </div>
        )}

        {blueprint && (
          <dl className="absolute right-5 md:right-8 bottom-5 md:bottom-8 hidden sm:grid grid-cols-[auto_auto] border border-[rgb(225_238_255/0.55)] font-mono text-[11px] text-[rgb(225_238_255/0.85)] [&>*]:px-2.5 [&>*]:py-1 [&>dt]:border-r [&>dt]:border-[rgb(225_238_255/0.35)] [&>dt]:text-[rgb(225_238_255/0.55)] [&>*:nth-child(n+3)]:border-t [&>*:nth-child(n+3)]:border-t-[rgb(225_238_255/0.35)]">
            <dt>Drawing</dt>
            <dd>BAML toolchain</dd>
            <dt>View</dt>
            <dd>{locked ? 'Assembled' : 'Exploded'}</dd>
            <dt>Parts</dt>
            <dd>6</dd>
          </dl>
        )}

        <p
          aria-live="polite"
          className={`absolute inset-x-0 bottom-6 md:bottom-10 px-5 text-center text-[15px] md:text-xl text-balance transition-[opacity,translate] duration-700 ${locked ? '' : 'opacity-0 translate-y-2'} ${blueprint ? 'font-mono md:text-lg text-white' : 'text-white/85'}`}
        >
          From the language to observability, BAML is one toolchain.
        </p>
      </div>
    </figure>
  );
}

export function HeroStackKeynote() {
  return <Stack />;
}

export function HeroStackBlueprint() {
  return <Stack blueprint />;
}
