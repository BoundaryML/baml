import type { FC } from 'react';
import { useState } from 'react';
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from './components/ui/collapsible';
import { Button } from './components/ui/button';
import { Input } from './components/ui/input';
import { cn } from './lib/utils';
import { Bot, FunctionSquare, ChevronRight, RefreshCw, Search, Loader2, FlaskConical, Wrench } from 'lucide-react';
import type { FunctionInfo, RunEntry } from './worker-protocol';

// ---------------------------------------------------------------------------
// SerializedTestDef — the proto-decoded shape from TestRegistry.serialize()
// ---------------------------------------------------------------------------

/** A single test: { type: "test", name: string } */
export type SerializedTest = { type: 'test'; name: string };

/** A lazy (not-yet-expanded) testset: { type: "lazyTestSet", name: string } */
export type SerializedLazyTestSet = { type: 'lazyTestSet'; name: string };

/** An expanded testset: { name: string, items: SerializedTestDef[], loadingTimeMs: number } */
export type SerializedTestSet = { name: string; items: SerializedTestDef[]; loadingTimeMs: number };

export type SerializedTestDef = SerializedTest | SerializedLazyTestSet | SerializedTestSet;

// ---------------------------------------------------------------------------
// TestTreeNode — recursive tree renderer for SerializedTestDef items
// ---------------------------------------------------------------------------

interface TestTreeNodeProps {
  def: SerializedTestDef;
  depth?: number;
  onRunTest?: (name: string) => void;
  testRunResults?: Map<string, unknown>;
  failedExpands?: Set<string>;
  onRetryExpand?: (name: string) => void;
}

function TestTreeNode({ def, depth = 0, onRunTest, testRunResults, failedExpands, onRetryExpand }: TestTreeNodeProps) {
  const [expanded, setExpanded] = useState(true);
  const indent = 8 + depth * 12;

  if ('type' in def && def.type === 'lazyTestSet') {
    const isFailed = failedExpands?.has(def.name);
    return (
      <div
        className="flex items-center gap-1.5 pr-2 py-0.5 text-[10px] font-vsc-mono text-vsc-text-muted"
        style={{ paddingLeft: indent }}
      >
        {isFailed ? (
          <FlaskConical size={12} className="text-red-500 shrink-0" />
        ) : (
          <Loader2 size={12} className="animate-spin text-vsc-text-faint shrink-0" />
        )}
        <span className="truncate text-[11px] font-medium italic text-vsc-text-faint">
          {def.name.split('/').pop()}
        </span>
        <span className={cn('text-[9px] ml-1', isFailed ? 'text-red-500' : 'text-vsc-text-faint')}>
          {isFailed ? 'failed' : 'loading\u2026'}
        </span>
        {isFailed && onRetryExpand && (
          <button
            className="ml-auto text-[9px] text-vsc-text-faint hover:text-vsc-text px-1 shrink-0"
            onClick={(e) => { e.stopPropagation(); onRetryExpand(def.name); }}
            title={`Retry expansion: ${def.name}`}
          >
            retry
          </button>
        )}
      </div>
    );
  }

  if ('type' in def && def.type === 'test') {
    const report = testRunResults?.get(def.name);
    const reportObj = report != null && typeof report === 'object' ? (report as Record<string, unknown>) : null;
    const outcome = typeof reportObj?.outcome === 'string' ? reportObj.outcome : undefined;
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

  // Expanded testset: has `name` + `items` (no `type` field, or type absent)
  const set = def as SerializedTestSet;
  return (
    <Collapsible open={expanded} onOpenChange={setExpanded}>
      <CollapsibleTrigger
        className="flex items-center gap-1 w-full pr-2 py-0.5 cursor-pointer text-[10px] font-vsc-mono text-vsc-text-muted hover:bg-vsc-hover"
        style={{ paddingLeft: indent }}
      >
        <ChevronRight className={cn('h-3 w-3 text-vsc-text-faint transition-transform', expanded && 'rotate-90')} />
        <span className="truncate text-[11px] font-medium">{set.name.split('/').pop()}</span>
        <span className="text-vsc-text-faint ml-1">({set.items.length})</span>
        {set.loadingTimeMs > 0 && (
          <span className="text-[9px] text-vsc-text-faint ml-auto shrink-0">
            {set.loadingTimeMs >= 1000
              ? `${(set.loadingTimeMs / 1000).toFixed(1)}s`
              : `${set.loadingTimeMs}ms`}
          </span>
        )}
      </CollapsibleTrigger>
      <CollapsibleContent>
        {set.items.map((child, i) => (
          <TestTreeNode
            key={`${('name' in child ? child.name : i)}-${i}`}
            def={child}
            depth={depth + 1}
            onRunTest={onRunTest}
            testRunResults={testRunResults}
            failedExpands={failedExpands}
            onRetryExpand={onRetryExpand}
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
  showInternalFunctions: boolean;
  onShowInternalFunctionsChange: (show: boolean) => void;
  internalFunctionCount: number;
  testTree?: any; // SerializedTestDef[] from BAML TestRegistry.serialize()
  selectedFn: string | null;
  onSelectFn: (name: string | null) => void;
  onRefreshTests: () => void;
  onRunTest?: (name: string) => void;
  testRunResults?: Map<string, unknown>;
  /** Testset names whose expansion failed — shows error state instead of spinner */
  failedExpands?: Set<string>;
  /** Called when the user clicks retry on a failed testset expansion */
  onRetryExpand?: (name: string) => void;
  /** The synthetic collection RunEntry (if any) — used to show fetch log count badge */
  collectionRun?: RunEntry | null;
  /** True when the main panel is showing the collection view */
  viewingCollection?: boolean;
  /** Called when the user clicks the collection debug icon */
  onSelectCollectionView?: () => void;
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export const FunctionSidebar: FC<FunctionSidebarProps> = ({
  functions,
  showInternalFunctions,
  onShowInternalFunctionsChange,
  internalFunctionCount,
  testTree,
  selectedFn,
  onSelectFn,
  onRefreshTests,
  onRunTest,
  testRunResults,
  failedExpands,
  onRetryExpand,
  collectionRun,
  viewingCollection,
  onSelectCollectionView,
}) => {
  const [search, setSearch] = useState('');

  const lowerSearch = search.toLowerCase();

  // Filter functions: visible if name matches search
  const filteredFns = functions.filter((fn) => {
    if (!search) return true;
    return fn.name.toLowerCase().includes(lowerSearch);
  });

  const treeItems: SerializedTestDef[] = Array.isArray(testTree) ? testTree : [];
  let emptyFunctionMessage = 'No matches';
  if (functions.length === 0) {
    emptyFunctionMessage = internalFunctionCount > 0 && !showInternalFunctions
      ? 'No user functions'
      : 'No functions yet';
  }

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
        {internalFunctionCount > 0 && (
          <label className="mt-1.5 flex items-center gap-1.5 text-[10px] text-vsc-text-faint cursor-pointer select-none">
            <input
              type="checkbox"
              checked={showInternalFunctions}
              onChange={(e) => onShowInternalFunctionsChange(e.currentTarget.checked)}
              className="h-3 w-3 accent-vsc-accent"
            />
            <span>Show internal functions</span>
            <span className="ml-auto font-vsc-mono">{internalFunctionCount}</span>
          </label>
        )}
      </div>

      {/* Function list */}
      <div className="flex-1 overflow-y-auto py-0.5">
        {filteredFns.length === 0 && (
          <div className="px-2 py-3 text-center text-vsc-text-faint text-[11px]">
            {emptyFunctionMessage}
          </div>
        )}

        {filteredFns.map((fn) => {
          const isSelected = selectedFn === fn.name;
          const isInternal = fn.origin !== 'userDefined';
          const Icon = fn.kind === 'llm' ? Bot : FunctionSquare;

          return (
            <button
              type="button"
              key={fn.name}
              className={`flex items-center gap-1 w-full px-2 py-1 cursor-pointer text-[11px] font-vsc-mono text-left ${
                isSelected
                  ? 'bg-vsc-accent/15 text-vsc-text font-semibold'
                  : 'text-vsc-text-muted hover:bg-vsc-hover'
              }`}
              onClick={() => onSelectFn(isSelected ? null : fn.name)}
            >
              <span className="w-4 shrink-0" />
              <Icon className="h-3.5 w-3.5 shrink-0 text-vsc-text-faint" />
              <span className="truncate">{fn.name}</span>
              {isInternal && (
                <span className="ml-auto shrink-0 rounded border border-vsc-border px-1 py-0 text-[9px] text-vsc-text-faint">
                  {fn.origin}
                </span>
              )}
            </button>
          );
        })}

        {/* Tests section */}
        <div className="border-t border-vsc-border mt-1">
          <div className="flex items-center gap-1 px-2 py-1 text-[11px] font-semibold text-vsc-text-muted">
            <FlaskConical size={12} />
            <span>Tests</span>
            {onSelectCollectionView && (
              <Button
                variant="ghost"
                size="icon"
                className={`ml-auto h-5 w-5 ${viewingCollection ? 'text-vsc-accent' : 'text-vsc-text-faint hover:text-vsc-text'}`}
                onClick={onSelectCollectionView}
                title={
                  collectionRun && collectionRun.fetchLogs.length > 0
                    ? `View collection logs (${collectionRun.fetchLogs.length} request${collectionRun.fetchLogs.length !== 1 ? 's' : ''})`
                    : 'View collection logs'
                }
              >
                <Wrench size={10} />
              </Button>
            )}
            <Button
              variant="ghost"
              size="icon"
              className={`h-5 w-5 text-vsc-text-faint hover:text-vsc-text${onSelectCollectionView ? '' : ' ml-auto'}`}
              onClick={onRefreshTests}
              title="Re-collect tests"
            >
              <RefreshCw size={10} />
            </Button>
          </div>

          {!testTree && (
            <div className="px-4 py-2 text-[10px] text-vsc-text-faint italic">
              No test data yet
            </div>
          )}

          {testTree && treeItems.length === 0 && (
            <div className="px-4 py-2 text-[10px] text-vsc-text-faint italic">
              No tests found
            </div>
          )}

          {treeItems.map((def, i) => (
            <TestTreeNode
              key={`${'name' in def ? def.name : i}-${i}`}
              def={def}
              onRunTest={onRunTest}
              testRunResults={testRunResults}
              failedExpands={failedExpands}
            />
          ))}
        </div>
      </div>
    </div>
  );
};
