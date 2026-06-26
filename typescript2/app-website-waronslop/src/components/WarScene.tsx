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

// the lone lead ("little guy") walks himself out; scrolling speeds him along.
const LEAD_AUTO_VW = 6; // vw/sec he covers on his own
const LEAD_BOOST_VW = 0.5; // vw added each scroll tick
const LEAD_START_VW = -12;
const LEAD_END_VW = 132;
const LEAD_SIZE = 84;

// a transparent, tranquil card per scene — alternating sides — naming where the
// slop creeps in. Each fades in with its backdrop (opacity follows --bN).
const CARDS: { side: 'left' | 'right'; body: string }[] = [
  {
    side: 'left',
    body: 'Slop is born in the prompt. Freeform strings and “just ask nicely” hand the model the wheel — no contract, no guaranteed shape, a different answer every call.',
  },
  {
    side: 'right',
    body: 'Then it spreads through the plumbing. Hand-rolled JSON parsing, retries, and glue code pile up around every call until the architecture itself becomes slop.',
  },
  {
    side: 'left',
    body: 'And then it ships. Untyped outputs drift in production, slip past your tests, and surface as the 2am page — the slop you can’t see is the slop you can’t fix.',
  },
];

const STYLES = `
  .warscene2 [data-leg] {
    image-rendering: pixelated; image-rendering: -moz-crisp-edges; image-rendering: crisp-edges;
    animation-name: wf2; animation-timing-function: steps(4); animation-iteration-count: infinite;
    animation-duration: var(--ms); animation-play-state: paused;
  }
  .warscene2.is-scrolling [data-leg] { animation-play-state: running; }
  /* the lead walks on his own, so his legs always run */
  .warscene2 [data-lead] {
    image-rendering: pixelated; image-rendering: -moz-crisp-edges; image-rendering: crisp-edges;
    animation: wf2 var(--ms) steps(4) infinite;
  }
  @keyframes wf2 { from { background-position: 0 0; } to { background-position: calc(var(--sw) * -1) 0; } }
  @media (prefers-reduced-motion: reduce) { .warscene2 [data-leg], .warscene2 [data-lead] { animation: none !important; } }

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
  const leadX = useRef(LEAD_START_VW);
  const leadBoost = useRef(0);
  const [scene, setScene] = useState(0);
  const [scale, setScale] = useState(1);
  const [started, setStarted] = useState(false);
  const [leadDone, setLeadDone] = useState(false);

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
      leadBoost.current += LEAD_BOOST_VW; // scrolling nudges the lead along
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

  // the lead walks himself out (auto), and each scroll tick speeds him along.
  useEffect(() => {
    const root = sceneRef.current;
    if (!root) return;
    if (window.matchMedia('(prefers-reduced-motion: reduce)').matches) {
      setLeadDone(true);
      return;
    }
    let raf = 0;
    let last = 0;
    const tick = (now: number) => {
      if (!last) last = now;
      const dt = Math.min(0.05, (now - last) / 1000);
      last = now;
      leadX.current += LEAD_AUTO_VW * dt + leadBoost.current;
      leadBoost.current = 0;
      root.style.setProperty('--lead', leadX.current.toFixed(2));
      if (leadX.current >= LEAD_END_VW) {
        setLeadDone(true);
        return;
      }
      raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  }, []);

  const strip = Math.round(20 * scale);
  const ground = Math.round(6 * scale);

  return (
    <section ref={wrapRef} className="relative" style={{ height: `${SCROLL_VH}vh` }}>
      <div
        ref={sceneRef}
        className="warscene2 sticky top-0 h-screen w-full overflow-hidden"
        style={{ ['--p' as string]: '0', ['--b0' as string]: '1', ['--b1' as string]: '0', ['--b2' as string]: '0', ['--strip' as string]: `${strip}px`, ['--lead' as string]: String(LEAD_START_VW) }}
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

        {/* the lead "little guy" — walks himself out (--lead), scroll speeds him */}
        {!leadDone && (
          <div
            data-lead
            style={{
              ...sprite('walk', Math.round(LEAD_SIZE * scale)),
              left: 0,
              bottom: ground,
              zIndex: 40,
              transform: 'translateX(calc(var(--lead) * 1vw))',
            }}
          />
        )}

        {/* a tranquil, see-through card per scene — alternating sides — that fades
            in with its backdrop and names where the slop creeps in */}
        {CARDS.map((c, i) => (
          <div
            key={i}
            className={`pointer-events-none absolute bottom-24 left-4 right-4 z-20 sm:bottom-auto sm:top-1/2 sm:max-w-sm sm:-translate-y-1/2 ${
              c.side === 'left' ? 'sm:left-10 sm:right-auto' : 'sm:left-auto sm:right-10'
            }`}
            style={{ opacity: `var(--b${i})` as unknown as number }}
          >
            <div className="rounded-2xl border border-ink/10 bg-[#fbf7ed]/75 p-5 shadow-sm backdrop-blur-md sm:p-6">
              <p
                className="text-[15px] leading-relaxed text-ink sm:text-base"
                style={{ fontFamily: "'Times New Roman', Times, serif" }}
              >
                {c.body}
              </p>
            </div>
          </div>
        ))}

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
