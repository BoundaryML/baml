'use client';

import Image from 'next/image';
import { type CSSProperties, useEffect, useRef, useState } from 'react';
import { section } from '@/components/home/analytics';
import { PAGE_WRAP } from '@/components/home/page-shell';

// Scroll-driven war hero: the battle stage pins while you scroll. Progress (--p)
// scrubs the column across the screen and cross-fades backdrops + info panels.

const F = {
  tank: { fh: 56, fw: 96, ms: 440, n: 4, src: '/war-on-slop/tank_roll.png' },
  walk: { fh: 68, fw: 48, ms: 460, n: 4, src: '/war-on-slop/spartan_walk.png' },
} as const;
type Kind = keyof typeof F;

type BattleScene = {
  id: string;
  backdrop: string;
  caption: string;
  title: string;
  body: string;
  link?: { href: string; label: string };
  images: { alt: string; height: number; src: string; width: number }[];
};

const SCENES: BattleScene[] = [
  {
    backdrop: '/war-on-slop/scene_design.png',
    body: 'Code can be slop, writing cannot. We built BEPS, a full site for writing, reviewing, and managing detailed design specs for the BAML language.',
    caption: 'I · The Battle of Design',
    id: 'design',
    images: [
      {
        alt: 'BEPS kanban board of proposals',
        height: 1834,
        src: '/war-on-slop/battle-design-kanban.png',
        width: 2960,
      },
      {
        alt: 'BEP-050 Metrics and Tracing IDs document',
        height: 1786,
        src: '/war-on-slop/battle-design-bep50.png',
        width: 2592,
      },
      {
        alt: 'Slack thread announcing a new BEP',
        height: 1518,
        src: '/war-on-slop/battle-design-slack.png',
        width: 1498,
      },
    ],
    link: { href: 'https://beps.boundaryml.com', label: 'beps.boundaryml.com' },
    title: 'The battle of design',
  },
  {
    backdrop: '/war-on-slop/scene_arch.png',
    body: 'We built a massive Rust crate tracing system to track dependencies across different parts of the BAML language. This helps keep our codebase clean and prevent breaking changes.',
    caption: 'II · The Battle of Architecture',
    id: 'arch',
    images: [
      {
        alt: 'BAML dependency graph visualization',
        height: 1003,
        src: '/war-on-slop/battle-arch-deps.png',
        width: 1920,
      },
    ],
    title: 'The battle of architecture',
  },
  {
    backdrop: '/war-on-slop/scene_deploy.png',
    body: 'We have agents write thousands of BAML programs to see what breaks and what works.',
    caption: 'III · The Battle of Deployment',
    id: 'deploy',
    images: [
      {
        alt: 'Agent runs dashboard for BAML deployments',
        height: 1550,
        src: '/war-on-slop/battle-deploy-runs.png',
        width: 2490,
      },
    ],
    title: 'The battle of deployments',
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
    position: absolute; left: 0; right: 0; bottom: 0;
    width: 100%; height: auto; max-width: none; display: block;
  }
  @media (max-width: 767px) {
    .warscene2 .bd { top: 0; bottom: 0; height: 100%; object-fit: cover; object-position: center bottom; }
  }
`;

const SPAN = 240;

const COLUMN: { kind: Kind; size: number; off: number }[] = [
  { kind: 'walk', off: -4, size: 92 },
  { kind: 'walk', off: -11, size: 86 },
  { kind: 'walk', off: -18, size: 96 },
  { kind: 'walk', off: -25, size: 90 },
  { kind: 'walk', off: -32, size: 94 },
  { kind: 'walk', off: -39, size: 88 },
  { kind: 'walk', off: -46, size: 97 },
  { kind: 'tank', off: -56, size: 118 },
  { kind: 'walk', off: -68, size: 93 },
  { kind: 'walk', off: -75, size: 91 },
  { kind: 'walk', off: -82, size: 95 },
  { kind: 'walk', off: -89, size: 89 },
  { kind: 'walk', off: -96, size: 96 },
  { kind: 'tank', off: -106, size: 112 },
  { kind: 'walk', off: -118, size: 94 },
  { kind: 'walk', off: -125, size: 87 },
  { kind: 'walk', off: -132, size: 92 },
  { kind: 'walk', off: -139, size: 98 },
];

/**
 * Styles for an animated sprite of `kind`, scaled to `height`, with its strip
 * width and frame time as CSS variables.
 */
function sprite(kind: Kind, height: number): CSSProperties {
  const f = F[kind];
  const w = Math.round((height * f.fw) / f.fh);
  return {
    backgroundImage: `url(${f.src})`,
    backgroundRepeat: 'no-repeat',
    backgroundSize: `${w * f.n}px ${height}px`,
    height,
    position: 'absolute',
    width: w,
    ['--sw' as string]: `${w * f.n}px`,
    ['--ms' as string]: `${f.ms}ms`,
  };
}

/** `n` held between `lo` and `hi`. */
const clamp = (n: number, lo: number, hi: number) =>
  Math.min(hi, Math.max(lo, n));

/** The text card and images for one battle scene, shown while it's active. */
function ScenePanel({
  scene,
  active,
}: {
  scene: BattleScene;
  active: boolean;
}) {
  const multi = scene.images.length > 1;

  return (
    <div
      aria-hidden={!active}
      className="absolute inset-x-0 top-0 z-20 pt-24 sm:pt-28"
      style={{
        // Snap to the thresholded active scene (Math.round of scroll progress)
        // and animate the swap, rather than cross-fading continuously with the
        // scroll position — that left two cards overlapping at partial opacity
        // (a fuzzy, unreadable in-between). The backdrops still cross-fade via
        // --b{index}; only the text card is thresholded.
        opacity: active ? 1 : 0,
        pointerEvents: active ? 'auto' : 'none',
        transform: active ? 'translateY(0)' : 'translateY(14px)',
        transition: 'opacity 280ms ease, transform 280ms ease',
        // The active card paints on top during the brief crossfade so the
        // outgoing one never bleeds through its text.
        zIndex: active ? 21 : 20,
      }}
    >
      <div className={PAGE_WRAP}>
        <div className="grid gap-5 rounded-2xl border border-rule bg-ground/75 p-5 backdrop-blur-lg sm:gap-7 sm:p-7 lg:grid-cols-[minmax(0,1fr)_minmax(0,1.15fr)] lg:p-9">
          <div className="min-w-0">
            <p className="v4-caption text-muted">{scene.caption}</p>
            <h3 className="mt-3 v4-h3">{scene.title}</h3>
            <p className="mt-3 text-[16px] leading-[1.4] text-ink-2 sm:text-[20px]">
              {scene.body}
            </p>
            {scene.link && (
              <a
                className="mt-4 inline-block text-base font-medium text-accent underline underline-offset-4 hover:text-accent-hover sm:text-lg"
                href={scene.link.href}
                rel="noopener noreferrer"
                target="_blank"
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
              <Image
                alt={img.alt}
                className={
                  multi
                    ? 'h-[120px] w-full rounded-lg border border-rule bg-raised object-cover object-top sm:h-[170px] lg:h-[210px]'
                    : 'max-h-[210px] w-full rounded-lg border border-rule bg-raised object-contain object-top sm:max-h-[290px] lg:max-h-[360px] lg:max-w-full'
                }
                height={img.height}
                key={img.src}
                sizes={
                  multi
                    ? '(min-width: 1024px) 220px, 33vw'
                    : '(min-width: 1024px) 640px, 100vw'
                }
                src={img.src}
                width={img.width}
              />
            ))}
          </div>
        </div>
      </div>
    </div>
  );
}

/**
 * The scroll-driven war hero: a pinned battle stage whose marching column and
 * scenes advance with scroll.
 */
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
      setScale(
        clamp(
          Math.min(window.innerWidth / 1280, window.innerHeight / 760),
          0.62,
          1.3,
        ),
      );
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
      const p =
        total > 0 ? clamp(-wrap.getBoundingClientRect().top / total, 0, 1) : 0;
      root.style.setProperty('--p', String(p));
      const s = p * (BACKDROPS.length - 1);
      for (let i = 0; i < BACKDROPS.length; i++) {
        root.style.setProperty(
          `--b${i}`,
          String(clamp(1 - Math.abs(s - i), 0, 1)),
        );
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
      idleTimer.current = setTimeout(
        () => root.classList.remove('is-scrolling'),
        150,
      );
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

  // The soldiers march along the stone ground at the foot of each backdrop.
  const ground = Math.round(6 * scale);

  return (
    <section
      {...section('slop_battles')}
      className="relative mt-2 sm:mt-0"
      ref={wrapRef}
      style={{ height: `${SCROLL_VH}vh` }}
    >
      <div
        className="warscene2 sticky top-0 h-screen w-full overflow-hidden"
        ref={sceneRef}
        style={{
          ['--p' as string]: '0',
          ['--b0' as string]: '1',
          ['--b1' as string]: '0',
          ['--b2' as string]: '0',
        }}
      >
        <style>{STYLES}</style>

        {BACKDROPS.map((src, i) => (
          <Image
            alt=""
            aria-hidden="true"
            className="bd"
            height={654}
            key={src}
            priority={i === 0}
            sizes="100vw"
            src={src}
            style={{ opacity: `var(--b${i})` as unknown as number }}
            width={1526}
          />
        ))}

        {COLUMN.map((m) => {
          const size = Math.round(m.size * scale);
          return (
            <div
              data-leg
              key={`${m.kind}${m.off}`}
              style={{
                ...sprite(m.kind, size),
                bottom: ground,
                left: 0,
                transform: `translateX(calc((var(--p) * ${SPAN} + ${m.off}) * 1vw))`,
                zIndex: 10,
              }}
            />
          );
        })}

        {SCENES.map((s, i) => (
          <ScenePanel active={scene === i} key={s.id} scene={s} />
        ))}

        <div
          className="pointer-events-none absolute inset-x-0 bottom-7 z-30 flex justify-center transition-opacity duration-500"
          style={{ opacity: started ? 0 : 1 }}
        >
          <span className="animate-bounce rounded-full border border-rule bg-ground/75 px-3 py-1.5 v4-caption text-ink-2 backdrop-blur-md">
            scroll to march ↓
          </span>
        </div>
      </div>
    </section>
  );
}
