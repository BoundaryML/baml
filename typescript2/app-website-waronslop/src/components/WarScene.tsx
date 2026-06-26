'use client';

import { CSSProperties, useEffect, useRef, useState } from 'react';

// ─────────────────────────────────────────────────────────────────────────────
// Three "battle" rows. Each scene panel bleeds off the side of the screen and
// floats; the dark, transparent paragraph card sits on the inner side. As a row
// scrolls into view it materializes (blur→sharp, fade, settle) and its character
// walks in FROM THE SIDE the scene extends from, then holds (the lead settles
// into a wave). All scrubbed by per-row scroll progress (--r); legs only run
// while scrolling. Cross-browser: plain CSS vars + transforms (no scroll-timeline).
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
  .battles [data-leg] { animation: wf2 var(--ms) steps(4) infinite; animation-play-state: paused; }
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

// march a character to `rest`% of the panel, entering `travel`% away on the side
// it walks in from (dir +1 = from the left moving right, −1 = from the right).
const slide = (rest: number, travel: number, dir: number) =>
  `calc((${rest - dir * travel} + var(--r) * ${dir * travel}) * 1%)`;

function Characters({ kind, scale, ground, dir }: { kind: CharKind; scale: number; ground: number; dir: number }) {
  const flip = dir === -1 ? 'scaleX(-1)' : undefined; // face the way they walk

  if (kind === 'lead') {
    const size = Math.round(96 * scale);
    const fw = widthOf('front', size);
    const left = slide(42, 46, dir);
    return (
      <>
        <div data-leg style={{ ...sprite('walk', size), left, bottom: ground, transform: flip, opacity: 'clamp(0, calc((0.8 - var(--r)) * 12), 1)' as unknown as number }} />
        <div data-wave style={{ ...sprite('front', size), left, bottom: ground, transform: flip, ['--fw' as string]: `${fw}px`, opacity: 'clamp(0, calc((var(--r) - 0.78) * 12), 1)' as unknown as number }} />
      </>
    );
  }

  if (kind === 'group') {
    const rests = [22, 33, 44, 55, 66];
    const sizes = [86, 92, 88, 94, 90];
    return (
      <>
        {rests.map((rp, i) => {
          const size = Math.round(sizes[i] * scale);
          return <div key={i} data-leg style={{ ...sprite('walk', size), left: slide(rp, 48, dir), bottom: ground, zIndex: rests.length - i, transform: flip }} />;
        })}
      </>
    );
  }

  const size = Math.round(122 * scale);
  return <div data-leg style={{ ...sprite('tank', size), left: slide(40, 54, dir), bottom: ground, transform: flip }} />;
}

const MATERIALIZE: CSSProperties = {
  opacity: 'var(--r)' as unknown as number,
  filter: 'blur(calc((1 - var(--r)) * 5px))',
  transform: 'scale(calc(0.97 + var(--r) * 0.03))',
};

function ScenePanel({ battle, scale, ground, strip, dir, className }: { battle: Battle; scale: number; ground: number; strip: number; dir: number; className: string }) {
  return (
    <div className={className}>
      <div className="relative h-full w-full overflow-hidden rounded-2xl border border-ink/10 bg-[#fbf7ed] shadow-xl lg:rounded-3xl" style={MATERIALIZE}>
        <img src={battle.backdrop} alt="" aria-hidden="true" className="absolute inset-0 h-full w-full" style={{ objectFit: 'cover', objectPosition: 'center bottom' }} />
        <div aria-hidden="true" className="absolute inset-x-0 bottom-0" style={{ height: strip, background: '#d8c39a', borderTop: '1px solid rgba(169,142,97,0.45)' }} />
        <Characters kind={battle.character} scale={scale} ground={ground} dir={dir} />
      </div>
    </div>
  );
}

function ParagraphPanel({ battle, className }: { battle: Battle; className: string }) {
  return (
    <div className={className}>
      <div className="tweet-font rounded-2xl bg-[#2e2e2e]/75 p-6 shadow-lg backdrop-blur-sm sm:p-8" style={{ opacity: 'var(--r)' as unknown as number, transform: 'translateY(calc((1 - var(--r)) * 16px))' }}>
        <div className="text-sm font-semibold tracking-wide text-white/55">{battle.name}</div>
        <p className="mt-3 text-base leading-relaxed text-white/90 sm:text-lg">{battle.body}</p>
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
      {BATTLES.map((battle, i) => {
        const left = battle.side === 'left';
        const sceneClass = [
          'relative aspect-[16/10] w-full',
          'lg:absolute lg:top-1/2 lg:aspect-auto lg:h-[58vh] lg:w-[56vw] lg:-translate-y-1/2',
          left ? 'lg:left-[-4vw] lg:right-auto' : 'lg:right-[-4vw] lg:left-auto',
        ].join(' ');
        const paraClass = [
          'relative',
          'lg:absolute lg:top-1/2 lg:w-[34vw] lg:max-w-md lg:-translate-y-1/2',
          left ? 'lg:right-[6vw] lg:left-auto' : 'lg:left-[6vw] lg:right-auto',
        ].join(' ');
        return (
          <section
            key={battle.name}
            ref={(el) => {
              rowRefs.current[i] = el;
            }}
            className="relative flex min-h-[88vh] flex-col justify-center gap-6 overflow-hidden px-5 py-12 sm:px-8 lg:block lg:px-0 lg:py-0"
            style={{ ['--r' as string]: '0' }}
          >
            <ScenePanel battle={battle} scale={scale} ground={ground} strip={strip} dir={left ? 1 : -1} className={sceneClass} />
            <ParagraphPanel battle={battle} className={paraClass} />
          </section>
        );
      })}
    </div>
  );
}
