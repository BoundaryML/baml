import type { FC } from 'react';
import { useState } from 'react';
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from './components/ui/collapsible';
import { Button } from './components/ui/button';
import { Input } from './components/ui/input';
import { cn } from './lib/utils';
import { Bot, FunctionSquare, ChevronRight, RefreshCw, Search, Loader2, FlaskConical } from 'lucide-react';
import type { FunctionInfo, TestCollectionStatus, TestDef } from './worker-protocol';

// ---------------------------------------------------------------------------
// TestTreeNode — recursive tree renderer for TestDef items
// ---------------------------------------------------------------------------

interface TestTreeNodeProps {
  def: TestDef;
  depth?: number;
  onExpandTestSet?: (name: string) => void;
  cachedExpansions?: Map<string, TestDef>;
  expandingTestSets?: Set<string>;
  onRunTest?: (name: string) => void;
  testRunResults?: Map<string, Record<string, unknown>>;
}

function TestTreeNode({ def, depth = 0, onExpandTestSet, cachedExpansions, expandingTestSets, onRunTest, testRunResults }: TestTreeNodeProps) {
  const [expanded, setExpanded] = useState(true);
  const indent = 8 + depth * 12;

  if (def.type === 'lazyTestSet') {
    // Check if we have a cached expansion for this lazy set
    const cached = cachedExpansions?.get(def.name);
    if (cached) {
      return (
        <TestTreeNode
          def={cached}
          depth={depth}
          onExpandTestSet={onExpandTestSet}
          cachedExpansions={cachedExpansions}
          expandingTestSets={expandingTestSets}
          onRunTest={onRunTest}
          testRunResults={testRunResults}
        />
      );
    }
    const isExpanding = expandingTestSets?.has(def.name) ?? false;
    return (
      <div
        className="flex items-center gap-1.5 pr-2 py-0.5 text-[10px] font-vsc-mono text-vsc-text-muted cursor-pointer hover:bg-vsc-hover"
        style={{ paddingLeft: indent }}
        onClick={() => !isExpanding && onExpandTestSet?.(def.name)}
      >
        {isExpanding
          ? <Loader2 size={12} className="animate-spin text-vsc-text-faint shrink-0" />
          : <ChevronRight size={12} className="text-vsc-text-faint shrink-0" />
        }
        <span className="truncate text-[11px] font-medium italic text-vsc-text-faint">
          {def.name.split('/').pop()}
        </span>
        <span className="text-[9px] text-vsc-text-faint ml-1">
          {isExpanding ? 'loading…' : 'click to load'}
        </span>
      </div>
    );
  }

  if (def.type === 'test') {
    const report = testRunResults?.get(def.name);
    const outcome = typeof report?.outcome === 'string' ? report.outcome : undefined;
    return (
      <div
        className="flex items-center gap-1.5 pr-2 py-0.5 text-[10px] font-vsc-mono text-vsc-text-muted"
        style={{ paddingLeft: indent }}
      >
        <FlaskConical size={12} className="text-vsc-text-faint shrink-0" />
        <span className="truncate text-[11px]">{def.name.split('/').pop()}</span>
        {onRunTest && (
          <button
            className="ml-auto text-[9px] text-vsc-text-faint hover:text-vsc-text px-1 shrink-0"
            onClick={(e) => { e.stopPropagation(); onRunTest(def.name); }}
            title={`Run test: ${def.name}`}
          >
            run
          </button>
        )}
        {outcome && (
          <span className={cn(
            'text-[9px] shrink-0',
            outcome === 'pass' ? 'text-green-500' : 'text-red-500'
          )}>
            {outcome}
          </span>
        )}
      </div>
    );
  }

  return (
    <Collapsible open={expanded} onOpenChange={setExpanded}>
      <CollapsibleTrigger asChild>
        <div
          className="flex items-center gap-1 pr-2 py-0.5 cursor-pointer text-[10px] font-vsc-mono text-vsc-text-muted hover:bg-vsc-hover"
          style={{ paddingLeft: indent }}
        >
          <ChevronRight className={cn('h-3 w-3 text-vsc-text-faint transition-transform', expanded && 'rotate-90')} />
          <span className="truncate text-[11px] font-medium">{def.name}</span>
          <span className="text-vsc-text-faint ml-1">({def.items.length})</span>
          {def.totalLoadingTimeMs > 0 && (
            <span className="text-[9px] text-vsc-text-faint ml-auto shrink-0">
              {def.totalLoadingTimeMs >= 1000
                ? `${(def.totalLoadingTimeMs / 1000).toFixed(1)}s`
                : `${def.totalLoadingTimeMs}ms`}
            </span>
          )}
        </div>
      </CollapsibleTrigger>
      <CollapsibleContent>
        {def.items.map((child, i) => (
          <TestTreeNode
            key={`${child.name}-${i}`}
            def={child}
            depth={depth + 1}
            onExpandTestSet={onExpandTestSet}
            cachedExpansions={cachedExpansions}
            expandingTestSets={expandingTestSets}
            onRunTest={onRunTest}
            testRunResults={testRunResults}
          />
        ))}
      </CollapsibleContent>
    </Collapsible>
  );
}

// ---------------------------------------------------------------------------
// Props
// ---------------------------------------------------------------------------

export interface FunctionSidebarProps {
  functions: FunctionInfo[];
  testCollection?: TestCollectionStatus;
  selectedFn: string | null;
  onSelectFn: (name: string | null) => void;
  onRefreshTests: () => void;
  onExpandTestSet?: (name: string) => void;
  cachedExpansions?: Map<string, TestDef>;
  expandingTestSets?: Set<string>;
  backgroundTaskCount?: number;
  onRunTest?: (name: string) => void;
  testRunResults?: Map<string, Record<string, unknown>>;
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export const FunctionSidebar: FC<FunctionSidebarProps> = ({
  functions,
  testCollection,
  selectedFn,
  onSelectFn,
  onRefreshTests,
  onExpandTestSet,
  cachedExpansions,
  expandingTestSets,
  backgroundTaskCount = 0,
  onRunTest,
  testRunResults,
}) => {
  const [search, setSearch] = useState('');

  const lowerSearch = search.toLowerCase();

  // Filter functions: visible if name matches search
  const filteredFns = functions.filter((fn) => {
    if (!search) return true;
    return fn.name.toLowerCase().includes(lowerSearch);
  });

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

      {/* Function list */}
      <div className="flex-1 overflow-y-auto py-0.5">
        {filteredFns.length === 0 && (
          <div className="px-2 py-3 text-center text-vsc-text-faint text-[11px]">
            {functions.length === 0 ? 'No functions yet' : 'No matches'}
          </div>
        )}

        {filteredFns.map((fn) => {
          const isSelected = selectedFn === fn.name;
          const Icon = fn.kind === 'llm' ? Bot : FunctionSquare;

          return (
            <div
              key={fn.name}
              className={`flex items-center gap-1 px-2 py-1 cursor-pointer text-[11px] font-vsc-mono ${
                isSelected
                  ? 'bg-vsc-accent/15 text-vsc-text font-semibold'
                  : 'text-vsc-text-muted hover:bg-vsc-hover'
              }`}
              onClick={() => onSelectFn(isSelected ? null : fn.name)}
            >
              <span className="w-4 shrink-0" />
              <Icon className="h-3.5 w-3.5 shrink-0 text-vsc-text-faint" />
              <span className="truncate">{fn.name}</span>
            </div>
          );
        })}

        {/* Tests section */}
        <div className="border-t border-vsc-border mt-1">
          <div className="flex items-center gap-1 px-2 py-1 text-[11px] font-semibold text-vsc-text-muted">
            <FlaskConical size={12} />
            <span>Tests</span>
            {backgroundTaskCount > 0 && (
              <span className="flex items-center gap-1 text-[9px] text-vsc-text-faint font-normal">
                <Loader2 size={10} className="animate-spin" />
                {backgroundTaskCount} task{backgroundTaskCount !== 1 ? 's' : ''}
              </span>
            )}
            <Button
              variant="ghost"
              size="icon"
              className="ml-auto h-5 w-5 text-vsc-text-faint hover:text-vsc-text"
              onClick={onRefreshTests}
              title="Re-collect tests"
            >
              <RefreshCw size={10} />
            </Button>
          </div>

          {!testCollection && (
            <div className="px-4 py-2 text-[10px] text-vsc-text-faint italic">
              No test data yet
            </div>
          )}

          {testCollection?.status === 'collecting' && (
            <div className="flex items-center gap-1.5 px-4 py-2 text-[10px] text-vsc-text-faint">
              <Loader2 size={12} className="animate-spin" />
              Collecting tests...
            </div>
          )}

          {testCollection?.status === 'error' && (
            <div className="px-4 py-2 text-[10px] text-red-400">
              Error: {testCollection.message}
            </div>
          )}

          {testCollection?.status === 'done' && testCollection.items.length === 0 && (
            <div className="px-4 py-2 text-[10px] text-vsc-text-faint italic">
              No tests found
            </div>
          )}

          {testCollection?.status === 'done' && testCollection.items.map((def, i) => (
            <TestTreeNode
              key={`${def.name}-${i}`}
              def={def}
              onExpandTestSet={onExpandTestSet}
              cachedExpansions={cachedExpansions}
              expandingTestSets={expandingTestSets}
              onRunTest={onRunTest}
              testRunResults={testRunResults}
            />
          ))}
        </div>
      </div>
    </div>
  );
};
