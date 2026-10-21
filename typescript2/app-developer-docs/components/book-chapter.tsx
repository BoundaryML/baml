'use client';

import { Check } from 'lucide-react';
import Image from 'next/image';
import {
  createContext,
  type ReactNode,
  useContext,
  useEffect,
  useId,
  useLayoutEffect,
  useRef,
  useState,
} from 'react';
import {
  type BookPerspective,
  bookPerspectiveDefinitions,
  bookPerspectiveIds,
  matchingSection,
  perspectiveHref,
  resolveBookPerspective,
  type SectionKeys,
} from '@/lib/content/book-perspectives';
import { useBookPerspective } from './book-perspective-provider';
import { PerspectiveKey } from './perspective-note';

export interface BookChapterVersion {
  content: ReactNode;
  headings: string[];
  perspective: BookPerspective;
  sectionKeys: SectionKeys;
}

const ChapterContext = createContext<{
  active: BookPerspective;
  change: (perspective: BookPerspective) => void;
} | null>(null);

function currentHeading(headings: string[]) {
  const threshold =
    Number.parseFloat(
      getComputedStyle(document.documentElement).scrollPaddingTop,
    ) || 0;
  const controlsHeight =
    document.querySelector('.book-reading-control')?.getBoundingClientRect()
      .height ?? 0;
  let current: string | undefined;
  for (const id of headings) {
    const element = document.getElementById(id);
    if (
      element &&
      element.getBoundingClientRect().top <= threshold + controlsHeight + 60
    )
      current = id;
  }
  return current;
}

export function BookChapter({ versions }: { versions: BookChapterVersion[] }) {
  const { requested, save } = useBookPerspective();
  const active = resolveBookPerspective(
    requested,
    versions.map((version) => version.perspective),
  );
  const version = versions.find((version) => version.perspective === active);
  const frame = useRef<HTMLDivElement>(null);
  useLayoutEffect(() => {
    if (!version) return;
    const element = frame.current;
    const controls = element?.querySelector('.book-reading-control');
    if (!element || !controls) return;
    const update = () =>
      element.style.setProperty(
        '--book-controls-height',
        `${controls.getBoundingClientRect().height}px`,
      );
    update();
    const observer = new ResizeObserver(update);
    observer.observe(controls);
    return () => observer.disconnect();
  }, [version]);
  const pendingScroll = useRef<{
    heading?: string;
    perspective: BookPerspective;
  } | null>(null);
  useLayoutEffect(() => {
    const pending = pendingScroll.current;
    if (!pending || pending.perspective !== requested) return;
    pendingScroll.current = null;
    if (pending.heading)
      document
        .getElementById(pending.heading)
        ?.scrollIntoView({ behavior: 'instant', block: 'start' });
    else window.scrollTo({ top: 0 });
  }, [requested]);
  if (!version)
    throw new Error('A book chapter must have a default perspective');

  const change = (perspective: BookPerspective) => {
    if (perspective === requested) return;
    const nextActive = resolveBookPerspective(
      perspective,
      versions.map((version) => version.perspective),
    );
    const next = versions.find((version) => version.perspective === nextActive);
    if (!next) return;
    const heading = currentHeading(version.headings);
    const target = matchingSection(
      heading,
      version.sectionKeys,
      next.sectionKeys,
      next.headings,
    );
    const url = new URL(
      perspectiveHref(window.location.href, perspective),
      window.location.origin,
    );
    // A preference change on a default-only chapter should not move the reader.
    if (nextActive !== active) {
      url.hash = target ?? '';
      pendingScroll.current = { heading: target, perspective };
    }
    save(perspective);
    // Next integrates native history updates with useSearchParams, without a server round trip.
    window.history.pushState(
      null,
      '',
      `${url.pathname}${url.search}${url.hash}`,
    );
  };

  return (
    <ChapterContext.Provider value={{ active, change }}>
      <div
        className="flex flex-1 flex-col"
        data-book-perspective={active}
        ref={frame}
      >
        {version.content}
      </div>
    </ChapterContext.Provider>
  );
}

export function BookPerspectiveSelector() {
  const id = useId();
  const { requested } = useBookPerspective();
  const chapter = useContext(ChapterContext);
  const control = useRef<HTMLFieldSetElement>(null);
  const [compact, setCompact] = useState(false);
  useEffect(() => {
    const container = control.current?.closest('[data-book-controls]');
    if (!container) return;
    let frame = 0;
    const update = () => {
      frame = 0;
      const header =
        Number.parseFloat(
          getComputedStyle(document.documentElement).scrollPaddingTop,
        ) || 0;
      setCompact(container.getBoundingClientRect().top <= header + 0.5);
    };
    const schedule = () => {
      if (!frame) frame = requestAnimationFrame(update);
    };
    update();
    window.addEventListener('scroll', schedule, { passive: true });
    window.addEventListener('resize', schedule);
    return () => {
      cancelAnimationFrame(frame);
      window.removeEventListener('scroll', schedule);
      window.removeEventListener('resize', schedule);
    };
  }, []);
  if (!chapter) throw new Error('Perspective selector requires BookChapter');
  return (
    <fieldset
      className="book-reading-control not-typeset"
      data-compact={compact}
      ref={control}
    >
      <legend className="book-reading-heading">
        <span className="book-reading-heading-prefix">Reading </span>perspective
      </legend>
      <div className="book-reading-options">
        {bookPerspectiveIds.map((perspective) => {
          const item = bookPerspectiveDefinitions[perspective];
          const selected = requested === perspective;
          return (
            <label
              className="book-reading-option"
              data-selected={selected}
              key={perspective}
            >
              <input
                aria-label={item.label}
                checked={selected}
                className="sr-only"
                name={id}
                onChange={() => chapter.change(perspective)}
                type="radio"
                value={perspective}
              />
              <Image
                alt=""
                className="book-reading-logo"
                height={16}
                src={item.logo}
                width={16}
              />
              <span className="book-reading-copy">
                <span className="book-reading-option-label book-reading-full-label">
                  {item.label}
                </span>
                <span
                  aria-hidden="true"
                  className="book-reading-option-label book-reading-short-label"
                >
                  {item.shortLabel}
                </span>
              </span>
              <span className="book-reading-check">
                {selected ? <Check aria-hidden="true" size={16} /> : null}
              </span>
            </label>
          );
        })}
      </div>
      {requested !== 'default' ? <PerspectiveKey /> : null}
    </fieldset>
  );
}
