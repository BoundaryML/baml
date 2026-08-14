'use client';

import { CSSProperties, useEffect, useRef, useState } from 'react';

// Scroll-driven war hero: the battle stage pins while you scroll. Progress (--p)
// scrubs the column across the screen and cross-fades backdrops + info panels.

const F = {
  walk: { src: '/war-on-slop/spartan_walk.png', fw: 48, fh: 68, n: 4, ms: 460 },
  tank: { src: '/war-on-slop/tank_roll.png', fw: 96, fh: 56, n: 4, ms: 440 },
} as const;
type Kind = keyof typeof F;

type BattleScene = {
  id: string;
  backdrop: string;
  caption: string;
  title: string;
  body: string;
  link?: { href: string; label: string };
  images: { src: string; alt: string }[];
};

const SCENES: BattleScene[] = [
  {
    id: 'design',
    backdrop: '/war-on-slop/scene_design.png',
    caption: 'I · The Battle of Design',
    title: 'The battle of design',
    body: 'Code can be slop, writing cannot. We built BEPS, a full site for writing, reviewing, and managing detailed design specs for the BAML language.',
    link: { href: 'https://beps.boundaryml.com', label: 'beps.boundaryml.com' },
    images: [
      { src: '/war-on-slop/battle-design-kanban.png', alt: 'BEPS kanban board of proposals' },
      { src: '/war-on-slop/battle-design-bep50.png', alt: 'BEP-050 Metrics and Tracing IDs document' },
      { src: '/war-on-slop/battle-design-slack.png', alt: 'Slack thread announcing a new BEP' },
    ],
  },
  {
    id: 'arch',
    backdrop: '/war-on-slop/scene_arch.png',
    caption: 'II · The Battle of Architecture',
    title: 'The battle of architecture',
    body: 'We built a massive Rust crate tracing system to track dependencies across different parts of the BAML language. This helps keep our codebase clean and prevent breaking changes.',
    images: [{ src: '/war-on-slop/battle-arch-deps.png', alt: 'BAML dependency graph visualization' }],
  },
  {
    id: 'deploy',
    backdrop: '/war-on-slop/scene_deploy.png',
    caption: 'III · The Battle of Deployment',
    title: 'The battle of deployments',
    body: 'We have agents write thousands of BAML programs to see what breaks and what works.',
    link: { href: 'https://new.boundaryml.com/atb', label: 'new.boundaryml.com/atb' },
    images: [{ src: '/war-on-slop/battle-deploy-runs.png', alt: 'Agent runs dashboard for BAML deployments' }],
  },
];

const BACKDROPS = SCENES.map((s) => s.backdrop);
const SCROLL_VH = 300;

const STYLES = `
  .warscene2 [data-leg] {
    image-rendering: pixelated; image-rendering: -moz-crisp-edges; image-rendering: crisp-edges;
    animation-name: wf2; animation-timing-function: steps(4); animation-iteration-count: infinite;
    animation-duration: var(--ms); animation-play-state: paused;
  }
  .warscene2.is-scrolling [data-leg] { animation-play-state: running; }
  @keyframes wf2 { from { background-position: 0 0; } to { background-position: calc(var(--sw) * -1) 0; } }
  @media (prefers-reduced-motion: reduce) { .warscene2 [data-leg] { animation: none !important; } }

  .warscene2 .bd {
    position: absolute; left: 0; right: 0; bottom: var(--strip);
    width: 100%; height: auto; max-width: none; display: block;
  }
  @media (max-width: 767px) {
    .warscene2 .bd { top: 0; bottom: 0; height: 100%; object-fit: cover; object-position: center bottom; }
  }
`;

const SPAN = 240;

const COLUMN: { kind: Kind; size: number; off: number }[] = [
  { kind: 'walk', size: 92, off: -4 },
  { kind: 'walk', size: 86, off: -11 },
  { kind: 'walk', size: 96, off: -18 },
  { kind: 'walk', size: 90, off: -25 },
  { kind: 'walk', size: 94, off: -32 },
  { kind: 'walk', size: 88, off: -39 },
  { kind: 'walk', size: 97, off: -46 },
  { kind: 'tank', size: 118, off: -56 },
  { kind: 'walk', size: 93, off: -68 },
  { kind: 'walk', size: 91, off: -75 },
  { kind: 'walk', size: 95, off: -82 },
  { kind: 'walk', size: 89, off: -89 },
  { kind: 'walk', size: 96, off: -96 },
  { kind: 'tank', size: 112, off: -106 },
  { kind: 'walk', size: 94, off: -118 },
  { kind: 'walk', size: 87, off: -125 },
  { kind: 'walk', size: 92, off: -132 },
  { kind: 'walk', size: 98, off: -139 },
];

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

const clamp = (n: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, n));

function ScenePanel({
  scene,
  index,
  active,
}: {
  scene: BattleScene;
  index: number;
  active: boolean;
}) {
  const multi = scene.images.length > 1;

  return (
    <div
      className="absolute inset-x-0 top-0 z-20 px-3 pt-28 sm:px-6 sm:pt-32 lg:px-8"
      style={{
        // Snap to the thresholded active scene (Math.round of scroll progress)
        // and animate the swap, rather than cross-fading continuously with the
        // scroll position — that left two cards overlapping at partial opacity
        // (a fuzzy, unreadable in-between). The backdrops still cross-fade via
        // --b{index}; only the text card is thresholded.
        opacity: active ? 1 : 0,
        transform: active ? 'translateY(0)' : 'translateY(14px)',
        transition: 'opacity 280ms ease, transform 280ms ease',
        pointerEvents: active ? 'auto' : 'none',
        // The active card paints on top during the brief crossfade so the
        // outgoing one never bleeds through its text.
        zIndex: active ? 21 : 20,
      }}
      aria-hidden={!active}
    >
      <div className="tweet-font mx-auto grid max-w-7xl gap-5 rounded-2xl border border-wos-line/60 bg-wos-cream-hi/60 p-5 shadow-sm backdrop-blur-lg sm:gap-7 sm:p-7 lg:grid-cols-[minmax(0,1fr)_minmax(0,1.15fr)] lg:p-9">
        <div className="min-w-0">
          <p
            className="text-xs font-bold uppercase tracking-[0.14em] text-wos-ink-2 sm:text-[13px]"
            style={{ fontFamily: "ui-monospace, 'SFMono-Regular', Menlo, monospace" }}
          >
            {scene.caption}
          </p>
          <h3 className="mt-2 text-2xl font-bold leading-tight text-wos-ink sm:text-4xl">{scene.title}</h3>
          <p className="mt-3 text-base leading-relaxed text-wos-ink sm:text-[19px] sm:leading-relaxed">{scene.body}</p>
          {scene.link && (
            <a
              href={scene.link.href}
              target="_blank"
              rel="noopener noreferrer"
              className="mt-4 inline-block text-base font-bold text-wos-accent underline underline-offset-4 hover:text-wos-accent-deep sm:text-lg"
            >
              {scene.link.label}
            </a>
          )}
        </div>

        <div
          className={
            multi
              ? 'grid min-w-0 grid-cols-3 gap-2 sm:gap-3'
              : 'flex min-w-0 items-start justify-center lg:justify-end'
          }
        >
          {scene.images.map((img) => (
            <img
              key={img.src}
              src={img.src}
              alt={img.alt}
              className={
                multi
                  ? 'h-[120px] w-full rounded-lg border border-wos-line bg-white object-cover object-top shadow-sm sm:h-[170px] lg:h-[210px]'
                  : 'max-h-[210px] w-full rounded-lg border border-wos-line bg-white object-contain object-top shadow-sm sm:max-h-[290px] lg:max-h-[360px] lg:max-w-full'
              }
              loading="lazy"
            />
          ))}
        </div>
      </div>
    </div>
  );
}

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
      const s = p * (BACKDROPS.length - 1);
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
    <section ref={wrapRef} className="relative mt-2 sm:mt-0" style={{ height: `${SCROLL_VH}vh` }}>
      <div
        ref={sceneRef}
        className="warscene2 sticky top-0 h-screen w-full overflow-hidden"
        style={{
          ['--p' as string]: '0',
          ['--b0' as string]: '1',
          ['--b1' as string]: '0',
          ['--b2' as string]: '0',
          ['--strip' as string]: `${strip}px`,
        }}
      >
        <style>{STYLES}</style>

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
                zIndex: 10,
                transform: `translateX(calc((var(--p) * ${SPAN} + ${m.off}) * 1vw))`,
              }}
            />
          );
        })}

        {SCENES.map((s, i) => (
          <ScenePanel key={s.id} scene={s} index={i} active={scene === i} />
        ))}

        <div
          className="pointer-events-none absolute inset-x-0 bottom-7 z-30 flex justify-center transition-opacity duration-500"
          style={{ opacity: started ? 0 : 1 }}
        >
          <span className="tweet-font animate-bounce text-xs font-bold uppercase tracking-widest text-wos-ink-2">
            scroll to march ↓
          </span>
        </div>
      </div>
    </section>
  );
}
