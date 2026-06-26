'use client';

import { CSSProperties, useEffect, useRef, useState } from 'react';

// ─────────────────────────────────────────────────────────────────────────────
// The march against slop, on the site's cream background, before a skyline of
// Roman temples. A lone Spartan walks out and waves; then the army marches in,
// in tight batches — each soldier crosses the screen, and some kneel mid-way to
// loose a volley before walking on. The BAML tank rolls along.
//
// Everything moves with PURE CSS keyframe animations (transform / opacity /
// background-position) — no JS-driven transforms — so it renders identically in
// Chrome and Firefox. Sprites generated with Aseprite.
// ─────────────────────────────────────────────────────────────────────────────

const F = {
  walk:   { src: '/spartan_walk.png',  fw: 48, fh: 68, n: 4 },
  front:  { src: '/spartan_front.png', fw: 48, fh: 68, n: 3 },
  kneel:  { src: '/spartan_kneel.png', fw: 48, fh: 68, n: 8 },
  archer: { src: '/spartan_archer.png',fw: 48, fh: 68, n: 9 },
  tank:   { src: '/tank_roll.png',     fw: 96, fh: 56, n: 4 },
} as const;
type Kind = keyof typeof F;

const CAPTIONS = [
  'I · The Battle of Design',
  'II · The Battle of Architecture',
  'III · The Battle of Deployment',
];
// a distinct hand-drawn backdrop scene per battle (1526×654 panoramas whose
// foreground/ground sits along the bottom edge — where the column marches)
const BACKDROPS = ['/scene_design.png', '/scene_arch.png', '/scene_deploy.png'];

const GROUND = 8;
const WALK_MS = 480;       // one stride of the leg cycle
const SPEED = 7;           // vw per second — slow, deliberate
const START_VW = -22;      // enters from off the left edge
const END_VW = 122;        // exits off the right edge
const LEAD_CENTER = 47.5;  // where the lead pauses to wave

// One kneeler cycle: walk in → kneel down → loose volley → stand up → walk on.
// The kneel SPOT and how long the volley is held vary per battalion + per scene
// (see KNEEL_VARIANTS), so each kneeler's keyframes are generated per-instance.
const KN = { kneelDown: 800, standUp: 800 };
const MAX_DRAW_MS = 3200; // longest volley hold any variant uses — bounds scene timing

// [scene][battalion] → where the archer kneels (vw) and how long the volley holds.
// No two kneelers in a scene stop at the same spot, and it shifts every scene.
const KNEEL_VARIANTS: { vw: number; draw: number }[][] = [
  [{ vw: 38, draw: 2600 }, { vw: 63, draw: 3100 }],
  [{ vw: 57, draw: 3000 }, { vw: 31, draw: 2300 }],
  [{ vw: 46, draw: 2200 }, { vw: 70, draw: 2900 }],
];

// The lead is a one-shot intro: walk in → wave → walk on at the head of the column.
const LD = { walkIn: 10000, wave: 2000, walkOut: 10600 };
const LEAD_MS = LD.walkIn + LD.wave + LD.walkOut;
const lp1 = (100 * LD.walkIn) / LEAD_MS;
const lp2 = (100 * (LD.walkIn + LD.wave)) / LEAD_MS;

const ARMY_START = LD.walkIn + LD.wave; // soldiers appear once the lead marches on
const LEAD_GONE = LEAD_MS + 400;        // lead has fully exited the right edge
const LEAD_SIZE = 94;                   // same scale as the rank and file
const TANK_DELAY = 6500;                // armour brings up the rear of the two battalions

const pc = (n: number) => `${n.toFixed(2)}%`;

const STYLES = `
  .warscene div { image-rendering: pixelated; image-rendering: -moz-crisp-edges; image-rendering: crisp-edges; }

  /* sprite-sheet frame cycling (animate the position shorthand — Firefox-safe) */
  @keyframes wf     { from { background-position: 0 0; } to { background-position: calc(var(--sw) * -1) 0; } }
  @keyframes wwave  { from { background-position: calc(var(--fw) * -1) 0; } to { background-position: calc(var(--fw) * -3) 0; } }

  /* horizontal travel */
  @keyframes wmarch { from { transform: translateX(${START_VW}vw); } to { transform: translateX(${END_VW}vw); } }
  @keyframes wtankx { from { transform: translateX(-26vw); } to { transform: translateX(128vw); } }
  @keyframes warrow { 0% { transform: translateX(0); opacity: 0; } 10% { opacity: 1; } 100% { transform: translateX(210px); opacity: 0; } }
  @keyframes wbg    { from { opacity: 0; } to { opacity: 1; } }

  /* (kneeler keyframes are generated per-instance — see kneelerKeyframes) */

  /* lead: one-shot walk in → wave → walk on */
  @keyframes lmove {
    0% { transform: translateX(${START_VW}vw); }
    ${pc(lp1)} { transform: translateX(${LEAD_CENTER}vw); }
    ${pc(lp2)} { transform: translateX(${LEAD_CENTER}vw); }
    100% { transform: translateX(${END_VW}vw); }
  }
  @keyframes lwalk  { 0%,${pc(lp1)} { opacity: 1; } ${pc(lp1 + 0.05)},${pc(lp2 - 0.05)} { opacity: 0; } ${pc(lp2)},100% { opacity: 1; } }
  @keyframes lfront { 0%,${pc(lp1 - 0.05)} { opacity: 0; } ${pc(lp1)},${pc(lp2)} { opacity: 1; } ${pc(lp2 + 0.05)},100% { opacity: 0; } }

  @media (prefers-reduced-motion: reduce) { .warscene * { animation: none !important; } }
`;

function sprite(kind: Kind, height: number): CSSProperties {
  const f = F[kind];
  const w = Math.round((height * f.fw) / f.fh);
  return {
    position: 'absolute', width: w, height,
    backgroundImage: `url(${f.src})`, backgroundRepeat: 'no-repeat',
    backgroundPosition: '0 0', backgroundSize: `${w * f.n}px ${height}px`,
    ['--sw' as string]: `${w * f.n}px`,
  };
}
const widthOf = (kind: Kind, height: number) => Math.round((height * F[kind].fw) / F[kind].fh);
const frames = (kind: Kind, ms: number) => `wf ${ms}ms steps(${F[kind].n}) infinite`;
const ms = (fromVw: number, toVw: number) => (Math.abs(toVw - fromVw) / SPEED) * 1000;
const CROSS_MS = ms(START_VW, END_VW);
const TANK_MS = 22000;
// walk-in + walk-out always covers the same END−START distance, so a kneeler's
// total only varies by its volley hold; bound the longest one for scene timing.
const KNEELER_MS_MAX = CROSS_MS + KN.kneelDown + MAX_DRAW_MS + KN.standUp;

// Per-instance kneeler keyframes: the parent travels in, HOLDS at `kneelVw`,
// then travels on; three stacked sheets (walk/kneel/archer) cross-fade by phase.
// `uid` makes the animation names unique so every kneeler can stop somewhere new.
function kneelerKeyframes(uid: string, kneelVw: number, k7: string, p1: number, p2: number, p3: number, p4: number) {
  const e = 0.05;
  return `
  @keyframes kmove_${uid} {
    0% { transform: translateX(${START_VW}vw); }
    ${pc(p1)} { transform: translateX(${kneelVw}vw); }
    ${pc(p4)} { transform: translateX(${kneelVw}vw); }
    100% { transform: translateX(${END_VW}vw); }
  }
  @keyframes kwalk_${uid}  { 0%,${pc(p1)} { opacity: 1; } ${pc(p1 + e)},${pc(p4 - e)} { opacity: 0; } ${pc(p4)},100% { opacity: 1; } }
  @keyframes kkneel_${uid} { 0%,${pc(p1 - e)} { opacity: 0; } ${pc(p1)},${pc(p2)} { opacity: 1; } ${pc(p2 + e)},${pc(p3 - e)} { opacity: 0; } ${pc(p3)},${pc(p4)} { opacity: 1; } ${pc(p4 + e)},100% { opacity: 0; } }
  @keyframes karch_${uid}  { 0%,${pc(p2 - e)} { opacity: 0; } ${pc(p2)},${pc(p3)} { opacity: 1; } ${pc(p3 + e)},100% { opacity: 0; } }
  @keyframes kframes_${uid} {
    0% { background-position: 0 0; }
    ${pc(p1)} { background-position: 0 0; animation-timing-function: steps(${F.kneel.n}, jump-none); }
    ${pc(p2)} { background-position: ${k7} 0; }
    ${pc(p3)} { background-position: ${k7} 0; animation-timing-function: steps(${F.kneel.n}, jump-none); }
    ${pc(p4)} { background-position: 0 0; }
    100% { background-position: 0 0; }
  }`;
}

// ── lead Spartan: marches in, waves, then marches on at the head of the column
function Lead({ size, ground }: { size: number; ground: number }) {
  const w = widthOf('walk', size);
  const fw = widthOf('front', size);
  return (
    <div style={{ position: 'absolute', left: 0, bottom: ground, width: w, height: size, animation: `lmove ${LEAD_MS}ms linear forwards` }}>
      <div style={{ ...sprite('walk', size), left: 0, bottom: 0, animation: `${frames('walk', WALK_MS)}, lwalk ${LEAD_MS}ms linear forwards` }} />
      <div style={{ ...sprite('front', size), left: 0, bottom: 0, ['--fw' as string]: `${fw}px`, animation: `wwave 480ms steps(2) infinite, lfront ${LEAD_MS}ms linear forwards` }} />
    </div>
  );
}

// ── a soldier who just walks across (enters left, exits right) ──────────────
function Walker({ size, delay, z, ground }: { size: number; delay: number; z: number; ground: number }) {
  // Base transform parks the soldier off the left edge during its stagger delay
  // (matching wmarch's `from`), so it isn't stuck at translateX(0).
  return (
    <div style={{ ...sprite('walk', size), left: 0, bottom: ground, zIndex: z, transform: `translateX(${START_VW}vw)`, animation: `${frames('walk', WALK_MS)}, wmarch ${CROSS_MS}ms linear ${delay}ms forwards` }} />
  );
}

// ── a soldier that walks in, kneels to loose a volley, then walks on ──────────
// `kneelVw` (where it stops) and `drawMs` (how long it holds) vary per battalion
// + per scene, so each kneeler emits its OWN keyframes (keyed by `uid`). Walk
// speed stays exactly SPEED everywhere — the walk-in/out durations are derived
// from the real distance to/from the kneel spot, so the feet never slip.
function Kneeler({ size, delay, z, ground, kneelVw, drawMs, uid }: { size: number; delay: number; z: number; ground: number; kneelVw: number; drawMs: number; uid: string }) {
  const w = widthOf('kneel', size);
  const k7 = `-${7 * w}px`;
  const walkIn = ((kneelVw - START_VW) / SPEED) * 1000;
  const walkOut = ((END_VW - kneelVw) / SPEED) * 1000;
  const total = walkIn + KN.kneelDown + drawMs + KN.standUp + walkOut;
  const p1 = (100 * walkIn) / total;
  const p2 = (100 * (walkIn + KN.kneelDown)) / total;
  const p3 = (100 * (walkIn + KN.kneelDown + drawMs)) / total;
  const p4 = (100 * (walkIn + KN.kneelDown + drawMs + KN.standUp)) / total;
  const A = `${Math.round(total)}ms linear ${delay}ms forwards`;
  return (
    <div style={{ position: 'absolute', left: 0, bottom: ground, width: w, height: size, zIndex: z, transform: `translateX(${START_VW}vw)`, animation: `kmove_${uid} ${A}` }}>
      <style>{kneelerKeyframes(uid, kneelVw, k7, p1, p2, p3, p4)}</style>
      <div style={{ ...sprite('walk', size), left: 0, bottom: 0, animation: `${frames('walk', WALK_MS)}, kwalk_${uid} ${A}` }} />
      <div style={{ ...sprite('kneel', size), left: 0, bottom: 0, animation: `kframes_${uid} ${A}, kkneel_${uid} ${A}` }} />
      <div style={{ ...sprite('archer', size), left: 0, bottom: 0, animation: `wf 950ms steps(9) infinite, karch_${uid} ${A}` }}>
        <div style={{ position: 'absolute', left: w - 6, bottom: size * 0.34, width: 22, height: 8, backgroundImage: 'url(/arrow.png)', backgroundRepeat: 'no-repeat', backgroundSize: '44px 8px', animation: `warrow 950ms linear 500ms infinite` }} />
      </div>
    </div>
  );
}

function Tank({ scale, ground }: { scale: number; ground: number }) {
  const size = Math.round(116 * scale);
  // Holds off-screen-left (base transform) until TANK_DELAY, so the infantry
  // march for a while before the tank rolls in.
  return <div style={{ ...sprite('tank', size), left: 0, bottom: ground, transform: 'translateX(-26vw)', animation: `${frames('tank', 460)}, wtankx ${TANK_MS}ms linear ${TANK_DELAY}ms forwards` }} />;
}

// Two battalions per scene — tight shoulder-to-shoulder clumps with a clear gap
// between them. Each battalion's rear archer kneels to loose a volley (its spot
// + hold come from KNEEL_VARIANTS, so they differ by battalion and by scene).
const COLUMN: { startDelay: number; kneel: boolean; size: number }[] = [
  // Battalion I — a tight clump, rear archer kneels to loose a volley
  { startDelay: 0, kneel: false, size: 94 },
  { startDelay: 480, kneel: false, size: 90 },
  { startDelay: 960, kneel: false, size: 96 },
  { startDelay: 1440, kneel: true, size: 92 },
  // Battalion II — close behind (the march is slow, so a short head start
  // already opens a clear gap on screen between the two battalions)
  { startDelay: 3400, kneel: false, size: 93 },
  { startDelay: 3880, kneel: false, size: 97 },
  { startDelay: 4360, kneel: false, size: 91 },
  { startDelay: 4840, kneel: true, size: 95 },
];

const COLUMN_DONE_MS = Math.max(
  TANK_DELAY + TANK_MS,
  ...COLUMN.map((s) => s.startDelay + (s.kneel ? KNEELER_MS_MAX : CROSS_MS)),
);
// COLUMN_DONE_MS is the moment the LAST unit (rear kneeling archer / tank) has
// fully exited the right edge; we hold just a short beat after that before
// crossfading + filing the next battalion in, so it reads as the column marching
// straight on from one battlefield into the next (never switches mid-screen).
const SCENE_TRANSITION_PAD_MS = 150;

export default function WarScene() {
  const stageRef = useRef<HTMLDivElement>(null);
  const [assembling, setAssembling] = useState(false);
  const [leadGone, setLeadGone] = useState(false);
  const [scene, setScene] = useState(0);
  const [marchKey, setMarchKey] = useState(0);
  const [scale, setScale] = useState(1);

  useEffect(() => {
    const updateScale = () => {
      const rect = stageRef.current?.getBoundingClientRect();
      const stageHeight = rect?.height ?? window.innerHeight * 0.5;
      const next = Math.min(1.35, Math.max(0.72, Math.min(window.innerWidth / 1280, stageHeight / 520)));
      setScale(next);
    };
    updateScale();
    window.addEventListener('resize', updateScale);
    return () => window.removeEventListener('resize', updateScale);
  }, []);

  useEffect(() => {
    // Respect reduced-motion: the CSS already stops the keyframes, so also skip
    // the timed scene/lead state changes and keep the band static.
    if (window.matchMedia('(prefers-reduced-motion: reduce)').matches) return;
    const timers: ReturnType<typeof setTimeout>[] = [];

    const beginMarch = (delay: number) => {
      timers.push(setTimeout(() => setAssembling(true), delay));
    };

    const advanceScene = (delay: number) => {
      timers.push(setTimeout(() => {
        setAssembling(false);
        setScene((s) => (s + 1) % CAPTIONS.length);
        setMarchKey((k) => k + 1);
        beginMarch(80);
        advanceScene(COLUMN_DONE_MS + SCENE_TRANSITION_PAD_MS);
      }, delay));
    };

    beginMarch(ARMY_START);
    const t2 = setTimeout(() => setLeadGone(true), LEAD_GONE);
    timers.push(t2);
    advanceScene(ARMY_START + COLUMN_DONE_MS + SCENE_TRANSITION_PAD_MS);

    return () => timers.forEach(clearTimeout);
  }, []);

  const ground = Math.round(GROUND * scale);
  // a thin ground bar, in the site palette, that the column marches on; the
  // panorama rests its bottom edge on top of it.
  const strip = Math.round(22 * scale);

  return (
    <div ref={stageRef} className="warscene relative size-full overflow-hidden">
      <style>{STYLES}</style>

      {/* hand-drawn backdrop scene — full-bleed width, anchored to the TOP so the
          top of the clouds always shows. height:auto gives it its natural
          width/2.33 height; the band (sized off viewport WIDTH, see page.tsx) is a
          bit shorter, so its overflow crops a slice off the bottom foreground —
          which the ground strip below then caps. A plain block <img> with no
          object-fit / no JS sizing lays out byte-identically in Chrome, Firefox,
          and Safari. */}
      <img
        key={scene}
        src={BACKDROPS[scene]}
        alt=""
        aria-hidden="true"
        style={{
          position: 'absolute',
          left: 0,
          right: 0,
          top: 0,
          width: '100%',
          height: 'auto',
          maxWidth: 'none',
          display: 'block',
          imageRendering: 'auto',
          animation: 'wbg 700ms ease-out both',
        }}
      />

      {/* the thin ground strip the soldiers march on */}
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

      {/* caption */}
      <div className="absolute left-0 top-0 flex justify-start p-3">
        <span key={scene} className="rounded-full border border-[#a98e61]/45 bg-[#d8c39a]/90 px-4 py-1 text-sm font-bold tracking-wide text-ink shadow-sm" style={{ fontFamily: "ui-monospace, 'SFMono-Regular', Menlo, monospace" }}>
          {CAPTIONS[scene]}
        </span>
      </div>

      {assembling && (
        <div key={marchKey} className="absolute inset-0">
          <Tank scale={scale} ground={ground} />
          {COLUMN.map((s, i) => {
            if (!s.kneel) {
              return <Walker key={i} size={Math.round(s.size * scale)} delay={s.startDelay} z={COLUMN.length - i} ground={ground} />;
            }
            // which kneeler this is (0 = battalion I, 1 = battalion II) → its
            // per-scene kneel spot + volley hold
            const ord = COLUMN.slice(0, i).filter((c) => c.kneel).length;
            const variants = KNEEL_VARIANTS[scene] ?? KNEEL_VARIANTS[0];
            const v = variants[ord % variants.length];
            return (
              <Kneeler
                key={i}
                size={Math.round(s.size * scale)}
                delay={s.startDelay}
                z={COLUMN.length - i}
                ground={ground}
                kneelVw={v.vw}
                drawMs={v.draw}
                uid={`${marchKey}_${i}`}
              />
            );
          })}
        </div>
      )}
      {!leadGone && <Lead size={Math.round(LEAD_SIZE * scale)} ground={ground} />}
    </div>
  );
}
