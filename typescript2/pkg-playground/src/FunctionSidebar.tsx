import type { FC } from 'react';
import { useEffect, useState } from 'react';
import * as Collapsible from '@radix-ui/react-collapsible';
import { Bot, FunctionSquare, ChevronRight, Play, Search } from 'lucide-react';
import type { FunctionInfo, TestInfo } from './worker-protocol';

export interface FunctionSidebarProps {
  functions: FunctionInfo[];
  tests: TestInfo[];
  selectedFn: string | null;
  onSelectFn: (name: string | null) => void;
  onSelectTest: (test: TestInfo) => void;
  onRunTest: (test: TestInfo) => void;
  isRunning: boolean;
}

export const FunctionSidebar: FC<FunctionSidebarProps> = ({
  functions,
  tests,
  selectedFn,
  onSelectFn,
  onSelectTest,
  onRunTest,
  isRunning,
}) => {
  const [search, setSearch] = useState('');
  const [expandedFns, setExpandedFns] = useState<Set<string>>(new Set());

  const lowerSearch = search.toLowerCase();

  // Group tests by function name
  const testsByFn = new Map<string, TestInfo[]>();
  for (const t of tests) {
    const arr = testsByFn.get(t.functionName) ?? [];
    arr.push(t);
    testsByFn.set(t.functionName, arr);
  }

  // Auto-expand selected function's test group when selection changes from outside
  useEffect(() => {
    if (selectedFn && testsByFn.has(selectedFn)) {
      setExpandedFns((prev) => {
        if (prev.has(selectedFn)) return prev;
        const next = new Set(prev);
        next.add(selectedFn);
        return next;
      });
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps -- testsByFn is derived from tests, not stable
  }, [selectedFn]);

  // Filter functions: visible if name matches search OR any of its tests match
  const filteredFns = functions.filter((fn) => {
    if (!search) return true;
    if (fn.name.toLowerCase().includes(lowerSearch)) return true;
    const fnTests = testsByFn.get(fn.name) ?? [];
    return fnTests.some((t) => t.name.toLowerCase().includes(lowerSearch));
  });

  const toggleExpanded = (name: string) => {
    setExpandedFns((prev) => {
      const next = new Set(prev);
      if (next.has(name)) next.delete(name);
      else next.add(name);
      return next;
    });
  };

  return (
    <div className="flex flex-col h-full">
      {/* Search */}
      <div className="px-2 py-1.5 border-b border-vsc-border shrink-0">
        <div className="flex items-center gap-1 px-1.5 py-0.5 rounded border border-vsc-input-border bg-vsc-input-bg">
          <Search className="h-3 w-3 text-vsc-text-faint shrink-0" />
          <input
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            placeholder="Filter..."
            className="flex-1 bg-transparent text-vsc-input-fg font-vsc-mono text-[11px] border-none outline-none placeholder:text-vsc-text-faint"
          />
        </div>
      </div>

      {/* Function list */}
      <div className="flex-1 overflow-y-auto py-0.5">
        {filteredFns.length === 0 && (
          <div className="px-2 py-3 text-center text-vsc-text-faint text-[11px]">
            {functions.length === 0 ? 'No functions yet' : 'No matches'}
          </div>
        )}

        {filteredFns.map((fn) => {
          const fnTests = testsByFn.get(fn.name) ?? [];
          const hasTests = fnTests.length > 0;
          const isSelected = selectedFn === fn.name;
          const isExpanded = expandedFns.has(fn.name);
          const Icon = fn.kind === 'llm' ? Bot : FunctionSquare;

          return (
            <Collapsible.Root
              key={fn.name}
              open={isExpanded}
              onOpenChange={() => toggleExpanded(fn.name)}
            >
              <div
                className={`flex items-center gap-1 px-2 py-1 cursor-pointer text-[11px] font-vsc-mono ${
                  isSelected
                    ? 'bg-vsc-accent/15 text-vsc-text font-semibold'
                    : 'text-vsc-text-muted hover:bg-vsc-hover'
                }`}
                onClick={() => onSelectFn(isSelected ? null : fn.name)}
              >
                {hasTests ? (
                  <Collapsible.Trigger asChild>
                    <span
                      className="shrink-0 p-0.5 -ml-0.5"
                      onClick={(e) => { e.stopPropagation(); }}
                    >
                      <ChevronRight
                        className={`h-3 w-3 text-vsc-text-faint transition-transform ${
                          isExpanded ? 'rotate-90' : ''
                        }`}
                      />
                    </span>
                  </Collapsible.Trigger>
                ) : (
                  <span className="w-4 shrink-0" />
                )}
                <Icon className="h-3.5 w-3.5 shrink-0 text-vsc-text-faint" />
                <span className="truncate">{fn.name}</span>
              </div>

              {hasTests && (
                <Collapsible.Content>
                  {fnTests
                    .filter((t) => !search || t.name.toLowerCase().includes(lowerSearch) || fn.name.toLowerCase().includes(lowerSearch))
                    .map((t) => (
                      <div
                        key={t.name}
                        className="flex items-center gap-1 pl-8 pr-2 py-0.5 cursor-pointer text-[10px] font-vsc-mono text-vsc-text-muted hover:bg-vsc-hover group"
                        onClick={() => onSelectTest(t)}
                      >
                        <span className="truncate flex-1">{t.name}</span>
                        <button
                          disabled={isRunning}
                          onClick={(e) => { e.stopPropagation(); onRunTest(t); }}
                          className="opacity-0 group-hover:opacity-100 p-0.5 rounded hover:bg-vsc-accent/20 disabled:opacity-30"
                          title="Run test"
                        >
                          <Play className="h-3 w-3 text-vsc-green" />
                        </button>
                      </div>
                    ))}
                </Collapsible.Content>
              )}
            </Collapsible.Root>
          );
        })}
      </div>
    </div>
  );
};
