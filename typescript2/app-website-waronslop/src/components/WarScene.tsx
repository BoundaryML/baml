'use client';

import { CSSProperties, useEffect, useState } from 'react';

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
// a distinct skyline of Roman temples per battle (they sit on the walking line)
const SKYLINES = [
  // I — Design: a temple flanked by smaller ones
  [
    { src: '/temple_b.png', w: 92, h: 88, left: '2%', size: 124 },
    { src: '/temple_a.png', w: 130, h: 96, left: '33%', size: 152 },
    { src: '/temple_b.png', w: 92, h: 88, left: '65%', size: 118 },
    { src: '/temple_a.png', w: 130, h: 96, left: '83%', size: 140 },
  ],
  // II — Architecture: grand, wide colonnades
  [
    { src: '/temple_c.png', w: 168, h: 104, left: '-5%', size: 176 },
    { src: '/temple_b.png', w: 92, h: 88, left: '40%', size: 122 },
    { src: '/temple_c.png', w: 168, h: 104, left: '66%', size: 164 },
  ],
  // III — Deployment: a varied row
  [
    { src: '/temple_a.png', w: 130, h: 96, left: '1%', size: 134 },
    { src: '/temple_c.png', w: 168, h: 104, left: '27%', size: 150 },
    { src: '/temple_b.png', w: 92, h: 88, left: '63%', size: 116 },
    { src: '/temple_a.png', w: 130, h: 96, left: '80%', size: 146 },
  ],
];

const SCENE_MS = 9000;
const SCENE_START = 12000; // start cycling captions once the march is rolling
const GROUND = 8;
const WALK_MS = 480;       // one stride of the leg cycle
const SPEED = 7;           // vw per second — slow, deliberate
const START_VW = -22;      // enters from off the left edge
const END_VW = 122;        // exits off the right edge
const KNEEL_VW = 50;       // midpoint of START..END — kneel dead-centre
const LEAD_CENTER = 47.5;  // where the lead pauses to wave

// One kneeler cycle, in ms: walk in → kneel down → loose → stand up → walk on.
const KN = { walkIn: 10300, kneelDown: 800, draw: 2800, standUp: 800, walkOut: 10300 };
const KNEELER_MS = KN.walkIn + KN.kneelDown + KN.draw + KN.standUp + KN.walkOut;
const kp1 = (100 * KN.walkIn) / KNEELER_MS;
const kp2 = (100 * (KN.walkIn + KN.kneelDown)) / KNEELER_MS;
const kp3 = (100 * (KN.walkIn + KN.kneelDown + KN.draw)) / KNEELER_MS;
const kp4 = (100 * (KN.walkIn + KN.kneelDown + KN.draw + KN.standUp)) / KNEELER_MS;

// The lead is a one-shot intro: walk in → wave → walk on at the head of the column.
const LD = { walkIn: 10000, wave: 2000, walkOut: 10600 };
const LEAD_MS = LD.walkIn + LD.wave + LD.walkOut;
const lp1 = (100 * LD.walkIn) / LEAD_MS;
const lp2 = (100 * (LD.walkIn + LD.wave)) / LEAD_MS;

const ARMY_START = LD.walkIn + LD.wave; // soldiers appear once the lead marches on
const LEAD_GONE = LEAD_MS + 400;        // lead has fully exited the right edge
const LEAD_SIZE = 94;                   // same scale as the rank and file

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
  @keyframes wfade  { from { opacity: 0; } to { opacity: 0.5; } }

  /* kneeler: parent travels (holding still mid-screen); child sheets cross-fade */
  @keyframes kmove {
    0% { transform: translateX(${START_VW}vw); }
    ${pc(kp1)} { transform: translateX(${KNEEL_VW}vw); }
    ${pc(kp4)} { transform: translateX(${KNEEL_VW}vw); }
    100% { transform: translateX(${END_VW}vw); }
  }
  @keyframes kwalk  { 0%,${pc(kp1)} { opacity: 1; } ${pc(kp1 + 0.05)},${pc(kp4 - 0.05)} { opacity: 0; } ${pc(kp4)},100% { opacity: 1; } }
  @keyframes kkneel { 0%,${pc(kp1 - 0.05)} { opacity: 0; } ${pc(kp1)},${pc(kp2)} { opacity: 1; } ${pc(kp2 + 0.05)},${pc(kp3 - 0.05)} { opacity: 0; } ${pc(kp3)},${pc(kp4)} { opacity: 1; } ${pc(kp4 + 0.05)},100% { opacity: 0; } }
  @keyframes karch  { 0%,${pc(kp2 - 0.05)} { opacity: 0; } ${pc(kp2)},${pc(kp3)} { opacity: 1; } ${pc(kp3 + 0.05)},100% { opacity: 0; } }
  /* kneel sheet position: hold standing, step down (kp1→kp2), hold kneeled,
     step up (kp3→kp4), hold standing — ends held on the real last frame */
  @keyframes kframes {
    0% { background-position: 0 0; }
    ${pc(kp1)} { background-position: 0 0; animation-timing-function: steps(${F.kneel.n}, jump-none); }
    ${pc(kp2)} { background-position: var(--k7) 0; }
    ${pc(kp3)} { background-position: var(--k7) 0; animation-timing-function: steps(${F.kneel.n}, jump-none); }
    ${pc(kp4)} { background-position: 0 0; }
    100% { background-position: 0 0; }
  }

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

// ── lead Spartan: marches in, waves, then marches on at the head of the column
function Lead({ size }: { size: number }) {
  const w = widthOf('walk', size);
  const fw = widthOf('front', size);
  return (
    <div style={{ position: 'absolute', left: 0, bottom: GROUND, width: w, height: size, animation: `lmove ${LEAD_MS}ms linear forwards` }}>
      <div style={{ ...sprite('walk', size), left: 0, bottom: 0, animation: `${frames('walk', WALK_MS)}, lwalk ${LEAD_MS}ms linear forwards` }} />
      <div style={{ ...sprite('front', size), left: 0, bottom: 0, ['--fw' as string]: `${fw}px`, animation: `wwave 480ms steps(2) infinite, lfront ${LEAD_MS}ms linear forwards` }} />
    </div>
  );
}

// ── a soldier who just walks across (enters left, exits right) ──────────────
function Walker({ size, delay, z }: { size: number; delay: number; z: number }) {
  const cross = ms(START_VW, END_VW);
  // Base transform parks the soldier off the left edge during its stagger delay
  // (matching wmarch's `from`), so it isn't stuck at translateX(0) — and it loops
  // back there seamlessly.
  return (
    <div style={{ ...sprite('walk', size), left: 0, bottom: GROUND, zIndex: z, transform: `translateX(${START_VW}vw)`, animation: `${frames('walk', WALK_MS)}, wmarch ${cross}ms linear ${delay}ms infinite` }} />
  );
}

// ── a soldier that walks in, kneels dead-centre to loose a volley, walks on ──
// Pure CSS: parent moves+holds via `kmove`; three stacked sheets (walk / kneel
// / archer) cross-fade by phase, all sharing the same period + delay.
function Kneeler({ size, delay, z }: { size: number; delay: number; z: number }) {
  const w = widthOf('kneel', size);
  const k7 = `-${7 * w}px`;
  const A = `${KNEELER_MS}ms linear ${delay}ms infinite`;
  return (
    <div style={{ position: 'absolute', left: 0, bottom: GROUND, width: w, height: size, zIndex: z, transform: `translateX(${START_VW}vw)`, animation: `kmove ${A}` }}>
      <div style={{ ...sprite('walk', size), left: 0, bottom: 0, animation: `${frames('walk', WALK_MS)}, kwalk ${A}` }} />
      <div style={{ ...sprite('kneel', size), left: 0, bottom: 0, ['--k7' as string]: k7, animation: `kframes ${A}, kkneel ${A}` }} />
      <div style={{ ...sprite('archer', size), left: 0, bottom: 0, animation: `wf 950ms steps(9) infinite, karch ${A}` }}>
        <div style={{ position: 'absolute', left: w - 6, bottom: size * 0.34, width: 22, height: 8, backgroundImage: 'url(/arrow.png)', backgroundRepeat: 'no-repeat', backgroundSize: '44px 8px', animation: `warrow 950ms linear 500ms infinite` }} />
      </div>
    </div>
  );
}

function Tank() {
  const size = 116;
  return <div style={{ ...sprite('tank', size), left: 0, bottom: GROUND, transform: 'translateX(-26vw)', animation: `${frames('tank', 460)}, wtankx 20000ms linear infinite` }} />;
}

// Legionaries march in tight batches (shoulder-to-shoulder clumps) with clear
// gaps between them. Three batches, spaced ~evenly across the crossing time so
// they keep coming in waves. A rear archer in some batches kneels to loose.
const COLUMN: { startDelay: number; kneel: boolean; size: number }[] = [
  // batch I
  { startDelay: 0, kneel: false, size: 94 },
  { startDelay: 480, kneel: false, size: 90 },
  { startDelay: 960, kneel: false, size: 96 },
  { startDelay: 1440, kneel: true, size: 92 },
  // batch II
  { startDelay: 6850, kneel: false, size: 93 },
  { startDelay: 7330, kneel: false, size: 97 },
  { startDelay: 7810, kneel: false, size: 91 },
  { startDelay: 8290, kneel: false, size: 95 },
  // batch III
  { startDelay: 13700, kneel: false, size: 95 },
  { startDelay: 14180, kneel: false, size: 90 },
  { startDelay: 14660, kneel: false, size: 96 },
  { startDelay: 15140, kneel: true, size: 92 },
];

export default function WarScene() {
  const [assembling, setAssembling] = useState(false);
  const [leadGone, setLeadGone] = useState(false);
  const [scene, setScene] = useState(0);
  useEffect(() => {
    // Respect reduced-motion: the CSS already stops the keyframes, so also skip
    // the timed scene/lead state changes and keep the band static.
    if (window.matchMedia('(prefers-reduced-motion: reduce)').matches) return;
    const t1 = setTimeout(() => setAssembling(true), ARMY_START);
    const t2 = setTimeout(() => setLeadGone(true), LEAD_GONE);
    let id: ReturnType<typeof setInterval>;
    const t3 = setTimeout(() => {
      id = setInterval(() => setScene((s) => (s + 1) % CAPTIONS.length), SCENE_MS);
    }, SCENE_START);
    return () => { clearTimeout(t1); clearTimeout(t2); clearTimeout(t3); if (id) clearInterval(id); };
  }, []);

  return (
    <div className="warscene relative size-full overflow-hidden">
      <style>{STYLES}</style>

      {/* Roman temple skyline — a different arrangement per battle, sitting on
          the walking line, faint behind the action */}
      <div key={scene} style={{ position: 'absolute', inset: 0, animation: 'wfade 700ms ease-out both' }}>
        {SKYLINES[scene].map((tp, i) => {
          const w = Math.round((tp.size * tp.w) / tp.h);
          return (
            <div key={i} style={{ position: 'absolute', left: tp.left, bottom: GROUND, width: w, height: tp.size, backgroundImage: `url(${tp.src})`, backgroundRepeat: 'no-repeat', backgroundSize: `${w}px ${tp.size}px` }} />
          );
        })}
      </div>

      {/* caption */}
      <div className="absolute inset-x-0 top-0 flex justify-center pt-3">
        <span key={scene} className="rounded-full bg-black/80 px-4 py-1 text-sm font-bold tracking-wide text-white" style={{ fontFamily: "ui-monospace, 'SFMono-Regular', Menlo, monospace" }}>
          {CAPTIONS[scene]}
        </span>
      </div>

      {assembling && (
        <>
          <Tank />
          {COLUMN.map((s, i) =>
            s.kneel ? (
              <Kneeler key={i} size={s.size} delay={s.startDelay} z={COLUMN.length - i} />
            ) : (
              <Walker key={i} size={s.size} delay={s.startDelay} z={COLUMN.length - i} />
            ),
          )}
        </>
      )}
      {!leadGone && <Lead size={LEAD_SIZE} />}
    </div>
  );
}
