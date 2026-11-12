'use client';

import { type ReactNode, useState } from 'react';

// Logos drawn by code. Each family below is a function from a random number generator to SVG
// shapes in a 120 by 120 box, so one family yields as many variants as there are seeds. Shuffling
// changes the seed. Every number that reaches the markup is rounded, so the server and the browser
// render the same string.

type Rng = () => number;

// mulberry32: a small seeded generator, enough for picking shapes.
function rngFrom(seed: number): Rng {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) | 0;
    let t = Math.imul(a ^ (a >>> 15), 1 | a);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

const f = (x: number) => Math.round(x * 100) / 100;
const between = (r: Rng, a: number, b: number) => a + r() * (b - a);
const int = (r: Rng, a: number, b: number) => Math.floor(between(r, a, b + 1));
function pick<T>(r: Rng, items: readonly T[]): T {
  return items[Math.floor(r() * items.length)] as T;
}
const chance = (r: Rng, p: number) => r() < p;
const times = <T,>(n: number, fn: (i: number) => T) =>
  Array.from({ length: n }, (_, i) => fn(i));

// Pasture for Boundary, Clover for BAML.
const GREEN = ['#E1F3DC', '#82B571', '#4A7A39'];
const PURPLE = ['#F1E9FF', '#CAAAF2', '#9A5BDA'];

function Grad({
  colors,
  id,
  vertical = false,
}: {
  colors: string[];
  id: string;
  vertical?: boolean;
}) {
  return (
    <linearGradient
      gradientUnits="userSpaceOnUse"
      id={id}
      x1={vertical ? 60 : 20}
      x2={vertical ? 60 : 100}
      y1="6"
      y2="114"
    >
      {colors.map((c, i) => (
        <stop key={c} offset={i / (colors.length - 1)} stopColor={c} />
      ))}
    </linearGradient>
  );
}

const SHAPES = ['disc', 'disc', 'squircle', 'hex'] as const;
type ShapeKind = (typeof SHAPES)[number];

function Shape({ kind }: { kind: ShapeKind }) {
  if (kind === 'squircle')
    return <rect height="102" rx="30" width="102" x="9" y="9" />;
  if (kind === 'hex')
    return <polygon points="60,6 106.8,33 106.8,87 60,114 13.2,87 13.2,33" />;
  return <circle cx="60" cy="60" r="52" />;
}

type Family = {
  name: string;
  varies: string;
  draw: (r: Rng, id: string) => ReactNode;
};

/* ---------------------------------- Boundary ---------------------------------- */

const slant: Family = {
  draw: (r, id) => {
    const kind = pick(r, SHAPES);
    const n = int(r, 2, 5);
    const angle = pick(r, [-35, -24, -17, 0, 17, 24, 35, 45]);
    const gap = between(r, 4, 9);
    const fade = chance(r, 0.65);
    const h = (104 - gap * (n - 1)) / n;
    return (
      <>
        <defs>
          <Grad colors={GREEN} id={`${id}g`} />
          <clipPath id={`${id}c`}>
            <Shape kind={kind} />
          </clipPath>
        </defs>
        <g clipPath={`url(#${id}c)`}>
          <g fill={`url(#${id}g)`} transform={`rotate(${angle} 60 60)`}>
            {times(n, (i) => {
              const top = i === 0 ? -60 : 8 + i * (h + gap);
              const bottom = i === n - 1 ? 180 : 8 + i * (h + gap) + h;
              return (
                <rect
                  height={f(bottom - top)}
                  key={i}
                  opacity={fade ? f(1 - (i * 0.7) / (n - 1)) : 1}
                  width="240"
                  x="-60"
                  y={f(top)}
                />
              );
            })}
          </g>
        </g>
      </>
    );
  },
  name: 'Slant',
  varies: 'outline, number of bands, angle, gap, fade',
};

const ringB: Family = {
  draw: (r, id) => {
    const w = between(r, 11, 19);
    const R = between(r, 22, 29);
    const style = pick(r, ['ring', 'ring', 'solid', 'double', 'dot'] as const);
    const cap = pick(r, ['round', 'butt'] as const);
    const sx = 60 - R;
    const cy = 110 - w / 2 - R;
    const top = between(r, 8, 22) + w / 2;
    const ink = `url(#${id}g)`;
    const inner = R - w / 2;
    return (
      <>
        <defs>
          <Grad colors={GREEN} id={`${id}g`} />
        </defs>
        <g fill="none" stroke={ink} strokeLinecap={cap} strokeWidth={f(w)}>
          <path d={`M${f(sx)} ${f(top)}V${f(cy)}`} />
          <circle
            cx={f(sx + R)}
            cy={f(cy)}
            fill={style === 'solid' ? ink : 'none'}
            r={f(R)}
          />
          {style === 'double' && inner > 9 ? (
            <circle
              cx={f(sx + R)}
              cy={f(cy)}
              r={f(inner - 5)}
              strokeWidth="3"
            />
          ) : null}
        </g>
        {style === 'dot' ? (
          <circle cx={f(sx + R)} cy={f(cy)} fill={ink} r={f(inner * 0.38)} />
        ) : null}
      </>
    );
  },
  name: 'Ring b',
  varies: 'weight, ring size, post height, counter',
};

const field: Family = {
  draw: (r, id) => {
    const kind = pick(r, SHAPES);
    const y0 = between(r, 44, 64);
    const q = between(r, 0.62, 0.86);
    let h = between(r, 8, 13);
    let g = between(r, 3, 5);
    const rows: ReactNode[] = [
      <rect height={f(y0)} key="top" width="120" x="0" y="0" />,
    ];
    let y = y0 + g;
    while (y < 112 && h > 1.2) {
      rows.push(<rect height={f(h)} key={y} width="120" x="0" y={f(y)} />);
      y += h + g;
      h *= q;
      g *= 1.12;
    }
    return (
      <>
        <defs>
          <Grad colors={GREEN} id={`${id}g`} vertical />
          <clipPath id={`${id}c`}>
            <Shape kind={kind} />
          </clipPath>
        </defs>
        <g clipPath={`url(#${id}c)`} fill={`url(#${id}g)`}>
          {rows}
        </g>
      </>
    );
  },
  name: 'Field',
  varies: 'outline, horizon height, how fast the rows thin',
};

const bars: Family = {
  draw: (r, id) => {
    const n = int(r, 3, 5);
    const sign = chance(r, 0.5) ? -1 : 1;
    const slope = Math.tan((pick(r, [10, 14, 18, 22]) * Math.PI) / 180);
    const zig = chance(r, 0.5);
    const gap = between(r, 5, 9);
    const fade = chance(r, 0.4);
    const W = 76;
    const maxDy = slope * W;
    const h = (100 - maxDy - (n - 1) * gap) / n;
    return (
      <>
        <defs>
          <Grad colors={GREEN} id={`${id}g`} />
        </defs>
        <g fill={`url(#${id}g)`}>
          {times(n, (i) => {
            const wide = i % 2 === 0 ? 1 : between(r, 0.45, 0.75);
            const s = slope * sign * (zig && i % 2 === 1 ? -1 : 1);
            const x0 = 22;
            const x1 = 22 + W * wide;
            const yL = 10 + i * (h + gap) + (s < 0 ? maxDy : 0);
            const yR = yL + s * (x1 - x0);
            return (
              <polygon
                key={i}
                opacity={fade ? f(1 - (i * 0.6) / (n - 1)) : 1}
                points={`${x0},${f(yL)} ${f(x1)},${f(yR)} ${f(x1)},${f(yR + h)} ${x0},${f(yL + h)}`}
              />
            );
          })}
        </g>
      </>
    );
  },
  name: 'Bars',
  varies: 'number of bars, slope, straight or folded, short bars',
};

const frames: Family = {
  draw: (r, id) => {
    const n = int(r, 2, 4);
    const w = between(r, 5, 9);
    const step = w + between(r, 5, 8);
    const turned = chance(r, 0.3);
    const round = between(r, 0.15, 0.5);
    const ink = `url(#${id}g)`;
    const shapes: ReactNode[] = [];
    let last = 0;
    for (let i = 0; i < n; i += 1) {
      const half = 50 - w / 2 - i * step;
      if (half < 7) break;
      last = half;
      const open = between(r, 8, 22);
      shapes.push(
        <rect
          height={f(half * 2)}
          key={i}
          pathLength="100"
          rx={f(half * round)}
          strokeDasharray={`${f(100 - open)} ${f(open)}`}
          strokeDashoffset={f(between(r, 0, 100))}
          width={f(half * 2)}
          x={f(60 - half)}
          y={f(60 - half)}
        />,
      );
    }
    const dot = last - w / 2 > 9 && chance(r, 0.6);
    return (
      <>
        <defs>
          <Grad colors={GREEN} id={`${id}g`} />
        </defs>
        <g
          transform={
            turned
              ? 'translate(60 60) rotate(45) scale(0.74) translate(-60 -60)'
              : undefined
          }
        >
          <g fill="none" stroke={ink} strokeLinecap="round" strokeWidth={f(w)}>
            {shapes}
          </g>
          {dot ? (
            <circle cx="60" cy="60" fill={ink} r={f(between(r, 3.5, 6))} />
          ) : null}
        </g>
      </>
    );
  },
  name: 'Frames',
  varies: 'number of frames, corner radius, where each opens, rotation',
};

const dotGrid: Family = {
  draw: (r, id) => {
    const n = int(r, 4, 6);
    const cell = 96 / n;
    const a = between(r, 0, Math.PI);
    const t = between(r, -0.15, 0.15);
    const square = chance(r, 0.35);
    return (
      <>
        <defs>
          <Grad colors={GREEN} id={`${id}g`} />
        </defs>
        <g fill={`url(#${id}g)`}>
          {times(n * n, (k) => {
            const cx = 12 + cell * ((k % n) + 0.5);
            const cy = 12 + cell * (Math.floor(k / n) + 0.5);
            const side =
              ((cx - 60) * Math.cos(a) + (cy - 60) * Math.sin(a)) / 60;
            const rad = side > t ? cell * 0.38 : cell * 0.13;
            return square ? (
              <rect
                height={f(rad * 2)}
                key={k}
                rx={f(rad * 0.3)}
                width={f(rad * 2)}
                x={f(cx - rad)}
                y={f(cy - rad)}
              />
            ) : (
              <circle cx={f(cx)} cy={f(cy)} key={k} r={f(rad)} />
            );
          })}
        </g>
      </>
    );
  },
  name: 'Dot grid',
  varies: 'grid size, where the line falls, dots or squares',
};

/* ------------------------------------ BAML ------------------------------------ */

// A spiral of half circles along one horizontal line, each smaller than the last.
function spiral(x0: number, cy: number, R0: number, step: number) {
  let d = `M${f(x0)} ${f(cy)}`;
  let x = x0;
  let dir = 1;
  for (let R = R0; R >= 4; R -= step) {
    x += dir * 2 * R;
    d += `A${f(R)} ${f(R)} 0 0 0 ${f(x)} ${f(cy)}`;
    dir = -dir;
  }
  return d;
}

const horn: Family = {
  draw: (r, id) => {
    const w = between(r, 7, 11);
    const step = (w + between(r, 3.5, 6)) / 2;
    const R0 = between(r, 27, 34);
    const stem = chance(r, 0.75);
    const x0 = 60 - R0;
    const cy = stem ? 112 - w / 2 - R0 : 60 - step / 2;
    const top = w / 2 + between(r, 5, 14);
    return (
      <>
        <defs>
          <Grad colors={PURPLE} id={`${id}g`} />
        </defs>
        <path
          d={`${stem ? `M${f(x0)} ${f(top)}V${f(cy)}` : ''}${spiral(x0, cy, R0, step)}`}
          fill="none"
          stroke={`url(#${id}g)`}
          strokeLinecap="round"
          strokeWidth={f(w)}
        />
      </>
    );
  },
  name: 'Horn',
  varies: 'weight, how tightly it winds, with or without the stem',
};

const petals: Family = {
  draw: (r, id) => {
    const n = int(r, 3, 8);
    const pr = between(r, 18, 26);
    const d = Math.min(52 - pr, pr * between(r, 0.8, 1.15));
    const turn = chance(r, 0.5) ? Math.PI / n : 0;
    return (
      <>
        <defs>
          <Grad colors={PURPLE} id={`${id}g`} />
        </defs>
        <g fill={`url(#${id}g)`} fillOpacity={n > 5 ? 0.58 : 0.72}>
          {times(n, (i) => {
            const a = turn + (i / n) * Math.PI * 2;
            return (
              <circle
                cx={f(60 + d * Math.cos(a))}
                cy={f(60 + d * Math.sin(a))}
                key={i}
                r={f(pr)}
              />
            );
          })}
        </g>
      </>
    );
  },
  name: 'Clover',
  varies: 'number of leaves, leaf size, overlap, rotation',
};

const lambHead: Family = {
  draw: (r, id) => {
    const n = int(r, 3, 5);
    const fw = between(r, 19, 25);
    const earTilt = between(r, -5, 35);
    const earLen = between(r, 12, 17);
    const wool = times(n, (i) => {
      const t = i / (n - 1);
      const lift = Math.sin(t * Math.PI);
      return (
        <circle
          cx={f(60 + (t - 0.5) * 2 * (fw + 4))}
          cy={f(44 - lift * 10)}
          key={i}
          r={f(13 + lift * 5)}
        />
      );
    });
    const earX = fw + earLen - 3;
    return (
      <>
        <defs>
          <Grad
            colors={[PURPLE[1] as string, PURPLE[2] as string]}
            id={`${id}g`}
            vertical
          />
          <mask id={`${id}m`}>
            <rect fill="#fff" height="120" width="120" />
            <g fill="#000" stroke="#000" strokeWidth="6">
              {wool}
            </g>
            <circle cx={f(60 - fw * 0.45)} cy="76" fill="#000" r="4" />
            <circle cx={f(60 + fw * 0.45)} cy="76" fill="#000" r="4" />
          </mask>
        </defs>
        <g fill={`url(#${id}g)`} mask={`url(#${id}m)`}>
          <ellipse
            cx={f(60 - earX)}
            cy="64"
            rx={f(earLen)}
            ry={f(earLen * 0.5)}
            transform={`rotate(${f(earTilt)} ${f(60 - earX)} 64)`}
          />
          <ellipse
            cx={f(60 + earX)}
            cy="64"
            rx={f(earLen)}
            ry={f(earLen * 0.5)}
            transform={`rotate(${f(-earTilt)} ${f(60 + earX)} 64)`}
          />
          <path
            d={`M${f(60 - fw)} 50H${f(60 + fw)}V84A${f(fw)} ${f(fw)} 0 0 1 ${f(60 - fw)} 84Z`}
          />
        </g>
        <g fill={PURPLE[0]}>{wool}</g>
      </>
    );
  },
  name: 'Lamb head',
  varies: 'tufts of wool, face width, ear length and tilt',
};

const promptB: Family = {
  draw: (r, id) => {
    const R = between(r, 33, 40);
    const sw = between(r, 20, 26);
    const cx = 26 + R;
    const cy = 112 - R;
    const glyph = pick(r, [
      'chevron',
      'cursor',
      'dot',
      'slash',
      'line',
    ] as const);
    const cut = {
      fill: 'none',
      stroke: '#000',
      strokeLinecap: 'round',
      strokeLinejoin: 'round',
      strokeWidth: 11,
    } as const;
    return (
      <>
        <defs>
          <Grad colors={PURPLE} id={`${id}g`} />
          <mask id={`${id}m`}>
            <rect fill="#fff" height="120" width="120" />
            {glyph === 'chevron' ? (
              <path
                d={`M${f(cx - 10)} ${f(cy - 18)}L${f(cx + 10)} ${f(cy)}L${f(cx - 10)} ${f(cy + 18)}`}
                {...cut}
              />
            ) : null}
            {glyph === 'cursor' ? (
              <rect
                fill="#000"
                height="34"
                rx="2"
                width="15"
                x={f(cx - 6)}
                y={f(cy - 17)}
              />
            ) : null}
            {glyph === 'dot' ? (
              <circle cx={f(cx + 2)} cy={f(cy)} fill="#000" r={f(R * 0.3)} />
            ) : null}
            {glyph === 'slash' ? (
              <path
                d={`M${f(cx - 8)} ${f(cy + 18)}L${f(cx + 10)} ${f(cy - 18)}`}
                {...cut}
              />
            ) : null}
            {glyph === 'line' ? (
              <path d={`M${f(cx - 12)} ${f(cy + 12)}H${f(cx + 14)}`} {...cut} />
            ) : null}
          </mask>
        </defs>
        <g fill={`url(#${id}g)`} mask={`url(#${id}m)`}>
          <rect height="104" rx="5" width={f(sw)} x="16" y="8" />
          <circle cx={f(cx)} cy={f(cy)} r={f(R)} />
        </g>
      </>
    );
  },
  name: 'Prompt b',
  varies: 'bowl size, stem width, what is cut out of the bowl',
};

/* ---------------------------------- Combined ---------------------------------- */

const leap: Family = {
  draw: (r, id) => {
    const posts = pick(r, [1, 1, 2, 3]);
    const pw = between(r, 9, 13);
    const ph = posts > 1 ? between(r, 30, 44) : between(r, 34, 56);
    const spacing = pw + between(r, 8, 12);
    const w = between(r, 7, 10);
    const apex = 112 - ph - between(r, 16, 26);
    const cy = 2 * apex - 104;
    const t = between(r, 0.62, 0.88);
    // Split the curve from (12, 104) over (60, cy) to (108, 104) at t.
    const q0 = [12 + 48 * t, 104 + (cy - 104) * t];
    const q1 = [60 + 48 * t, cy + (104 - cy) * t];
    const head = [
      (q0[0] as number) + ((q1[0] as number) - (q0[0] as number)) * t,
      (q0[1] as number) + ((q1[1] as number) - (q0[1] as number)) * t,
    ];
    return (
      <>
        <defs>
          <linearGradient
            gradientUnits="userSpaceOnUse"
            id={`${id}a`}
            x1="12"
            x2={f(head[0] as number)}
            y1="104"
            y2={f(head[1] as number)}
          >
            <stop offset="0" stopColor={PURPLE[2]} stopOpacity="0.1" />
            <stop offset="1" stopColor={PURPLE[1]} />
          </linearGradient>
          <Grad
            colors={[GREEN[1] as string, GREEN[2] as string]}
            id={`${id}g`}
            vertical
          />
        </defs>
        <path
          d={`M12 104Q${f(q0[0] as number)} ${f(q0[1] as number)} ${f(head[0] as number)} ${f(head[1] as number)}`}
          fill="none"
          stroke={`url(#${id}a)`}
          strokeLinecap="round"
          strokeWidth={f(w)}
        />
        <circle
          cx={f(head[0] as number)}
          cy={f(head[1] as number)}
          fill={PURPLE[0]}
          r={f(w * 1.15)}
        />
        <g fill={`url(#${id}g)`}>
          {times(posts, (i) => (
            <rect
              height={f(ph)}
              key={i}
              rx={f(pw / 2)}
              width={f(pw)}
              x={f(60 + (i - (posts - 1) / 2) * spacing - pw / 2)}
              y={f(112 - ph)}
            />
          ))}
        </g>
      </>
    );
  },
  name: 'Leap',
  varies: 'number of posts, height of the jump, how far along the lamb is',
};

const split: Family = {
  draw: (r, id) => {
    const kind = pick(r, SHAPES);
    const angle = pick(r, [-35, -17, 0, 17, 35, 45, 90]);
    const n = pick(r, [2, 2, 3]);
    const gap = between(r, 5, 9);
    const first = chance(r, 0.5) ? 0 : 1;
    const h = (104 - gap * (n - 1)) / n;
    return (
      <>
        <defs>
          <Grad colors={GREEN} id={`${id}g`} />
          <Grad
            colors={[PURPLE[1] as string, PURPLE[2] as string]}
            id={`${id}p`}
          />
          <clipPath id={`${id}c`}>
            <Shape kind={kind} />
          </clipPath>
        </defs>
        <g clipPath={`url(#${id}c)`}>
          <g transform={`rotate(${angle} 60 60)`}>
            {times(n, (i) => {
              const top = i === 0 ? -60 : 8 + i * (h + gap);
              const bottom = i === n - 1 ? 180 : 8 + i * (h + gap) + h;
              return (
                <rect
                  fill={`url(#${id}${(i + first) % 2 === 0 ? 'g' : 'p'})`}
                  height={f(bottom - top)}
                  key={i}
                  width="240"
                  x="-60"
                  y={f(top)}
                />
              );
            })}
          </g>
        </g>
      </>
    );
  },
  name: 'Split',
  varies: 'outline, angle of the cut, two or three parts',
};

const doubleB: Family = {
  draw: (r, id) => {
    const R = between(r, 17, 23);
    const w = between(r, 8, 12);
    const dx = between(r, 22, 32);
    const dy = between(r, 10, 22);
    const stem = between(r, 46, 56);
    const sx = 60 - (2 * R + dx) / 2;
    const ty = 60 - (stem + R + dy) / 2;
    const b = (x: number, y: number, ink: string) => (
      <g stroke={ink}>
        <path d={`M${f(x)} ${f(y)}V${f(y + stem)}`} />
        <circle cx={f(x + R)} cy={f(y + stem)} r={f(R)} />
      </g>
    );
    return (
      <>
        <defs>
          <Grad colors={GREEN} id={`${id}g`} />
          <Grad colors={PURPLE} id={`${id}p`} />
        </defs>
        <g fill="none" strokeLinecap="round" strokeWidth={f(w)}>
          {b(sx, ty, `url(#${id}g)`)}
          {b(sx + dx, ty + dy, `url(#${id}p)`)}
        </g>
      </>
    );
  },
  name: 'Double b',
  varies: 'ring size, weight, how far the second b is offset',
};

const orbit: Family = {
  draw: (r, id) => {
    const n = int(r, 1, 3);
    const w = between(r, 5, 9);
    const step = w + between(r, 6, 9);
    const rings: ReactNode[] = [];
    let dot: ReactNode = null;
    for (let i = 0; i < n; i += 1) {
      const R = 46 - i * step;
      // Each ring is an arc that leaves an opening centered on `mid`.
      const open = between(r, 0.4, 1) * Math.PI * 0.5;
      const mid = between(r, 0, Math.PI * 2);
      const from = mid + open / 2;
      const to = mid - open / 2;
      rings.push(
        <path
          d={`M${f(60 + R * Math.cos(from))} ${f(60 + R * Math.sin(from))}A${f(R)} ${f(R)} 0 1 1 ${f(60 + R * Math.cos(to))} ${f(60 + R * Math.sin(to))}`}
          key={i}
        />,
      );
      if (i === 0) {
        // The lamb's dot sits in the middle of the outer ring's opening.
        dot = (
          <circle
            cx={f(60 + R * Math.cos(mid))}
            cy={f(60 + R * Math.sin(mid))}
            fill={`url(#${id}p)`}
            r={f(between(r, 6, 9))}
          />
        );
      }
    }
    return (
      <>
        <defs>
          <Grad colors={GREEN} id={`${id}g`} />
          <Grad
            colors={[PURPLE[0] as string, PURPLE[1] as string]}
            id={`${id}p`}
          />
        </defs>
        <g
          fill="none"
          stroke={`url(#${id}g)`}
          strokeLinecap="round"
          strokeWidth={f(w)}
        >
          {rings}
        </g>
        {dot}
      </>
    );
  },
  name: 'Orbit',
  varies: 'number of rings, where each opens, size of the dot',
};

const fence: Family = {
  draw: (r, id) => {
    const n = int(r, 4, 7);
    const pw = between(r, 7, 11);
    const gap = Math.min(between(r, 5, 9), (96 - n * pw) / (n - 1));
    const total = n * pw + (n - 1) * gap;
    const mode = pick(r, ['arc', 'rise', 'wave', 'dip'] as const);
    const lamb = int(r, 0, n - 1);
    const phase = between(r, 0, Math.PI);
    return (
      <>
        <defs>
          <Grad colors={GREEN} id={`${id}g`} vertical />
          <Grad
            colors={[PURPLE[0] as string, PURPLE[2] as string]}
            id={`${id}p`}
            vertical
          />
        </defs>
        {times(n, (i) => {
          const t = i / (n - 1);
          const level =
            mode === 'arc'
              ? Math.sin(t * Math.PI)
              : mode === 'rise'
                ? t
                : mode === 'dip'
                  ? 1 - Math.sin(t * Math.PI)
                  : 0.5 + 0.5 * Math.sin(t * Math.PI * 2 + phase);
          const h = 34 + level * 62;
          return (
            <rect
              fill={`url(#${id}${i === lamb ? 'p' : 'g'})`}
              height={f(h)}
              key={i}
              rx={f(pw / 2)}
              width={f(pw)}
              x={f(60 - total / 2 + i * (pw + gap))}
              y={f(108 - h)}
            />
          );
        })}
      </>
    );
  },
  name: 'Fence',
  varies: 'number of posts, the line their tops follow, which post is the lamb',
};

const GROUPS: { title: string; families: Family[] }[] = [
  { families: [slant, ringB, field, bars, frames, dotGrid], title: 'Boundary' },
  { families: [horn, petals, lambHead, promptB], title: 'BAML' },
  {
    families: [leap, split, doubleB, orbit, fence],
    title: 'Boundary and BAML',
  },
];

const PER_FAMILY = 8;

function Tile({ children }: { children: ReactNode }) {
  return (
    <div
      className="flex aspect-square items-center justify-center rounded-[22%]"
      style={{
        background: 'linear-gradient(160deg, #26262A 0%, #0C0C0E 70%)',
        boxShadow:
          'inset 0 1px 0 rgb(255 255 255 / 0.16), inset 0 0 0 1px rgb(255 255 255 / 0.06)',
      }}
    >
      {children}
    </div>
  );
}

export function Generated() {
  const [seed, setSeed] = useState(1);
  let familyIndex = 0;
  return (
    <div>
      <div className="flex flex-wrap items-center gap-4">
        <button
          className="rounded-full bg-white px-5 py-2.5 font-medium text-black text-sm transition-colors hover:bg-white/85 focus-visible:outline-2 focus-visible:outline-white focus-visible:outline-offset-2"
          onClick={() => setSeed((s) => s + 1)}
          type="button"
        >
          Shuffle all
        </button>
        {seed > 1 ? (
          <button
            className="rounded-full border border-white/25 px-5 py-2.5 text-sm text-white transition-colors hover:border-white/60 focus-visible:outline-2 focus-visible:outline-white focus-visible:outline-offset-2"
            onClick={() => setSeed((s) => s - 1)}
            type="button"
          >
            Previous set
          </button>
        ) : null}
        <span
          className="text-sm text-white/45"
          style={{ fontFamily: 'var(--font-geist-pixel)' }}
        >
          set {seed}
        </span>
      </div>

      {GROUPS.map((group) => (
        <div className="mt-14" key={group.title}>
          <h3
            className="font-medium text-2xl text-white"
            style={{ fontFamily: 'var(--font-lexend)' }}
          >
            {group.title}
          </h3>
          {group.families.map((family) => {
            familyIndex += 1;
            const base = seed * 1000 + familyIndex * 37;
            return (
              <div className="mt-7" key={family.name}>
                <p className="text-base text-white">
                  {family.name}
                  <span className="ml-3 text-sm text-white/45">
                    varies: {family.varies}
                  </span>
                </p>
                <div className="mt-3 grid grid-cols-4 gap-3 sm:grid-cols-8">
                  {times(PER_FAMILY, (i) => {
                    const id = `gen-${familyIndex}-${i}`;
                    return (
                      <Tile key={`${seed}-${i}`}>
                        <svg
                          className="size-[62%]"
                          role="img"
                          viewBox="0 0 120 120"
                        >
                          <title>{`${family.name} ${i + 1}`}</title>
                          {family.draw(rngFrom(base + i * 7919), id)}
                        </svg>
                      </Tile>
                    );
                  })}
                </div>
              </div>
            );
          })}
        </div>
      ))}
    </div>
  );
}
