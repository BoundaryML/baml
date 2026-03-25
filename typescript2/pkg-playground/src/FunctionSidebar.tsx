import type { FC } from 'react';
import { useEffect, useState } from 'react';
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from './components/ui/collapsible';
import { Button } from './components/ui/button';
import { Input } from './components/ui/input';
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from './components/ui/tooltip';
import { cn } from './lib/utils';
import { Bot, FunctionSquare, ChevronRight, Play, RefreshCw, Search, Square, Loader2, CheckCircle2, XCircle, Ban, FlaskConical } from 'lucide-react';
import type { FunctionInfo, RunEntry, TestInfo } from './worker-protocol';

function TestStatusIcon({ status }: { status?: RunEntry['status'] }) {
  switch (status) {
    case 'running': return <Loader2 size={12} className="text-blue-400 animate-spin" />;
    case 'success': return <CheckCircle2 size={12} className="text-green-400" />;
    case 'error': return <XCircle size={12} className="text-red-400" />;
    case 'cancelled': return <Ban size={12} className="text-vsc-text-faint" />;
    default: return <FlaskConical size={12} className="text-vsc-text-faint" />;
  }
}

export interface FunctionSidebarProps {
  functions: FunctionInfo[];
  tests: TestInfo[];
  selectedFn: string | null;
  onSelectFn: (name: string | null) => void;
  onSelectTest: (test: TestInfo) => void;
  onRunTest: (test: TestInfo) => void;
  isRunning: boolean;
  testStatuses: Map<string, RunEntry['status']>;
  onRunAllTests: () => void;
  onStopAllTests: () => void;
  onRerunFailed: () => void;
  hasFailedTests: boolean;
  hasRunningTests: boolean;
  parallelTests: boolean;
  onToggleParallel: () => void;
}

export const FunctionSidebar: FC<FunctionSidebarProps> = ({
  functions,
  tests,
  selectedFn,
  onSelectFn,
  onSelectTest,
  onRunTest,
  isRunning,
  testStatuses,
  onRunAllTests,
  onStopAllTests,
  onRerunFailed,
  hasFailedTests,
  hasRunningTests,
  parallelTests,
  onToggleParallel,
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
          <Input
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            placeholder="Filter..."
            className="flex-1 h-6 border-none bg-transparent text-xs"
          />
        </div>
      </div>

      {/* Batch controls */}
      {tests.length > 0 && (
        <div className="flex items-center gap-1 px-2 py-1 border-b border-vsc-border shrink-0">
          {hasRunningTests ? (
            <Button
              variant="destructive"
              size="sm"
              className="text-[10px] gap-1"
              onClick={onStopAllTests}
            >
              <Square size={10} /> Stop All
            </Button>
          ) : (
            <Button
              variant="default"
              size="sm"
              className="text-[10px] gap-1"
              onClick={onRunAllTests}
            >
              <Play size={10} /> Run All
            </Button>
          )}
          {hasFailedTests && !hasRunningTests && (
            <Button
              variant="ghost"
              size="sm"
              className="text-[10px] gap-1"
              onClick={onRerunFailed}
            >
              <RefreshCw size={10} /> Re-run Failed
            </Button>
          )}
          <TooltipProvider>
            <Tooltip>
              <TooltipTrigger asChild>
                <Button
                  variant="ghost"
                  size="icon"
                  className={cn('ml-auto h-6 w-6', parallelTests ? 'text-vsc-accent' : 'text-muted-foreground')}
                  onClick={onToggleParallel}
                >
                  <FlaskConical size={12} />
                </Button>
              </TooltipTrigger>
              <TooltipContent>{parallelTests ? 'Running in parallel' : 'Running sequentially'}</TooltipContent>
            </Tooltip>
          </TooltipProvider>
        </div>
      )}

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

          // Compute function-level aggregate border color from test statuses
          const fnStatuses = fnTests.map((t) => testStatuses.get(t.name)).filter(Boolean) as RunEntry['status'][];
          const fnBorderColor = fnStatuses.length === 0 ? '' :
            fnStatuses.some((s) => s === 'running') ? 'border-l-2 border-l-blue-400' :
            fnStatuses.some((s) => s === 'error') ? 'border-l-2 border-l-red-400' :
            fnStatuses.every((s) => s === 'success') ? 'border-l-2 border-l-green-400' : '';

          return (
            <Collapsible
              key={fn.name}
              open={isExpanded}
              onOpenChange={() => toggleExpanded(fn.name)}
            >
              <div
                className={`flex items-center gap-1 px-2 py-1 cursor-pointer text-[11px] font-vsc-mono ${fnBorderColor} ${
                  isSelected
                    ? 'bg-vsc-accent/15 text-vsc-text font-semibold'
                    : 'text-vsc-text-muted hover:bg-vsc-hover'
                }`}
                onClick={() => onSelectFn(isSelected ? null : fn.name)}
              >
                {hasTests ? (
                  <CollapsibleTrigger asChild>
                    <span
                      className="shrink-0 p-0.5 -ml-0.5"
                      onClick={(e) => { e.stopPropagation(); }}
                    >
                      <ChevronRight
                        className={cn('h-3 w-3 text-vsc-text-faint transition-transform', isExpanded && 'rotate-90')}
                      />
                    </span>
                  </CollapsibleTrigger>
                ) : (
                  <span className="w-4 shrink-0" />
                )}
                <Icon className="h-3.5 w-3.5 shrink-0 text-vsc-text-faint" />
                <span className="truncate">{fn.name}</span>
              </div>

              {!hasTests && isSelected && (
                <div className="pl-8 py-1 text-[10px] text-vsc-text-faint italic">
                  No test cases. Add a <code className="font-vsc-mono">test</code> block in your .baml file.
                </div>
              )}

              {hasTests && (
                <CollapsibleContent>
                  {fnTests
                    .filter((t) => !search || t.name.toLowerCase().includes(lowerSearch) || fn.name.toLowerCase().includes(lowerSearch))
                    .map((t) => (
                      <div
                        key={t.name}
                        className="flex items-center gap-1 pl-6 pr-2 py-0.5 cursor-pointer text-[10px] font-vsc-mono text-vsc-text-muted hover:bg-vsc-hover group"
                        onClick={() => onSelectTest(t)}
                      >
                        <div className="flex items-center gap-1.5 flex-1 min-w-0">
                          <TestStatusIcon status={testStatuses.get(t.name)} />
                          <span className="truncate text-[11px]">{t.name}</span>
                        </div>
                        <Button
                          variant="ghost"
                          size="icon"
                          className="h-5 w-5 opacity-0 group-hover:opacity-100 text-muted-foreground hover:text-vsc-green"
                          disabled={isRunning}
                          onClick={(e) => { e.stopPropagation(); onRunTest(t); }}
                        >
                          <Play size={10} />
                        </Button>
                      </div>
                    ))}
                </CollapsibleContent>
              )}
            </Collapsible>
          );
        })}
      </div>
    </div>
  );
};
