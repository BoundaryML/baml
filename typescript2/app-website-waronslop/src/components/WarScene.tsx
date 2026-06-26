'use client';

import { CSSProperties, useEffect, useRef, useState } from 'react';

// ─────────────────────────────────────────────────────────────────────────────
// Three "battle" rows, each a smaller scene panel + a paragraph panel on the
// opposite side (scene left / right / left). As a row scrolls into view it
// MATERIALIZES — blur→sharp, fade, settle — and its character marches in, then
// holds (it never walks off). All of it is scrubbed by the row's scroll progress
// (--r, 0→1), set on scroll; the leg cycle only runs while you're scrolling so
// nothing moonwalks at rest. The lead settles into a wave. Cross-browser (no
// scroll-timeline): plain CSS vars + transforms.
// ─────────────────────────────────────────────────────────────────────────────

const F = {
  walk: { src: '/spartan_walk.png', fw: 48, fh: 68, n: 4, ms: 460 },
  front: { src: '/spartan_front.png', fw: 48, fh: 68, n: 3, ms: 0 },
  tank: { src: '/tank_roll.png', fw: 96, fh: 56, n: 4, ms: 440 },
} as const;
type Kind = keyof typeof F;

type CharKind = 'lead' | 'group' | 'tank';
type Battle = { name: string; backdrop: string; side: 'left' | 'right'; character: CharKind; body: string };

const BATTLES: Battle[] = [
  {
    name: 'I · The Battle of Design',
    backdrop: '/scene_design.png',
    side: 'left',
    character: 'lead',
    body: 'Slop is born in the prompt. Freeform strings and “just ask nicely” hand the model the wheel — no contract, no guaranteed shape, a different answer every call.',
  },
  {
    name: 'II · The Battle of Architecture',
    backdrop: '/scene_arch.png',
    side: 'right',
    character: 'group',
    body: 'Then it spreads through the plumbing. Hand-rolled JSON parsing, retries, and glue code pile up around every call until the architecture itself becomes slop.',
  },
  {
    name: 'III · The Battle of Deployment',
    backdrop: '/scene_deploy.png',
    side: 'left',
    character: 'tank',
    body: 'And then it ships. Untyped outputs drift in production, slip past your tests, and surface as the 2am page — the slop you can’t see is the slop you can’t fix.',
  },
];

const STYLES = `
  .battles [data-leg], .battles [data-wave] {
    image-rendering: pixelated; image-rendering: -moz-crisp-edges; image-rendering: crisp-edges;
  }
  .battles [data-leg] {
    animation: wf2 var(--ms) steps(4) infinite; animation-play-state: paused;
  }
  .battles.is-scrolling [data-leg] { animation-play-state: running; }
  .battles [data-wave] { animation: wwave2 640ms steps(2) infinite; }
  @keyframes wf2 { from { background-position: 0 0; } to { background-position: calc(var(--sw) * -1) 0; } }
  @keyframes wwave2 { from { background-position: calc(var(--fw) * -1) 0; } to { background-position: calc(var(--fw) * -3) 0; } }
  @media (prefers-reduced-motion: reduce) { .battles [data-leg], .battles [data-wave] { animation: none !important; } }
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
    backgroundPosition: '0 0',
    backgroundSize: `${w * f.n}px ${height}px`,
    ['--sw' as string]: `${w * f.n}px`,
    ['--ms' as string]: `${f.ms}ms`,
  };
}
const widthOf = (kind: Kind, h: number) => Math.round((h * F[kind].fw) / F[kind].fh);
const clamp = (n: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, n));

// march a character from `start`% to `start+range`% of the panel as --r goes 0→1
const slide = (start: number, range: number) => `calc((${start} + var(--r) * ${range}) * 1%)`;

function Characters({ kind, scale, ground }: { kind: CharKind; scale: number; ground: number }) {
  if (kind === 'lead') {
    const size = Math.round(96 * scale);
    const fw = widthOf('front', size);
    const left = slide(2, 40);
    return (
      <>
        <div data-leg style={{ ...sprite('walk', size), left, bottom: ground, opacity: 'clamp(0, calc((0.8 - var(--r)) * 12), 1)' as unknown as number }} />
        <div
          data-wave
          style={{ ...sprite('front', size), left, bottom: ground, ['--fw' as string]: `${fw}px`, opacity: 'clamp(0, calc((var(--r) - 0.78) * 12), 1)' as unknown as number }}
        />
      </>
    );
  }
  if (kind === 'group') {
    const bases = [-16, -4, 8, 20, 32];
    const sizes = [86, 92, 88, 94, 90];
    return (
      <>
        {bases.map((b, i) => {
          const size = Math.round(sizes[i] * scale);
          return <div key={i} data-leg style={{ ...sprite('walk', size), left: slide(b, 46), bottom: ground, zIndex: bases.length - i }} />;
        })}
      </>
    );
  }
  // tank
  const size = Math.round(120 * scale);
  return <div data-leg style={{ ...sprite('tank', size), left: slide(-18, 52), bottom: ground }} />;
}

function ScenePanel({ battle, scale, ground, strip }: { battle: Battle; scale: number; ground: number; strip: number }) {
  return (
    <div
      className="relative aspect-[16/10] w-full overflow-hidden rounded-2xl border border-ink/10 bg-[#fbf7ed] shadow-sm"
      style={{
        opacity: 'var(--r)' as unknown as number,
        filter: 'blur(calc((1 - var(--r)) * 5px))',
        transform: 'scale(calc(0.965 + var(--r) * 0.035))',
      }}
    >
      <img
        src={battle.backdrop}
        alt=""
        aria-hidden="true"
        className="absolute inset-0 h-full w-full"
        style={{ objectFit: 'cover', objectPosition: 'center bottom' }}
      />
      <div aria-hidden="true" className="absolute inset-x-0 bottom-0" style={{ height: strip, background: '#d8c39a', borderTop: '1px solid rgba(169,142,97,0.45)' }} />
      <Characters kind={battle.character} scale={scale} ground={ground} />
    </div>
  );
}

function ParagraphPanel({ battle }: { battle: Battle }) {
  return (
    <div
      style={{
        opacity: 'var(--r)' as unknown as number,
        transform: 'translateY(calc((1 - var(--r)) * 18px))',
      }}
    >
      <div className="rounded-2xl border border-white/10 bg-[#1f1e1b]/80 p-7 shadow-lg backdrop-blur-md sm:p-9">
        <h3
          className="text-base font-bold tracking-wide text-[#e9c98f] sm:text-lg"
          style={{ fontFamily: "ui-monospace, 'SFMono-Regular', Menlo, monospace" }}
        >
          {battle.name}
        </h3>
        <p className="mt-4 text-lg leading-relaxed text-cream/90 sm:text-xl" style={{ fontFamily: "'Times New Roman', Times, serif" }}>
          {battle.body}
        </p>
      </div>
    </div>
  );
}

export default function WarScene() {
  const rootRef = useRef<HTMLDivElement>(null);
  const rowRefs = useRef<(HTMLElement | null)[]>([]);
  const idleTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const [scale, setScale] = useState(1);

  useEffect(() => {
    const onResize = () => setScale(clamp(window.innerWidth / 1180, 0.7, 1.1));
    onResize();
    window.addEventListener('resize', onResize);
    return () => window.removeEventListener('resize', onResize);
  }, []);

  useEffect(() => {
    const root = rootRef.current;
    if (!root) return;

    const apply = () => {
      const vh = window.innerHeight;
      for (const row of rowRefs.current) {
        if (!row) continue;
        const top = row.getBoundingClientRect().top;
        // 0 as the row enters from the bottom → 1 once it's well into view
        const r = clamp((vh - top) / (vh * 0.62), 0, 1);
        row.style.setProperty('--r', r.toFixed(3));
      }
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

  const strip = Math.round(18 * scale);
  const ground = Math.round(5 * scale);

  return (
    <div ref={rootRef} className="battles">
      <style>{STYLES}</style>
      {BATTLES.map((battle, i) => (
        <section
          key={battle.name}
          ref={(el) => {
            rowRefs.current[i] = el;
          }}
          className="mx-auto flex min-h-[88vh] max-w-6xl items-center px-5 sm:px-8"
          style={{ ['--r' as string]: '0' }}
        >
          <div className="grid w-full items-center gap-6 sm:gap-10 lg:grid-cols-2">
            {/* alternate which column the scene sits in (scene leads on mobile) */}
            <div className={battle.side === 'right' ? 'lg:order-2' : 'lg:order-1'}>
              <ScenePanel battle={battle} scale={scale} ground={ground} strip={strip} />
            </div>
            <div className={battle.side === 'right' ? 'lg:order-1' : 'lg:order-2'}>
              <ParagraphPanel battle={battle} />
            </div>
          </div>
        </section>
      ))}
    </div>
  );
}
