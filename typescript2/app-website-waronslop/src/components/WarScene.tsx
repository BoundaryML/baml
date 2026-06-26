'use client';

import { CSSProperties, useEffect, useRef, useState } from 'react';

type Actor = 'legionnaire' | 'tank' | 'march-out';

type Scene = {
  image: string;
  side: 'left' | 'right';
  title: string;
  body: string;
  actor?: Actor;
};

const SCENES: Scene[] = [
  {
    image: '/scene_design.png',
    side: 'left',
    title: 'The battle of design',
    body: 'Code can be slop, writing cannot. We built a full vibecoded "notion-like" site for writing, reviewing, and managing detailed design specs for the BAML language (BEPS).',
    actor: 'legionnaire',
  },
  {
    image: '/scene_arch.png',
    side: 'right',
    title: 'The battle of architecture',
    body: 'We built a massive rust crate tracing system to track dependencies across different parts of the BAML language. This helps keep our codebase clean and prevent breaking changes.',
    actor: 'tank',
  },
  {
    image: '/scene_deploy.png',
    side: 'left',
    title: 'The battle of deployments',
    body: 'We have agents write thousands of baml programs to see what breaks and what works.',
    actor: 'march-out',
  },
];

// ── Sprite actors ────────────────────────────────────────────────────────────
// All movement is pure CSS keyframes (translateX) gated on an `active` flag the
// scene sets once it scrolls into view — JS never drives transforms frame to
// frame (that was flaky in Firefox). Sheets are 48×68 (walk/front) and 96×56
// (tank); we step background-position with the shorthand + explicit `0` Y so
// Firefox animates it.
const SPARTAN_RATIO = 48 / 68;
const TANK_RATIO = 96 / 56;

const ACTOR_STYLES = `
.ws-pixel { image-rendering: pixelated; image-rendering: crisp-edges; image-rendering: -moz-crisp-edges; }
@keyframes ws-walk { from { background-position: 0 0; } to { background-position: var(--sheetNeg) 0; } }
@keyframes ws-wave { from { background-position: var(--fw1) 0; } to { background-position: var(--fw3) 0; } }
@keyframes ws-movein { from { transform: translateX(var(--from)) scaleX(var(--flip, 1)); } to { transform: translateX(var(--to)) scaleX(var(--flip, 1)); } }
@keyframes ws-march { from { transform: translateX(var(--from)); } to { transform: translateX(var(--to)); } }
@keyframes ws-fade-in { from { opacity: 0; } to { opacity: 1; } }
@keyframes ws-fade-out { from { opacity: 1; } to { opacity: 0; } }
@media (prefers-reduced-motion: reduce) {
  .ws-pixel, .ws-actor { animation: none !important; }
}
`;

const v = (key: string, value: string | number) => ({ [key]: value }) as CSSProperties;

// Scene 1 — a legionnaire marches in from the left, turns front, waves, then holds.
function Legionnaire({ active }: { active: boolean }) {
  const H = 104;
  const ww = Math.round(H * SPARTAN_RATIO);
  const walkSheet = ww * 4;
  const frontSheet = ww * 3;

  return (
    <div
      className="ws-actor"
      style={{
        position: 'absolute',
        bottom: '7%',
        left: 0,
        height: H,
        width: ww,
        transform: 'translateX(-6vw)',
        animation: active ? 'ws-movein 2600ms linear forwards' : 'none',
        ...v('--from', '-6vw'),
        ...v('--to', '24vw'),
      }}
    >
      <div
        className="ws-pixel"
        style={{
          position: 'absolute',
          inset: 0,
          backgroundImage: 'url(/spartan_walk.png)',
          backgroundRepeat: 'no-repeat',
          backgroundSize: `${walkSheet}px ${H}px`,
          ...v('--sheetNeg', `-${walkSheet}px`),
          animation: active
            ? 'ws-walk 600ms steps(4) infinite, ws-fade-out 160ms linear 2600ms forwards'
            : 'none',
        }}
      />
      <div
        className="ws-pixel"
        style={{
          position: 'absolute',
          inset: 0,
          opacity: 0,
          backgroundImage: 'url(/spartan_front.png)',
          backgroundRepeat: 'no-repeat',
          backgroundPosition: '0 0',
          backgroundSize: `${frontSheet}px ${H}px`,
          ...v('--fw1', `-${ww}px`),
          ...v('--fw3', `-${ww * 3}px`),
          animation: active
            ? 'ws-fade-in 160ms linear 2600ms forwards, ws-wave 460ms steps(2) 2780ms 3'
            : 'none',
        }}
      />
    </div>
  );
}

// Scene 2 — the BAML tank rolls in from the right (sheet faces right, so flip it).
function Tank({ active }: { active: boolean }) {
  const H = 86;
  const ww = Math.round(H * TANK_RATIO);
  const sheet = ww * 4;

  return (
    <div
      className="ws-actor ws-pixel"
      style={{
        position: 'absolute',
        bottom: '5%',
        left: 0,
        height: H,
        width: ww,
        backgroundImage: 'url(/tank_roll.png)',
        backgroundRepeat: 'no-repeat',
        backgroundSize: `${sheet}px ${H}px`,
        transform: 'translateX(86vw) scaleX(-1)',
        animation: active
          ? 'ws-movein 3200ms linear forwards, ws-walk 360ms steps(4) infinite'
          : 'none',
        ...v('--sheetNeg', `-${sheet}px`),
        ...v('--from', '86vw'),
        ...v('--to', '34vw'),
        ...v('--flip', -1),
      }}
    />
  );
}

// Scene 3 — a loose file of legionnaires walking out across the frame from the left.
const MARCHERS = [
  { H: 98, bottom: '6%', dur: 9000, delay: 0 },
  { H: 84, bottom: '11%', dur: 11000, delay: 1600 },
  { H: 108, bottom: '3%', dur: 8200, delay: 3400 },
  { H: 90, bottom: '8%', dur: 10000, delay: 5200 },
];

function MarchOut({ active }: { active: boolean }) {
  return (
    <>
      {MARCHERS.map((m, i) => {
        const ww = Math.round(m.H * SPARTAN_RATIO);
        const sheet = ww * 4;
        return (
          <div
            key={i}
            className="ws-actor ws-pixel"
            style={{
              position: 'absolute',
              bottom: m.bottom,
              left: 0,
              height: m.H,
              width: ww,
              backgroundImage: 'url(/spartan_walk.png)',
              backgroundRepeat: 'no-repeat',
              backgroundSize: `${sheet}px ${m.H}px`,
              transform: 'translateX(-14vw)',
              animation: active
                ? `ws-march ${m.dur}ms linear ${m.delay}ms infinite, ws-walk 600ms steps(4) ${-(i % 4) * 150}ms infinite`
                : 'none',
              ...v('--sheetNeg', `-${sheet}px`),
              ...v('--from', '-14vw'),
              ...v('--to', '120vw'),
            }}
          />
        );
      })}
    </>
  );
}

function ActorLayer({ actor, active }: { actor: Actor; active: boolean }) {
  if (actor === 'legionnaire') return <Legionnaire active={active} />;
  if (actor === 'tank') return <Tank active={active} />;
  return <MarchOut active={active} />;
}

const clamp = (value: number, min: number, max: number) => Math.min(max, Math.max(min, value));

// A tiled grid of dots that grow with --r, so the image "assembles" from
// particles as the scene scrolls into view (and scatters back out as it leaves).
const PARTICLE_MASK =
  'radial-gradient(circle, #000 calc(var(--r) * 145% - 14%), transparent calc(var(--r) * 145%))';

const materializeStyle = (fromLeft: boolean): CSSProperties =>
  ({
    opacity: 'calc(var(--r) * 1.25)',
    filter: 'blur(calc((1 - var(--r)) * 5px))',
    transform: `translateX(calc((1 - var(--r)) * ${fromLeft ? '-4vw' : '4vw'})) translateY(calc((1 - var(--r)) * 14px)) scale(calc(0.96 + var(--r) * 0.04))`,
    maskImage: PARTICLE_MASK,
    WebkitMaskImage: PARTICLE_MASK,
    maskSize: '8px 8px',
    WebkitMaskSize: '8px 8px',
    maskRepeat: 'repeat',
    WebkitMaskRepeat: 'repeat',
    willChange: 'opacity, transform, mask-image',
  }) as CSSProperties;

function FloatingScene({ scene }: { scene: Scene }) {
  const fromLeft = scene.side === 'left';
  const textOnRight = fromLeft;
  const imgRef = useRef<HTMLImageElement | null>(null);
  const cardRef = useRef<HTMLDivElement | null>(null);
  const sectionRef = useRef<HTMLElement | null>(null);
  const [actorActive, setActorActive] = useState(false);

  useEffect(() => {
    const img = imgRef.current;
    const card = cardRef.current;
    if (!img || !card) return;

    const sync = () => {
      card.style.height = window.innerWidth >= 1024 ? `${img.offsetHeight}px` : '';
    };

    sync();
    const observer = new ResizeObserver(sync);
    observer.observe(img);
    window.addEventListener('resize', sync);
    img.addEventListener('load', sync);

    return () => {
      observer.disconnect();
      window.removeEventListener('resize', sync);
      img.removeEventListener('load', sync);
    };
  }, []);

  // Kick off the sprite choreography once the scene is meaningfully in view.
  useEffect(() => {
    const el = sectionRef.current;
    if (!el || !scene.actor) return;

    const io = new IntersectionObserver(
      (entries) => {
        for (const entry of entries) {
          if (entry.isIntersecting && entry.intersectionRatio >= 0.35) {
            setActorActive(true);
            io.disconnect();
            break;
          }
        }
      },
      { threshold: [0, 0.35, 0.6] },
    );
    io.observe(el);
    return () => io.disconnect();
  }, [scene.actor]);

  return (
    <section
      ref={sectionRef}
      className="relative flex min-h-[72vh] items-center overflow-hidden py-2 sm:min-h-[86vh] sm:py-4"
    >
      <div
        className={[
          'relative w-[118vw] max-w-none sm:w-[94vw] lg:w-[82vw]',
          fromLeft ? '-ml-[18vw] sm:-ml-[10vw] lg:-ml-[7vw]' : 'ml-auto -mr-[18vw] sm:-mr-[10vw] lg:-mr-[7vw]',
        ].join(' ')}
      >
        <div className="relative w-full" style={materializeStyle(fromLeft)}>
          <img
            ref={imgRef}
            src={scene.image}
            alt=""
            aria-hidden="true"
            className="block h-auto w-full select-none"
            draggable={false}
          />
        </div>

        {scene.actor && (
          <div className="pointer-events-none absolute inset-0 z-[5] hidden lg:block">
            <ActorLayer actor={scene.actor} active={actorActive} />
          </div>
        )}
      </div>

      <div
        ref={cardRef}
        className={[
          'absolute top-1/2 z-10 flex flex-col justify-center rounded-2xl px-8 py-7 text-balance text-ink',
          'max-lg:bottom-8 max-lg:left-6 max-lg:right-6 max-lg:top-auto',
          textOnRight ? 'lg:left-[75vw] lg:right-0' : 'lg:left-0 lg:right-[75vw]',
        ].join(' ')}
        style={{
          backgroundColor: '#fffef5',
          opacity: 'calc((var(--r) - 0.75) * 4)' as unknown as number,
          transform: 'translateY(calc(-50% + (1 - var(--r)) * 18px))',
        }}
      >
        <h3 className="text-[26px] font-bold leading-tight">{scene.title}</h3>
        <p className="tweet-font mt-4 text-[19px] leading-8 text-ink">{scene.body}</p>
      </div>
    </section>
  );
}

export default function WarScene() {
  const rowRefs = useRef<(HTMLDivElement | null)[]>([]);

  useEffect(() => {
    const updateProgress = () => {
      const viewportHeight = window.innerHeight;

      for (const row of rowRefs.current) {
        if (!row) continue;
        const rect = row.getBoundingClientRect();
        // Assemble as the scene enters from the bottom...
        const enter = clamp((viewportHeight - rect.top) / (viewportHeight * 0.6), 0, 1);
        // ...and scatter back out only once it starts leaving past the top.
        const exit = clamp(rect.bottom / (viewportHeight * 0.4), 0, 1);
        const progress = Math.min(enter, exit);
        row.style.setProperty('--r', progress.toFixed(3));
      }
    };

    updateProgress();
    window.addEventListener('scroll', updateProgress, { passive: true });
    window.addEventListener('resize', updateProgress);

    return () => {
      window.removeEventListener('scroll', updateProgress);
      window.removeEventListener('resize', updateProgress);
    };
  }, []);

  return (
    <div className="mt-4 sm:mt-0">
      <style>{ACTOR_STYLES}</style>
      {SCENES.map((scene, index) => (
        <div
          key={scene.image}
          ref={(element) => {
            rowRefs.current[index] = element;
          }}
          style={{ ['--r' as string]: '0' }}
        >
          <FloatingScene scene={scene} />
        </div>
      ))}
    </div>
  );
}
