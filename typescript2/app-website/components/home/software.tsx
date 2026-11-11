'use client';

import {
  Code,
  Cog,
  Database,
  Handshake,
  Headset,
  type LucideIcon,
  Megaphone,
} from 'lucide-react';
import { useLayoutEffect, useRef, useState } from 'react';
import { section } from '@/components/home/analytics';
import { BoundaryMark } from '@/components/home/boundary-logo';
import { MarkTile } from '@/components/home/mark';
import { MARKS, type Mark } from '@/components/home/marks';
import { Looped } from '@/components/home/pinned';
import { Toggle } from '@/components/home/stack';

// Every team ships software now, and each sends its data somewhere different, or nowhere. Drawn as the
// stack section's toggle: in the traditional stack each file feeds its own mix of tools; in the
// Boundary half, files turn to BAML one by one and get hyperobservability, and the rest reaches the
// same place over OpenTelemetry.

type Store = 'langsmith' | 'datadog' | 'sentry' | 's3' | 'nowhere';

const STORES: { key: Store; mark?: Mark; name: string }[] = [
  { key: 'langsmith', mark: MARKS.langsmith, name: 'LangSmith' },
  { key: 'datadog', mark: MARKS.datadog, name: 'Datadog' },
  { key: 'sentry', mark: MARKS.sentry, name: 'Sentry' },
  { key: 's3', name: 'S3 bucket' },
  { key: 'nowhere', name: 'nowhere' },
];

/** Every team ships code now, not just engineering. */
const TEAMS = [
  { icon: Headset, label: 'Support', name: 'support' },
  { icon: Cog, label: 'Ops', name: 'ops' },
  { icon: Code, label: 'Engineering', name: 'eng' },
  { icon: Database, label: 'Data', name: 'data' },
  { icon: Handshake, label: 'Sales', name: 'sales' },
  { icon: Megaphone, label: 'Marketing', name: 'marketing' },
] as const satisfies readonly {
  icon: LucideIcon;
  label: string;
  name: string;
}[];

type Team = (typeof TEAMS)[number]['name'];

/** A team's name with its icon, so the row reads as people, not folders. */
function TeamName({ t }: { t: (typeof TEAMS)[number] }) {
  const Icon = t.icon;
  return (
    <span className="flex items-center gap-1.5 font-display text-[13px] font-medium text-head">
      <span className="grid h-5 w-5 shrink-0 place-items-center rounded-md bg-white/[0.07]">
        <Icon className="h-3 w-3 text-white/75" strokeWidth={2} />
      </span>
      {t.label}
    </span>
  );
}

/** A team's code. On the Boundary side `upgrade` is the file it becomes in BAML, and `fn` is the one
 *  function in it that moves to BAML while the rest stays put. `at` is when, in the move across, it
 *  happens: files don't all switch at once. */
const FILES: {
  at: number;
  baml?: boolean;
  fn?: string;
  name: string;
  team: Team;
  to: Store[];
  upgrade?: string;
}[] = [
  {
    at: 0.24,
    baml: true,
    name: 'routing.baml',
    team: 'support',
    to: ['langsmith'],
  },
  {
    at: 0.4,
    name: 'router.py',
    team: 'support',
    to: ['nowhere'],
    upgrade: 'router.baml',
  },
  {
    at: 0.3,
    baml: true,
    name: 'extract.baml',
    team: 'ops',
    to: ['datadog', 'sentry'],
  },
  {
    at: 0.56,
    name: 'agent.ts',
    team: 'eng',
    to: ['langsmith', 'nowhere'],
    upgrade: 'agent.baml',
  },
  {
    at: 0.48,
    fn: 'IsFraud()',
    name: 'checkout.go',
    team: 'eng',
    to: ['datadog', 'sentry'],
  },
  {
    at: 0.72,
    fn: 'Dedupe()',
    name: 'etl.py',
    team: 'data',
    to: ['s3', 'nowhere'],
  },
  {
    at: 0.64,
    name: 'leads.py',
    team: 'sales',
    to: ['nowhere'],
    upgrade: 'leads.baml',
  },
  {
    at: 0.8,
    name: 'draft.ipynb',
    team: 'marketing',
    to: ['nowhere'],
    upgrade: 'draft.baml',
  },
];

const ofTeam = (team: Team) => FILES.filter((f) => f.team === team);

const hyper = (f: (typeof FILES)[number]) =>
  f.baml || Boolean(f.upgrade || f.fn);

const clamp = (x: number) => Math.min(1, Math.max(0, x));

/** How far across the Boundary side has got: the tools give way to the cloud first, then the files
 *  turn to BAML one by one, and only once they're in do the questions get answered. */
const phase = (after: number) => ({
  answered: clamp((after - 0.82) / 0.18),
  cloud: clamp(after / 0.28),
  file: (f: (typeof FILES)[number]) => clamp((after - f.at) / 0.16),
});

/** What you want to know, and what it takes in the traditional stack. */
const ASKS = [
  { ask: 'Evals on real traffic', today: 'export, then join by hand' },
  { ask: 'Accuracy over time', today: 'only runs someone labeled' },
  { ask: 'Exactly what happened', today: 'sampled, across 3 tools' },
  { ask: 'Record and replay', today: 'only if you built it in ahead of time' },
  { ask: 'Cost per feature', today: 'prompts and users live apart' },
];

const RED = 'text-[#f85149]';
const STACK = 'col-start-1 row-start-1';
const GPU = 'will-change-[opacity,transform]';

/** A file. `u` is how far it has gone over to BAML: a file that moves swaps its name, one that only
 *  moves a function grows that function beside its name, and either one picks up the hyper tag. BAML
 *  is filled in, so it reads apart from everything else at a glance. */
function FileChip({
  chipRef,
  f,
  prefix,
  small,
  u,
}: {
  chipRef?: (el: HTMLSpanElement | null) => void;
  f: (typeof FILES)[number];
  // Shows the team in front of the name, where the file isn't already under its team's label.
  prefix?: boolean;
  small?: boolean;
  u: number;
}) {
  const up = f.upgrade ? u : 0;
  const fill = f.baml ? 1 : up;
  const team = prefix && <span style={{ opacity: 0.6 }}>{f.team}/</span>;
  return (
    <span
      className={`relative inline-flex items-center rounded-md border font-mono ${small ? 'px-1.5 py-0.5 text-[11px]' : 'px-1.5 py-1.5 text-[12px]'}`}
      ref={chipRef}
      style={{
        backgroundColor: `rgb(from var(--color-clover) r g b / ${fill})`,
        borderColor: `rgb(from var(--color-clover) r g b / ${0.12 + 0.88 * fill})`,
      }}
    >
      <span className="inline-grid">
        <span
          className={`${STACK} ${f.baml ? 'font-medium text-black' : 'text-ink-2'}`}
          style={{ opacity: 1 - up }}
        >
          {team}
          {f.name}
        </span>
        {f.upgrade && (
          <span
            className={`${STACK} font-medium text-black`}
            style={{ opacity: up }}
          >
            {team}
            {f.upgrade}
          </span>
        )}
      </span>
      {f.fn && (
        <span
          className="inline-flex overflow-hidden whitespace-nowrap"
          style={{ maxWidth: `${u * 8}rem`, opacity: u }}
        >
          <span className="ml-1.5 rounded bg-clover px-1 font-medium text-black">
            {f.fn}
          </span>
        </span>
      )}
      {hyper(f) && (
        <span
          className={`absolute -right-2 -top-2.5 rounded bg-clover px-1 text-[9px] font-semibold leading-[1.4] text-black ring-2 ring-raised ${GPU}`}
          style={{ opacity: u, transform: `scale(${0.6 + 0.4 * u})` }}
        >
          hyper
        </span>
      )}
    </span>
  );
}

function StoreTile({
  s,
  tileRef,
}: {
  s: (typeof STORES)[number];
  tileRef?: (el: HTMLDivElement | null) => void;
}) {
  const lost = s.key === 'nowhere';
  return (
    <div
      className={`flex items-center justify-center gap-2 rounded-lg border px-2 py-2.5 text-[13px] ${lost ? `border-dashed border-[#f85149]/60 font-mono ${RED}` : 'border-white/20 bg-white/[0.04] text-head'}`}
      ref={tileRef}
    >
      {s.mark && <MarkTile mark={s.mark} size={18} />}
      {s.name}
    </div>
  );
}

function Cloud({
  cloudRef,
  small,
}: {
  cloudRef?: (el: HTMLDivElement | null) => void;
  small?: boolean;
}) {
  return (
    <div
      className={`flex items-center justify-center rounded-xl border border-clover/50 bg-raised shadow-[inset_0_0_0_999px_rgb(from_var(--color-clover)_r_g_b_/_0.06)] ${small ? 'gap-2.5 px-3 py-2.5' : 'gap-3 px-5 py-2.5'}`}
      ref={cloudRef}
    >
      <BoundaryMark
        className={`${small ? 'h-6' : 'h-8'} w-auto shrink-0 text-white`}
      />
      <p
        className={`font-semibold text-head ${small ? 'text-[15px]' : 'text-[17px]'}`}
      >
        boundary cloud
      </p>
    </div>
  );
}

function Status({ after, today }: { after: number; today: string }) {
  return (
    <span className="inline-grid font-mono text-[12px]">
      <span className={`${STACK} ${RED}`} style={{ opacity: 1 - after }}>
        ✗ {today}
      </span>
      <span className={`${STACK} text-clover`} style={{ opacity: after }}>
        ✓ one query
      </span>
    </span>
  );
}

/** Where `el` sits inside `root`, from layout offsets, so transforms mid-animation don't move it. */
function within(el: HTMLElement, root: HTMLElement) {
  let x = 0;
  let y = 0;
  let n: HTMLElement | null = el;
  while (n && n !== root) {
    x += n.offsetLeft;
    y += n.offsetTop;
    n = n.offsetParent as HTMLElement | null;
  }
  return { h: el.offsetHeight, w: el.offsetWidth, x, y };
}

type Line = { d: string; hyper?: boolean; key: string; lost?: boolean };

const curve = (x1: number, y1: number, x2: number, y2: number) => {
  const ym = (y1 + y2) / 2;
  return `M${x1} ${y1} C${x1} ${ym} ${x2} ${ym} ${x2} ${y2}`;
};

/** Wider screens: the toggle, the code, the lines from it to where its data goes, and what you can
 *  ask of it. */
function Figure({
  after,
  pick,
  wait,
}: {
  after: number;
  pick: (end: 0 | 1) => void;
  wait: number | null;
}) {
  const root = useRef<HTMLDivElement>(null);
  const chips = useRef(new Map<string, HTMLElement>());
  const tiles = useRef(new Map<Store, HTMLElement>());
  const cloud = useRef<HTMLDivElement | null>(null);
  const [lines, setLines] = useState<{
    h: number;
    trad: Line[];
    unified: Line[];
    w: number;
  }>({
    h: 0,
    trad: [],
    unified: [],
    w: 0,
  });

  const at = phase(after);

  useLayoutEffect(() => {
    const el = root.current;
    if (!el) return;
    const measure = () => {
      // Hidden (phones draw their own figure), so there is nothing to measure.
      if (!el.offsetWidth) return;
      const trad: Line[] = [];
      const unified: Line[] = [];
      // How many lines land on each tool, so they spread across its top instead of meeting at a point.
      const landing = new Map<Store, number>();
      const box = cloud.current ? within(cloud.current, el) : null;
      const middle = box ? box.x + box.w / 2 : 0;
      for (const f of FILES) {
        const chip = chips.current.get(f.name);
        if (!chip) continue;
        const c = within(chip, el);
        const cx = c.x + c.w / 2;
        const cy = c.y + c.h;
        f.to.forEach((key, k) => {
          const tile = tiles.current.get(key);
          if (!tile) return;
          const t = within(tile, el);
          const n = landing.get(key) ?? 0;
          landing.set(key, n + 1);
          const from = cx + (k - (f.to.length - 1) / 2) * 10;
          const to = t.x + t.w / 2 + (n % 2 ? 1 : -1) * Math.ceil(n / 2) * 10;
          trad.push({
            d: curve(from, cy, to, t.y),
            key: `${f.name}>${key}`,
            lost: key === 'nowhere',
          });
        });
        // BAML code lands with hyperobservability; the rest over OpenTelemetry.
        if (box) {
          unified.push({
            d: curve(cx, cy, middle + (cx - middle) * 0.3, box.y),
            hyper: hyper(f),
            key: f.name,
          });
        }
      }
      setLines({ h: el.offsetHeight, trad, unified, w: el.offsetWidth });
    };
    measure();
    // Chips grow as functions move to BAML, which slides their neighbors along, so watch them too.
    const watcher = new ResizeObserver(measure);
    watcher.observe(el);
    for (const chip of chips.current.values()) watcher.observe(chip);
    document.fonts?.ready.then(measure);
    return () => watcher.disconnect();
  }, []);

  return (
    <div className="rounded-xl border border-rule bg-raised p-5 md:p-6">
      <Toggle after={after >= 0.5 ? 1 : 0} pick={pick} wait={wait} />
      <div className="relative mt-6" ref={root}>
        <p className="v4-caption text-ink-2">Your code</p>
        <div className="mt-3 flex justify-between gap-2">
          {TEAMS.map((t) => (
            <div
              className="rounded-lg border border-white/10 bg-white/[0.02] px-1.5 pb-1.5 pt-1.5"
              key={t.name}
            >
              <div className="mb-3">
                <TeamName t={t} />
              </div>
              <div className="flex gap-2">
                {ofTeam(t.name).map((f) => (
                  <FileChip
                    chipRef={(c) => {
                      if (c) chips.current.set(f.name, c);
                      else chips.current.delete(f.name);
                    }}
                    f={f}
                    key={f.name}
                    u={at.file(f)}
                  />
                ))}
              </div>
            </div>
          ))}
        </div>
        <div className="h-24" />
        <div className="grid">
          <div
            className={`${STACK} grid grid-cols-5 gap-3 ${GPU}`}
            style={{ opacity: 1 - at.cloud }}
          >
            {STORES.map((s) => (
              <StoreTile
                key={s.key}
                s={s}
                tileRef={(t) => {
                  if (t) tiles.current.set(s.key, t);
                  else tiles.current.delete(s.key);
                }}
              />
            ))}
          </div>
          <div
            className={`${STACK} mx-auto w-full max-w-[30rem] self-center ${GPU}`}
            style={{
              opacity: at.cloud,
              transform: `scale(${0.94 + 0.06 * at.cloud})`,
            }}
          >
            <Cloud
              cloudRef={(c) => {
                cloud.current = c;
              }}
            />
          </div>
        </div>
        <p className="mt-10 v4-caption text-ink-2">What you want to know</p>
        <div className="mt-3 grid grid-cols-5 gap-3">
          {ASKS.map((a) => (
            <div
              className="rounded-lg border border-rule px-3 py-3"
              key={a.ask}
            >
              <p className="text-[14px] text-head">{a.ask}</p>
              <p className="mt-1.5">
                <Status after={at.answered} today={a.today} />
              </p>
            </div>
          ))}
        </div>
        <svg
          aria-hidden="true"
          className="pointer-events-none absolute inset-0 overflow-visible"
          height={lines.h}
          width={lines.w}
        >
          <g
            className={GPU}
            fill="none"
            strokeWidth={1.25}
            style={{ opacity: 1 - at.cloud }}
          >
            {lines.trad.map((l) => (
              <path
                d={l.d}
                key={l.key}
                stroke={
                  l.lost ? 'rgb(248 81 73 / 0.55)' : 'rgb(255 255 255 / 0.28)'
                }
                strokeDasharray={l.lost ? '4 4' : undefined}
              />
            ))}
          </g>
          <g className={GPU} fill="none" style={{ opacity: at.cloud }}>
            {lines.unified.map((l) => (
              <path
                d={l.d}
                key={l.key}
                stroke="rgb(from var(--color-clover) r g b / 0.4)"
                strokeWidth={1.25}
              />
            ))}
            {lines.unified.map((l) => {
              const f = FILES.find((x) => x.name === l.key);
              if (!f || !hyper(f)) return null;
              return (
                <path
                  d={l.d}
                  key={`${l.key}+`}
                  stroke="var(--color-clover)"
                  strokeWidth={2}
                  style={{ opacity: at.file(f) }}
                />
              );
            })}
          </g>
        </svg>
      </div>
    </div>
  );
}

/** Phones: each stack drawn once, one under the other. The traditional one lists each tool with the
 *  files that feed it; the Boundary one is a single box holding all of them. */
function PhoneSide({ boundary }: { boundary?: boolean }) {
  const after = boundary ? 1 : 0;
  return (
    <div className="rounded-xl border border-rule bg-raised p-4">
      <span
        className={`inline-flex items-center gap-2 rounded-full px-3.5 py-1 font-display text-[15px] font-medium ${boundary ? 'bg-clover text-black' : 'border border-rule text-white/85'}`}
      >
        {boundary && <BoundaryMark className="h-4 w-auto" />}
        {boundary ? 'Boundary' : 'Traditional stack'}
      </span>
      {boundary ? (
        <div className="mt-4 rounded-xl border border-clover/50 p-3 shadow-[inset_0_0_0_999px_rgb(from_var(--color-clover)_r_g_b_/_0.06)]">
          <Cloud small />
          <div className="mt-3 divide-y divide-white/8">
            {TEAMS.map((t) => (
              <div className="flex items-start gap-2 py-2" key={t.name}>
                <span className="w-[7.25rem] shrink-0">
                  <TeamName t={t} />
                </span>
                <span className="flex min-w-0 flex-wrap items-center gap-x-2.5 gap-y-3">
                  {ofTeam(t.name).map((f) => (
                    <FileChip f={f} key={f.name} small u={after} />
                  ))}
                </span>
              </div>
            ))}
          </div>
        </div>
      ) : (
        <div className="mt-4 divide-y divide-rule rounded-xl border border-rule">
          {STORES.map((s) => {
            const lost = s.key === 'nowhere';
            return (
              <div className="flex items-center gap-2.5 px-3 py-2" key={s.key}>
                <span
                  className={`flex w-[6.5rem] shrink-0 items-center gap-1.5 text-[13px] ${lost ? `font-mono ${RED}` : 'text-head'}`}
                >
                  {s.mark && <MarkTile mark={s.mark} size={16} />}
                  {s.name}
                </span>
                <span className="flex min-w-0 flex-wrap gap-1">
                  {FILES.filter((f) => f.to.includes(s.key)).map((f) => (
                    <FileChip f={f} key={f.name} prefix small u={after} />
                  ))}
                </span>
              </div>
            );
          })}
        </div>
      )}
      <div className="mt-4 divide-y divide-rule border-t border-rule">
        {ASKS.map((a) => (
          <div className="py-2" key={a.ask}>
            <p className="text-[13px] text-head">{a.ask}</p>
            <p
              className={`mt-0.5 font-mono text-[11px] ${boundary ? 'text-clover' : RED}`}
            >
              {boundary ? '✓ one query' : `✗ ${a.today}`}
            </p>
          </div>
        ))}
      </div>
    </div>
  );
}

export default function Software() {
  return (
    <section
      {...section('software')}
      aria-labelledby="software"
      className="border-t border-rule"
    >
      <div className="v3-wrap mx-auto px-(--frame-pad) py-20 md:py-28">
        <h2 className="v4-h2 text-head max-w-[22em]" id="software">
          Everyone writes code now.
          <br />
          Boundary gives them great defaults.
        </h2>
        <p className="mt-4 max-w-[40rem] v4-body text-ink-2">
          With BAML it takes zero config. Every team's data lands in one place,
          so anyone can ask any question, or hand it to their agent.
        </p>
        <div className="mt-10 md:mt-12">
          <Looped hold={2400} move={5200}>
            {(p, pick, wait) => (
              <div className="hidden xl:block">
                <Figure after={p} pick={pick} wait={wait} />
              </div>
            )}
          </Looped>
          <div className="space-y-4 xl:hidden">
            <PhoneSide />
            <PhoneSide boundary />
          </div>
        </div>
      </div>
    </section>
  );
}
