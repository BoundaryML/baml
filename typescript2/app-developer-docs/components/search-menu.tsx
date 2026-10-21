'use client';

import { ArrowRight, Search } from 'lucide-react';
import { useRouter } from 'next/navigation';
import {
  type KeyboardEvent as ReactKeyboardEvent,
  useEffect,
  useMemo,
  useRef,
  useState,
} from 'react';

import { searchablePages } from '@/lib/navigation';
import {
  type GeneratedSearchIndex,
  generatedSearchIndexSchema,
  type SearchEntry,
} from '@/lib/search';

const authoredEntries: SearchEntry[] = searchablePages.map((page) => ({
  ...page,
  current: true,
}));
const MAX_RESULTS = 100;

function isEditableTarget(target: EventTarget | null) {
  return (
    (target instanceof HTMLElement && target.isContentEditable) ||
    target instanceof HTMLInputElement ||
    target instanceof HTMLTextAreaElement ||
    target instanceof HTMLSelectElement
  );
}

export function SearchMenu() {
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState('');
  const [selectedIndex, setSelectedIndex] = useState(0);
  const [generatedIndex, setGeneratedIndex] =
    useState<GeneratedSearchIndex | null>(null);
  const [generatedIndexError, setGeneratedIndexError] = useState(false);
  const [versionFilter, setVersionFilter] = useState('current');
  const inputRef = useRef<HTMLInputElement>(null);
  const router = useRouter();

  const matchingResults = useMemo(() => {
    const entries = [
      ...authoredEntries,
      ...(generatedIndex?.entries ?? []),
    ].filter((page) => {
      if (!page.version) return true;
      if (versionFilter === 'all') return true;
      if (versionFilter === 'current') return page.current;
      return page.version === versionFilter;
    });
    const normalized = query.trim().toLowerCase();
    if (!normalized) return entries;
    return entries.filter((page) =>
      `${page.label} ${page.group} ${page.href} ${page.keywords ?? ''}`
        .toLowerCase()
        .includes(normalized),
    );
  }, [generatedIndex, query, versionFilter]);
  const results = matchingResults.slice(0, MAX_RESULTS);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      const requestsSearch =
        (event.key.toLowerCase() === 'k' && (event.metaKey || event.ctrlKey)) ||
        event.key === '/';

      if (requestsSearch && !isEditableTarget(event.target)) {
        event.preventDefault();
        setOpen((current) => !current);
      }
      if (event.key === 'Escape') setOpen(false);
    };
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, []);

  useEffect(() => {
    if (!open) return;
    setSelectedIndex(0);
    const frame = requestAnimationFrame(() => inputRef.current?.focus());
    return () => cancelAnimationFrame(frame);
  }, [open]);

  useEffect(() => {
    if (!open || generatedIndex || generatedIndexError) return;
    void fetch('/search-index.json')
      .then(async (response) => {
        if (!response.ok) throw new Error('Unable to load search index.');
        return generatedSearchIndexSchema.parse(await response.json());
      })
      .then(setGeneratedIndex)
      .catch(() => setGeneratedIndexError(true));
  }, [generatedIndex, generatedIndexError, open]);

  const visit = (href: string) => {
    setOpen(false);
    setQuery('');
    router.push(href);
  };

  const onInputKeyDown = (event: ReactKeyboardEvent<HTMLInputElement>) => {
    if (!results.length) return;
    if (event.key === 'ArrowDown') {
      event.preventDefault();
      setSelectedIndex((current) => (current + 1) % results.length);
    }
    if (event.key === 'ArrowUp') {
      event.preventDefault();
      setSelectedIndex(
        (current) => (current - 1 + results.length) % results.length,
      );
    }
    if (event.key === 'Enter') {
      event.preventDefault();
      const selected = results[selectedIndex];
      if (selected) visit(selected.href);
    }
  };

  return (
    <>
      <button
        aria-label="Search documentation"
        className="docs-focus-ring relative inline-flex h-8 w-full items-center justify-start rounded-lg border-none bg-muted px-3 text-sm text-foreground shadow-none transition-colors hover:bg-muted/50 md:w-48 lg:w-40 xl:w-64 dark:bg-card"
        onClick={() => setOpen(true)}
        type="button"
      >
        <span className="hidden xl:inline-flex">Search documentation...</span>
        <span className="inline-flex xl:hidden">Search...</span>
      </button>
      {open ? (
        <div
          aria-label="Search documentation"
          aria-modal="true"
          className="fixed inset-0 z-[100]"
          role="dialog"
        >
          <button
            aria-label="Close search"
            className="absolute inset-0 cursor-default bg-transparent"
            onClick={() => setOpen(false)}
            type="button"
          />
          <div className="fixed top-[15%] left-1/2 z-10 grid w-full max-w-[calc(100%-2rem)] -translate-x-1/2 gap-0 rounded-xl border-none bg-popover bg-clip-padding p-2 pb-11 text-popover-foreground shadow-2xl ring-4 ring-neutral-200/80 outline-none sm:max-w-lg dark:bg-neutral-900 dark:ring-neutral-800">
            <h2 className="sr-only">Search documentation</h2>
            <div className="flex h-9 items-center gap-3 rounded-md border border-input bg-input/50 px-3">
              <Search
                aria-hidden="true"
                className="size-4 shrink-0 text-muted-foreground"
              />
              <input
                aria-label="Search pages"
                className="min-w-0 flex-1 bg-transparent text-sm outline-none placeholder:text-muted-foreground"
                onChange={(event) => {
                  setQuery(event.target.value);
                  setSelectedIndex(0);
                }}
                onKeyDown={onInputKeyDown}
                placeholder="Search documentation..."
                ref={inputRef}
                value={query}
              />
            </div>
            <div
              className="scrollbar-none min-h-80 max-h-[22rem] overflow-y-auto scroll-py-1.5 py-1.5"
              id="search-results"
            >
              {generatedIndex?.versions.length ? (
                <label className="mb-1 flex items-center gap-2 px-3 py-1 text-xs text-muted-foreground">
                  Reference version
                  <select
                    className="ml-auto rounded border border-input bg-background px-2 py-1 text-foreground"
                    onChange={(event) => {
                      setVersionFilter(event.target.value);
                      setSelectedIndex(0);
                    }}
                    value={versionFilter}
                  >
                    <option value="current">Current channels</option>
                    {generatedIndex.versions.map((version) => (
                      <option
                        key={version.routeVersion}
                        value={version.routeVersion}
                      >
                        {version.routeVersion}
                        {version.channels.length
                          ? ` (${version.channels.join(', ')})`
                          : ''}
                      </option>
                    ))}
                    <option value="all">All versions</option>
                  </select>
                </label>
              ) : null}
              {results.length ? (
                <>
                  {results.map((page, index) => (
                    <button
                      className="flex min-h-9 w-full items-center gap-3 rounded-md border border-transparent px-3 py-2 text-left text-sm font-medium outline-none data-[selected=true]:border-input data-[selected=true]:bg-input/50 hover:bg-input/40"
                      data-selected={index === selectedIndex}
                      id={`search-result-${index}`}
                      key={`${page.href}-${page.label}`}
                      onClick={() => visit(page.href)}
                      onMouseEnter={() => setSelectedIndex(index)}
                      type="button"
                    >
                      <ArrowRight
                        aria-hidden="true"
                        className="size-4 shrink-0 text-muted-foreground"
                      />
                      <span className="min-w-0 truncate">{page.label}</span>
                      <span className="ml-auto max-w-44 shrink-0 truncate text-xs font-normal text-muted-foreground">
                        {page.group}
                      </span>
                    </button>
                  ))}
                  {matchingResults.length > MAX_RESULTS ? (
                    <p className="px-3 py-2 text-xs text-muted-foreground">
                      Showing the first {MAX_RESULTS} of{' '}
                      {matchingResults.length} results. Refine your search to
                      narrow the list.
                    </p>
                  ) : null}
                </>
              ) : (
                <p className="py-12 text-center text-sm text-muted-foreground">
                  {generatedIndexError
                    ? 'Generated reference search is unavailable. Authored pages remain searchable.'
                    : 'No results found.'}
                </p>
              )}
            </div>
            <div className="absolute inset-x-0 bottom-0 z-20 flex h-10 items-center gap-3 rounded-b-xl border-t border-t-neutral-100 bg-neutral-50 px-4 text-xs font-medium text-muted-foreground dark:border-t-neutral-700 dark:bg-neutral-800">
              <span className="flex items-center gap-2">
                <kbd className="flex h-5 items-center rounded border bg-background px-1 font-sans text-[0.7rem]">
                  ↵
                </kbd>
                Go to Page
              </span>
              <span className="ml-auto flex items-center gap-1">
                <kbd className="flex h-5 items-center rounded border bg-background px-1 font-sans text-[0.7rem]">
                  ↑
                </kbd>
                <kbd className="flex h-5 items-center rounded border bg-background px-1 font-sans text-[0.7rem]">
                  ↓
                </kbd>
                Navigate
              </span>
            </div>
          </div>
        </div>
      ) : null}
    </>
  );
}
