'use client';

import { CSSProperties, useEffect, useRef, useState } from 'react';

// ─────────────────────────────────────────────────────────────────────────────
// Scroll-driven war hero. The scene is PINNED while you scroll through a tall
// section; scroll progress (0→1) scrubs the marching column across the screen
// and cross-fades the three battle backdrops (Design → Architecture → Deploy).
// The same characters persist the whole way — they don't reset per scene.
//
// Pure CSS + a single CSS variable (--p) set on scroll: the horizontal march is
// `translateX(calc(var(--p) * SPAN ...))`, the leg cycle is a steps() keyframe
// that only RUNS while actively scrolling (so nobody "moonwalks" when idle).
// No scroll-timeline (Safari/Firefox still don't support it) — works everywhere.
// ─────────────────────────────────────────────────────────────────────────────

const F = {
  walk: { src: '/spartan_walk.png', fw: 48, fh: 68, n: 4, ms: 460 },
  tank: { src: '/tank_roll.png', fw: 96, fh: 56, n: 4, ms: 440 },
} as const;
type Kind = keyof typeof F;

const BACKDROPS = ['/scene_design.png', '/scene_arch.png', '/scene_deploy.png'];
const CAPTIONS = [
  'I · The Battle of Design',
  'II · The Battle of Architecture',
  'III · The Battle of Deployment',
];

const SPAN = 215; // vw the column travels across the full scroll
const SCROLL_VH = 300; // height of the scroll section, in vh (pins for SCROLL_VH−100)

const STYLES = `
  .warscene2 [data-leg] {
    image-rendering: pixelated; image-rendering: -moz-crisp-edges; image-rendering: crisp-edges;
    animation-name: wf2; animation-timing-function: steps(4); animation-iteration-count: infinite;
    animation-duration: var(--ms); animation-play-state: paused;
  }
  .warscene2.is-scrolling [data-leg] { animation-play-state: running; }
  @keyframes wf2 { from { background-position: 0 0; } to { background-position: calc(var(--sw) * -1) 0; } }
  @media (prefers-reduced-motion: reduce) { .warscene2 [data-leg] { animation: none !important; } }

  /* backdrop: full panorama anchored to the bottom on wide screens (full clouds);
     COVER to fill the tall hero on phones so there is no empty sky gap. */
  .warscene2 .bd {
    position: absolute; left: 0; right: 0; bottom: var(--strip);
    width: 100%; height: auto; max-width: none; display: block;
  }
  @media (max-width: 767px) {
    .warscene2 .bd { top: 0; bottom: 0; height: 100%; object-fit: cover; object-position: center bottom; }
  }
`;

function sprite(kind: Kind, height: number): CSSProperties {
  const f = F[kind];
  const w = Math.round((height * f.fw) / f.fh);
  return {
    position: 'absolute',
    width: w,
    height,
    backgroundImage: `url(${f.src})`,
    backgroundRepeat: 'no-repeat',
    backgroundSize: `${w * f.n}px ${height}px`,
    ['--sw' as string]: `${w * f.n}px`,
    ['--ms' as string]: `${f.ms}ms`,
  };
}

// the column: each marcher's starting offset (vw, negative = behind the left
// edge); they all advance by SPAN as scroll goes 0→1, so they stay a column.
const COLUMN: { kind: Kind; size: number; off: number }[] = [
  { kind: 'walk', size: 92, off: -8 },
  { kind: 'walk', size: 96, off: -20 },
  { kind: 'walk', size: 90, off: -31 },
  { kind: 'tank', size: 122, off: -47 },
  { kind: 'walk', size: 94, off: -63 },
  { kind: 'walk', size: 97, off: -74 },
  { kind: 'walk', size: 91, off: -85 },
  { kind: 'walk', size: 95, off: -97 },
];

const clamp = (n: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, n));

export default function WarScene() {
  const wrapRef = useRef<HTMLDivElement>(null);
  const sceneRef = useRef<HTMLDivElement>(null);
  const idleTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const lastIdx = useRef(0);
  const [scene, setScene] = useState(0);
  const [scale, setScale] = useState(1);
  const [started, setStarted] = useState(false);

  useEffect(() => {
    const onResize = () => {
      setScale(clamp(Math.min(window.innerWidth / 1280, window.innerHeight / 760), 0.62, 1.3));
    };
    onResize();
    window.addEventListener('resize', onResize);
    return () => window.removeEventListener('resize', onResize);
  }, []);

  useEffect(() => {
    const wrap = wrapRef.current;
    const root = sceneRef.current;
    if (!wrap || !root) return;

    const apply = () => {
      const total = wrap.offsetHeight - window.innerHeight;
      const p = total > 0 ? clamp(-wrap.getBoundingClientRect().top / total, 0, 1) : 0;
      root.style.setProperty('--p', String(p));
      const s = p * (BACKDROPS.length - 1); // 0 → (n−1)
      for (let i = 0; i < BACKDROPS.length; i++) {
        root.style.setProperty(`--b${i}`, String(clamp(1 - Math.abs(s - i), 0, 1)));
      }
      const idx = Math.round(s);
      if (idx !== lastIdx.current) {
        lastIdx.current = idx;
        setScene(idx);
      }
      if (p > 0.004) setStarted(true);
    };

    const onScroll = () => {
      apply();
      root.classList.add('is-scrolling');
      if (idleTimer.current) clearTimeout(idleTimer.current);
      idleTimer.current = setTimeout(() => root.classList.remove('is-scrolling'), 150);
    };

    apply();
    window.addEventListener('scroll', onScroll, { passive: true });
    window.addEventListener('resize', apply);
    return () => {
      window.removeEventListener('scroll', onScroll);
      window.removeEventListener('resize', apply);
      if (idleTimer.current) clearTimeout(idleTimer.current);
    };
  }, []);

  const strip = Math.round(20 * scale);
  const ground = Math.round(6 * scale);

  return (
    <section ref={wrapRef} className="relative" style={{ height: `${SCROLL_VH}vh` }}>
      <div
        ref={sceneRef}
        className="warscene2 sticky top-0 h-screen w-full overflow-hidden"
        style={{ ['--p' as string]: '0', ['--b0' as string]: '1', ['--b1' as string]: '0', ['--b2' as string]: '0', ['--strip' as string]: `${strip}px` }}
      >
        <style>{STYLES}</style>

        {/* the three battle backdrops, stacked and cross-faded by scroll (--bN) */}
        {BACKDROPS.map((src, i) => (
          <img
            key={src}
            src={src}
            alt=""
            aria-hidden="true"
            className="bd"
            style={{ opacity: `var(--b${i})` as unknown as number }}
          />
        ))}

        {/* the ground strip the column marches on */}
        <div
          aria-hidden="true"
          style={{
            position: 'absolute',
            left: 0,
            right: 0,
            bottom: 0,
            height: strip,
            background: '#d8c39a',
            borderTop: '1px solid rgba(169, 142, 97, 0.45)',
          }}
        />

        {/* the marching column — translateX scrubbed by scroll progress (--p) */}
        {COLUMN.map((m, i) => {
          const size = Math.round(m.size * scale);
          return (
            <div
              key={i}
              data-leg
              style={{
                ...sprite(m.kind, size),
                left: 0,
                bottom: ground,
                zIndex: COLUMN.length - i,
                transform: `translateX(calc((var(--p) * ${SPAN} + ${m.off}) * 1vw))`,
              }}
            />
          );
        })}

        {/* headline + byline + battle caption overlay (stacked, top-left) */}
        <div className="absolute left-0 top-0 z-30 p-6 sm:p-8">
          <h1 className="text-3xl font-bold leading-none tracking-tight text-ink sm:text-5xl">
            fight slop with slop
          </h1>
          <p className="tweet-font mt-2 text-sm text-accent">
            by{' '}
            <a href="https://x.com/boundaryml" target="_blank" rel="noopener noreferrer" className="hover:underline">
              @boundaryml
            </a>
          </p>
          <span
            key={scene}
            className="mt-4 inline-block rounded-full border border-[#a98e61]/45 bg-[#d8c39a]/90 px-3 py-0.5 text-xs font-bold tracking-wide text-ink shadow-sm sm:px-4 sm:py-1 sm:text-sm"
            style={{ fontFamily: "ui-monospace, 'SFMono-Regular', Menlo, monospace" }}
          >
            {CAPTIONS[scene]}
          </span>
        </div>

        {/* scroll hint, fades once you start */}
        <div
          className="pointer-events-none absolute inset-x-0 bottom-7 z-30 flex justify-center transition-opacity duration-500"
          style={{ opacity: started ? 0 : 1 }}
        >
          <span className="tweet-font animate-bounce text-xs font-bold uppercase tracking-widest text-ink-2">
            scroll to march ↓
          </span>
        </div>
      </div>
    </section>
  );
}
