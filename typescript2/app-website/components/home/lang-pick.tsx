'use client';

import { type ComponentType, useCallback, useEffect, useState } from 'react';
import {
  SiCplusplus,
  SiGo,
  SiOpenjdk,
  SiPython,
  SiRust,
  SiSwift,
  SiTypescript,
} from 'react-icons/si';

// The languages BAML generates an SDK for. Every language picker on the site uses this list and the
// same choice.
export const SDK_LANGS: { name: string }[] = [
  { name: 'Python' },
  { name: 'TypeScript' },
  { name: 'Go' },
  { name: 'Rust' },
  { name: 'Java' },
  { name: 'C++' },
  { name: 'Swift' },
];

// Each language's mark, from Simple Icons like the rest of the homepage. Java's is OpenJDK's.
const ICONS: Record<string, ComponentType<{ className?: string }>> = {
  'C++': SiCplusplus,
  Go: SiGo,
  Java: SiOpenjdk,
  Python: SiPython,
  Rust: SiRust,
  Swift: SiSwift,
  TypeScript: SiTypescript,
};

/** A language's logo, sized to sit next to its name. Nothing for a name it doesn't know. */
export function LangIcon({
  className = 'size-3.5',
  name,
}: {
  className?: string;
  name: string;
}) {
  const Icon = ICONS[name];
  return Icon ? <Icon aria-hidden className={`shrink-0 ${className}`} /> : null;
}

const KEY = 'boundary-sdk-lang';
const EVENT = 'boundary-sdk-lang';

function read() {
  try {
    const i = SDK_LANGS.findIndex((l) => l.name === localStorage.getItem(KEY));
    return i < 0 ? 0 : i;
  } catch {
    return 0;
  }
}

/** The visitor's language, pinned: picking it anywhere changes it everywhere, and it's remembered. */
export function usePinnedLang(): [number, (i: number) => void] {
  const [at, setLocal] = useState(0);
  useEffect(() => {
    setLocal(read());
    const sync = () => setLocal(read());
    window.addEventListener(EVENT, sync);
    window.addEventListener('storage', sync);
    return () => {
      window.removeEventListener(EVENT, sync);
      window.removeEventListener('storage', sync);
    };
  }, []);
  const setAt = useCallback((i: number) => {
    setLocal(i);
    try {
      localStorage.setItem(KEY, SDK_LANGS[i].name);
    } catch {
      // No storage: the choice still holds on this page, through the event below.
    }
    window.dispatchEvent(new Event(EVENT));
  }, []);
  return [at, setAt];
}

/** One pill per language, with its logo. */
export function LangPicker({
  at,
  setAt,
}: {
  at: number;
  setAt: (i: number) => void;
}) {
  return (
    <div className="flex flex-wrap items-center gap-1.5">
      {SDK_LANGS.map((l, i) => (
        <button
          aria-pressed={i === at}
          className={`focus-ring flex cursor-pointer items-center gap-1.5 rounded-full border px-3 py-1 text-[13px] transition-colors ${i === at ? 'border-clover bg-clover/15 text-head' : 'border-white/12 text-ink-2 hover:border-white/30'}`}
          key={l.name}
          onClick={() => setAt(i)}
          type="button"
        >
          <LangIcon name={l.name} />
          {l.name}
        </button>
      ))}
    </div>
  );
}
