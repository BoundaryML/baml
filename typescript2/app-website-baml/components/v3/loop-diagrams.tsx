'use client';

import {
  type ComponentType,
  type ReactNode,
  useCallback,
  useEffect,
  useState,
} from 'react';
import { track } from '../track';
import HeroCoverage from './hero-coverage';
import HeroLanguages from './hero-languages';
import { HeroSplitChat, HeroSplitQuery } from './hero-languages-split';
import { DIAGRAM_STYLES } from './hero-languages-styled';
import { HeroLifelineIntegrations } from './hero-minimal';
import HeroSelect from './hero-select';
import HeroTabs from './hero-tabs';
import { HeroVideoCode } from './hero-video';

// Takes on the same hero, to compare in place. The arrows, the arrow keys, and ?diagram=2 in the URL
// all pick one. This is the shortlist; the versions that were cut are still in this folder, unused,
// until one is chosen and the rest can go. A styled version also takes a style from /diagram-styles,
// picked with the style button, the up and down arrow keys, or ?style=ink in the URL.
// A version marked `whole` draws the entire hero, headline and buttons included, in place of the
// shared intro. A version with `options` gets a button for each one in the menu, which steps through its
// values; ?shape=dot in the URL picks one too.
const DIAGRAMS: {
  Diagram: ComponentType<{ options?: Record<string, string>; style?: string }>;
  options?: {
    id: string;
    name: string;
    values: { id: string; name: string }[];
  }[];
  name: string;
  styled?: boolean;
  whole?: boolean;
}[] = [
  { Diagram: HeroLanguages, name: 'Languages and BAML' },
  { Diagram: HeroCoverage, name: 'Tracker, by language', styled: true },
  { Diagram: HeroLifelineIntegrations, name: 'Lifeline, integrations' },
  { Diagram: HeroSplitQuery, name: 'Split, query', styled: true },
  { Diagram: HeroSplitChat, name: 'Split, agent chat', styled: true },
  { Diagram: HeroTabs, name: 'Tabs, file and terminal', whole: true },
  { Diagram: HeroVideoCode, name: 'Video, code lights up' },
  {
    Diagram: HeroSelect,
    name: 'Queries pick calls',
    options: [
      {
        id: 'shape',
        name: 'Shape',
        values: [
          { id: 'span', name: 'Spans' },
          { id: 'dot', name: 'Dots' },
          { id: 'pixel', name: 'Pixels' },
        ],
      },
    ],
  },
];

// The version shown when the URL doesn't pick one: 2, the tracker.
const DEFAULT_INDEX = 1;

function Chevron({ flip }: { flip?: boolean }) {
  return (
    <svg
      aria-hidden="true"
      className={flip ? 'rotate-180' : ''}
      fill="none"
      height="14"
      stroke="currentColor"
      strokeWidth="1.6"
      viewBox="0 0 14 14"
      width="14"
    >
      <path d="M9 2 4 7l5 5" />
    </svg>
  );
}

export default function LoopDiagrams({ intro }: { intro: ReactNode }) {
  const [index, setIndex] = useState(DEFAULT_INDEX);
  const [styleIndex, setStyleIndex] = useState(0);
  const [picked, setPicked] = useState<Record<string, string>>({});

  useEffect(() => {
    const params = new URLSearchParams(location.search);
    const n = Number(params.get('diagram'));
    if (n >= 1 && n <= DIAGRAMS.length) setIndex(n - 1);
    const s = DIAGRAM_STYLES.findIndex((x) => x.id === params.get('style'));
    if (s >= 0) setStyleIndex(s);
    setPicked(Object.fromEntries(params));
  }, []);

  const { Diagram, name, options, styled, whole } = DIAGRAMS[index];
  const style = DIAGRAM_STYLES[styleIndex];
  // Each option's value: the one picked, or its first.
  const chosen = (options ?? []).map((o) => ({
    ...o,
    value: o.values.find((v) => v.id === picked[o.id]) ?? o.values[0],
  }));
  const values = Object.fromEntries(chosen.map((o) => [o.id, o.value.id]));

  const stepOption = (o: (typeof chosen)[number]) => {
    const next = o.values[(o.values.indexOf(o.value) + 1) % o.values.length];
    setPicked((p) => ({ ...p, [o.id]: next.id }));
    track('v3_diagram_option_selected', { option: o.id, value: next.id });
    const url = new URL(location.href);
    url.searchParams.set(o.id, next.id);
    history.replaceState(null, '', url);
  };

  const stepStyle = useCallback(
    (by: number) => {
      const next =
        (styleIndex + by + DIAGRAM_STYLES.length) % DIAGRAM_STYLES.length;
      setStyleIndex(next);
      track('v3_diagram_style_selected', { style: DIAGRAM_STYLES[next].id });
      const url = new URL(location.href);
      url.searchParams.set('style', DIAGRAM_STYLES[next].id);
      history.replaceState(null, '', url);
    },
    [styleIndex],
  );

  const step = useCallback(
    (by: number) => {
      const next = (index + by + DIAGRAMS.length) % DIAGRAMS.length;
      setIndex(next);
      track('v3_diagram_selected', { diagram: DIAGRAMS[next].name });
      const url = new URL(location.href);
      url.searchParams.set('diagram', String(next + 1));
      history.replaceState(null, '', url);
    },
    [index],
  );

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'ArrowLeft') step(-1);
      if (e.key === 'ArrowRight') step(1);
      if (styled && (e.key === 'ArrowUp' || e.key === 'ArrowDown')) {
        e.preventDefault();
        stepStyle(e.key === 'ArrowDown' ? 1 : -1);
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [step, stepStyle, styled]);

  const arrow =
    'focus-ring size-8 grid place-items-center rounded-full text-white/70 hover:bg-white/10 hover:text-white cursor-pointer transition-colors';

  return (
    <>
      {/* The key restarts each version's animation and timers when it comes back. */}
      {whole ? (
        <Diagram key={name} />
      ) : (
        <>
          {/* The styled versions sit higher, so their production panel shows without scrolling. */}
          <div className={styled ? '-mt-4 md:-mt-10' : undefined}>{intro}</div>
          <div className={styled ? 'mt-6' : 'mt-12 md:mt-16'}>
            <Diagram key={name} options={values} style={style.id} />
          </div>
        </>
      )}
      {/* A tool for comparing versions, so it floats over the page rather than being part of the design. */}
      <div className="fixed z-40 bottom-5 left-1/2 -translate-x-1/2 flex items-center gap-1 p-1 rounded-full border border-white/15 bg-black/80 backdrop-blur-md shadow-[0_12px_40px_rgb(0_0_0/0.6)]">
        <button
          aria-label="Previous version"
          className={arrow}
          onClick={() => step(-1)}
          type="button"
        >
          <Chevron />
        </button>
        <p
          aria-live="polite"
          className={`${chosen.length > 1 ? 'w-24 sm:w-52' : styled || options ? 'w-40 sm:w-52' : 'w-52'} text-center text-[13px] text-white/55 whitespace-nowrap truncate`}
        >
          <span className="text-white">{name}</span> {index + 1}/
          {DIAGRAMS.length}
        </p>
        <button
          aria-label="Next version"
          className={arrow}
          onClick={() => step(1)}
          type="button"
        >
          <Chevron flip />
        </button>
        {styled && (
          <button
            aria-label={`Style: ${style.name}. Switch to the next style`}
            className="focus-ring h-8 px-3 rounded-full border-l border-white/15 text-[13px] text-white whitespace-nowrap hover:bg-white/10 cursor-pointer transition-colors"
            onClick={(e) => stepStyle(e.shiftKey ? -1 : 1)}
            type="button"
          >
            <span className="hidden sm:inline text-white/55">Style </span>
            {style.name}
          </button>
        )}
        {chosen.map((o) => (
          <button
            aria-label={`${o.name}: ${o.value.name}. Switch to the next one`}
            className="focus-ring h-8 px-3 rounded-full border-l border-white/15 text-[13px] text-white whitespace-nowrap hover:bg-white/10 cursor-pointer transition-colors"
            key={o.id}
            onClick={() => stepOption(o)}
            type="button"
          >
            <span className="hidden sm:inline text-white/55">{o.name} </span>
            {o.value.name}
          </button>
        ))}
      </div>
    </>
  );
}
