// biome-ignore-all assist/source/organizeImports: Preserve the existing import layout in this legacy component.
// biome-ignore-all assist/source/useSortedAttributes: Preserve the existing JSX attribute order used by render tests.
// biome-ignore-all lint/a11y/useAriaPropsSupportedByRole: Preserve the existing conditional test-row interaction.
// biome-ignore-all lint/a11y/useButtonType: Preserve the existing nested test-row action markup.
// biome-ignore-all lint/a11y/useSemanticElements: Preserve the existing status element markup.
// biome-ignore-all lint/style/useFilenamingConvention: Preserve the existing public component filename.

import type { FC } from 'react';
import { useMemo, useState } from 'react';
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from './components/ui/collapsible';
import { Button } from './components/ui/button';
import { Input } from './components/ui/input';
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from './components/ui/tooltip';
import { cn } from './lib/utils';
import {
  ArrowUpDown,
  Bot,
  Check,
  FunctionSquare,
  ChevronRight,
  RefreshCw,
  Search,
  Loader2,
  FlaskConical,
  Wrench,
} from 'lucide-react';
import {
  buildFunctionSidebarTree,
  type FunctionSidebarTreeNode,
  type FunctionSortOrder,
} from './function-sidebar-tree';
import type {
  SerializedTestDef,
  SerializedTestSet,
} from './serialized-test-tree';
import {
  previewTestKey,
  type FunctionInfo,
  type TestInfo,
} from './worker-protocol';
import {
  getSidebarLeafPaddingLeft,
  SIDEBAR_LEAF_ICON_CLASS,
  SIDEBAR_LEAF_ROW_CLASS,
} from './function-test-sidebar-row-styles';

// ---------------------------------------------------------------------------
// TestTreeNode — recursive tree renderer for SerializedTestDef items
// ---------------------------------------------------------------------------

interface TestTreeNodeProps {
  def: SerializedTestDef;
  depth?: number;
  /** Disable run/retry actions while the runtime is unavailable. */
  disabled?: boolean;
  selectedTestName?: string | null;
  onSelectTest?: (name: string) => void;
  onRunTest?: (name: string) => void;
  testRunResults?: Map<string, unknown>;
  failedExpands?: Set<string>;
  onRetryExpand?: (name: string) => void;
}

function TestTreeNode({
  def,
  depth = 0,
  disabled = false,
  selectedTestName,
  onSelectTest,
  onRunTest,
  testRunResults,
  failedExpands,
  onRetryExpand,
}: TestTreeNodeProps) {
  const [expanded, setExpanded] = useState(true);
  const branchIndent = 8 + depth * 12;
  const leafIndent = getSidebarLeafPaddingLeft(depth);

  if ('type' in def && def.type === 'lazyTestSet') {
    const isFailed = failedExpands?.has(def.name);
    return (
      <div
        className={cn(SIDEBAR_LEAF_ROW_CLASS, 'text-vsc-text-muted')}
        style={{ paddingLeft: leafIndent }}
      >
        {isFailed ? (
          <FlaskConical
            className={cn(SIDEBAR_LEAF_ICON_CLASS, 'text-red-500')}
          />
        ) : (
          <Loader2 className={cn(SIDEBAR_LEAF_ICON_CLASS, 'animate-spin')} />
        )}
        <span className="truncate font-medium italic text-vsc-text-faint">
          {def.name.split('/').pop()}
        </span>
        <span
          className={cn(
            'text-[9px] ml-1',
            isFailed ? 'text-red-500' : 'text-vsc-text-faint',
          )}
        >
          {isFailed ? 'failed' : 'loading\u2026'}
        </span>
        {isFailed && onRetryExpand && (
          <button
            className="ml-auto text-[9px] text-vsc-text-faint hover:text-vsc-text px-1 shrink-0 disabled:cursor-not-allowed disabled:opacity-50"
            disabled={disabled}
            onClick={(e) => {
              e.stopPropagation();
              onRetryExpand(def.name);
            }}
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
    const reportObj =
      report != null && typeof report === 'object'
        ? (report as Record<string, unknown>)
        : null;
    const outcome =
      typeof reportObj?.outcome === 'string' ? reportObj.outcome : undefined;
    const isSelected = selectedTestName === def.name;
    return (
      <div
        role={onSelectTest ? 'button' : undefined}
        tabIndex={onSelectTest ? 0 : undefined}
        aria-pressed={onSelectTest ? isSelected : undefined}
        className={cn(
          SIDEBAR_LEAF_ROW_CLASS,
          onSelectTest && 'cursor-pointer',
          isSelected
            ? 'bg-vsc-accent/15 text-vsc-text font-semibold'
            : 'text-vsc-text-muted hover:bg-vsc-hover',
        )}
        style={{ paddingLeft: leafIndent }}
        onClick={() => onSelectTest?.(def.name)}
        onKeyDown={(event) => {
          if (!onSelectTest || (event.key !== 'Enter' && event.key !== ' ')) {
            return;
          }
          event.preventDefault();
          onSelectTest(def.name);
        }}
        title={`Show workflow for test: ${def.name}`}
      >
        <FlaskConical className={SIDEBAR_LEAF_ICON_CLASS} />
        <span className="truncate">{def.name.split('/').pop()}</span>
        {outcome && (
          <span
            className={cn(
              'ml-auto text-[9px] shrink-0',
              outcome === 'pass' ? 'text-green-500' : 'text-red-500',
            )}
            role="status"
            aria-label={`Latest test run status: ${outcome}`}
            title={`Latest test run status: ${outcome}`}
          >
            {outcome}
          </span>
        )}
        {onRunTest && (
          <button
            className={cn(
              'text-[9px] text-vsc-text-faint hover:text-vsc-text px-1 shrink-0 disabled:cursor-not-allowed disabled:opacity-50',
              !outcome && 'ml-auto',
            )}
            disabled={disabled}
            onClick={(e) => {
              e.stopPropagation();
              onRunTest(def.name);
            }}
            title={`Run test: ${def.name}`}
          >
            run
          </button>
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
        style={{ paddingLeft: branchIndent }}
      >
        <ChevronRight
          className={cn(
            'h-3 w-3 text-vsc-text-faint transition-transform',
            expanded && 'rotate-90',
          )}
        />
        <span className="truncate text-[11px] font-medium">
          {set.name.split('/').pop()}
        </span>
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
            key={`${'name' in child ? child.name : i}-${i}`}
            def={child}
            depth={depth + 1}
            disabled={disabled}
            selectedTestName={selectedTestName}
            onSelectTest={onSelectTest}
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
  /** Rendered workflow graph node count for each function. */
  workflowNodeCounts?: ReadonlyMap<string, number>;
  /** Whether internal functions are currently shown (toggled from the
   * panel's settings gear menu) — used only for the empty-state message. */
  showInternalFunctions: boolean;
  internalFunctionCount: number;
  isLoadingProject?: boolean;
  /** Disable Run/Test-derived actions until the current build is ready. */
  runtimeControlsDisabled?: boolean;
  testTree?: SerializedTestDef[] | null;
  previewTests?: TestInfo[];
  selectedPreviewTestKey?: string | null;
  onSelectPreviewTest?: (test: TestInfo) => void;
  selectedTestName?: string | null;
  onSelectTest?: (name: string) => void;
  selectedFn: string | null;
  onSelectFn: (name: string | null) => void;
  onRefreshTests: () => void;
  onRunTest?: (name: string) => void;
  testRunResults?: Map<string, unknown>;
  /** Testset names whose expansion failed — shows error state instead of spinner */
  failedExpands?: Set<string>;
  /** Called when the user clicks retry on a failed testset expansion */
  onRetryExpand?: (name: string) => void;
  /** Number of debug fetch logs captured while collecting or expanding tests. */
  collectionLogCount?: number;
  /** True when the main panel is showing the collection view */
  viewingCollection?: boolean;
  /** Called when the user clicks the collection debug icon */
  onSelectCollectionView?: () => void;
}

type FunctionFolderOpenState = Record<string, boolean>;

interface FunctionTreeNodeProps {
  node: FunctionSidebarTreeNode;
  depth?: number;
  selectedFn: string | null;
  openFolderKeys: FunctionFolderOpenState;
  forcedOpenFolderKeys: Set<string>;
  onFolderOpenChange: (key: string, open: boolean) => void;
  onSelectFn: (name: string | null) => void;
}

function FunctionTreeNode({
  node,
  depth = 0,
  selectedFn,
  openFolderKeys,
  forcedOpenFolderKeys,
  onFolderOpenChange,
  onSelectFn,
}: FunctionTreeNodeProps) {
  const indent = 8 + depth * 12;

  if (node.type === 'folder') {
    const forcedOpen = forcedOpenFolderKeys.has(node.key);
    const requestedOpen = openFolderKeys[node.key];
    const collapsePending = forcedOpen && requestedOpen === false;
    const open = forcedOpen || (requestedOpen ?? false);
    const collapsePendingMessage =
      'Will collapse when this folder is no longer kept open automatically';
    return (
      <Collapsible
        open={open}
        onOpenChange={(nextOpen) =>
          onFolderOpenChange(
            node.key,
            collapsePending && !nextOpen ? true : nextOpen,
          )
        }
      >
        <CollapsibleTrigger
          className="flex items-center gap-1 w-full pr-2 py-0.5 cursor-pointer text-[10px] font-vsc-mono text-vsc-text-muted hover:bg-vsc-hover"
          style={{ paddingLeft: indent }}
          title={
            collapsePending
              ? `${node.path.join('.')} — ${collapsePendingMessage}`
              : node.path.join('.')
          }
          data-collapse-pending={collapsePending || undefined}
        >
          <ChevronRight
            className={cn(
              'h-3 w-3 shrink-0 transition-transform',
              collapsePending ? 'text-vsc-accent' : 'text-vsc-text-faint',
              open && !collapsePending && 'rotate-90',
            )}
          />
          <span className="truncate text-[11px] font-medium">{node.name}</span>
          <span className="text-vsc-text-faint ml-1">
            ({node.functionCount})
          </span>
          {collapsePending && (
            <span className="sr-only">{collapsePendingMessage}</span>
          )}
        </CollapsibleTrigger>
        <CollapsibleContent>
          {node.children.map((child) => (
            <FunctionTreeNode
              key={child.key}
              node={child}
              depth={depth + 1}
              selectedFn={selectedFn}
              openFolderKeys={openFolderKeys}
              forcedOpenFolderKeys={forcedOpenFolderKeys}
              onFolderOpenChange={onFolderOpenChange}
              onSelectFn={onSelectFn}
            />
          ))}
        </CollapsibleContent>
      </Collapsible>
    );
  }

  const isSelected = selectedFn === node.fullName;
  const isInternal = node.functionInfo.origin !== 'userDefined';
  const Icon = node.functionInfo.kind === 'llm' ? Bot : FunctionSquare;
  const sourcePosition = node.functionInfo.sourcePosition;
  const sourceLabel = sourcePosition
    ? `${sourcePosition.file} at ${sourcePosition.line}:${sourcePosition.column}`
    : 'Source position unavailable';
  return (
    <TooltipProvider delayDuration={300}>
      <Tooltip>
        <TooltipTrigger asChild>
          <button
            type="button"
            className={cn(
              SIDEBAR_LEAF_ROW_CLASS,
              'cursor-pointer',
              isSelected
                ? 'bg-vsc-accent/15 text-vsc-text font-semibold'
                : 'text-vsc-text-muted hover:bg-vsc-hover',
            )}
            style={{ paddingLeft: getSidebarLeafPaddingLeft(depth) }}
            onClick={() => onSelectFn(isSelected ? null : node.fullName)}
          >
            <Icon className={SIDEBAR_LEAF_ICON_CLASS} />
            <span className="truncate">{node.label}</span>
            {((node.workflowNodeCount != null && node.workflowNodeCount > 1) ||
              isInternal) && (
              <span className="ml-auto flex shrink-0 items-center gap-1">
                {node.workflowNodeCount != null &&
                  node.workflowNodeCount > 1 && (
                    <span
                      aria-hidden="true"
                      className="rounded border border-vsc-border bg-vsc-bg-secondary px-1 py-0 text-[9px] font-normal tabular-nums text-vsc-text-faint"
                    >
                      {node.workflowNodeCount}
                    </span>
                  )}
                {isInternal && (
                  <span
                    aria-hidden="true"
                    className="rounded border border-vsc-border px-1 py-0 text-[9px] text-vsc-text-faint"
                  >
                    {node.functionInfo.origin}
                  </span>
                )}
              </span>
            )}
          </button>
        </TooltipTrigger>
        <TooltipContent
          align="start"
          className="max-w-[28rem] text-left text-balance"
          side="right"
          sideOffset={4}
        >
          <div className="flex flex-col gap-1">
            <code className="whitespace-pre-wrap font-vsc-mono text-[11px]">
              {node.functionInfo.signature ?? node.fullName}
            </code>
            <span className="opacity-80">{sourceLabel}</span>
            <span className="opacity-80">
              Call graph nodes: {node.workflowNodeCount ?? 'calculating…'}
            </span>
          </div>
        </TooltipContent>
      </Tooltip>
    </TooltipProvider>
  );
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export const FunctionSidebar: FC<FunctionSidebarProps> = ({
  functions,
  workflowNodeCounts,
  showInternalFunctions,
  internalFunctionCount,
  isLoadingProject = false,
  runtimeControlsDisabled = false,
  testTree,
  previewTests = [],
  selectedPreviewTestKey,
  onSelectPreviewTest,
  selectedTestName,
  onSelectTest,
  selectedFn,
  onSelectFn,
  onRefreshTests,
  onRunTest,
  testRunResults,
  failedExpands,
  onRetryExpand,
  collectionLogCount,
  viewingCollection,
  onSelectCollectionView,
}) => {
  const [search, setSearch] = useState('');
  // Accordion state: tests are the primary view; functions start collapsed.
  const [functionsOpen, setFunctionsOpen] = useState(false);
  const [functionSortOrder, setFunctionSortOrder] =
    useState<FunctionSortOrder>('workflowNodeCount');
  const [showFunctionSortMenu, setShowFunctionSortMenu] = useState(false);
  const [openFolderKeys, setOpenFolderKeys] = useState<FunctionFolderOpenState>(
    {},
  );
  const [testsOpen, setTestsOpen] = useState(true);

  const functionTree = useMemo(
    () =>
      buildFunctionSidebarTree(functions, {
        search,
        selectedFunctionName: selectedFn,
        sortOrder: functionSortOrder,
        workflowNodeCounts,
      }),
    [functions, functionSortOrder, search, selectedFn, workflowNodeCounts],
  );
  const hasFunctionSearch = search.trim() !== '';
  const setFunctionFolderOpen = (key: string, open: boolean) => {
    setOpenFolderKeys((prev) => ({ ...prev, [key]: open }));
  };

  const treeItems = testTree ?? [];
  const previewNameCounts = useMemo(() => {
    const counts = new Map<string, number>();
    for (const test of previewTests) {
      counts.set(test.name, (counts.get(test.name) ?? 0) + 1);
    }
    return counts;
  }, [previewTests]);
  let emptyFunctionMessage = 'No matches';
  if (isLoadingProject) {
    emptyFunctionMessage = 'Loading project...';
  } else if (functions.length === 0) {
    emptyFunctionMessage =
      internalFunctionCount > 0 && !showInternalFunctions
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
      </div>

      {/* Accordion: Functions (collapsed by default) + Tests (open) */}
      <div className="flex-1 overflow-y-auto py-0.5">
        {/* Functions section — typing in the filter forces it open */}
        <Collapsible
          open={functionsOpen || hasFunctionSearch}
          onOpenChange={setFunctionsOpen}
        >
          <div className="flex items-center gap-1 px-2 py-0.5 text-[11px] font-semibold text-vsc-text-muted">
            <CollapsibleTrigger className="flex min-w-0 flex-1 items-center gap-1 cursor-pointer border-none bg-transparent p-0 text-left text-[11px] font-semibold text-vsc-text-muted hover:bg-vsc-hover">
              <ChevronRight
                className={cn(
                  'h-3 w-3 text-vsc-text-faint transition-transform',
                  (functionsOpen || hasFunctionSearch) && 'rotate-90',
                )}
              />
              <FunctionSquare size={12} />
              <span>Functions</span>
              {isLoadingProject ? (
                <Loader2 className="ml-1 h-3 w-3 animate-spin text-vsc-text-faint" />
              ) : (
                <span className="text-vsc-text-faint ml-1">
                  ({functionTree.functionCount})
                </span>
              )}
            </CollapsibleTrigger>
            <TooltipProvider delayDuration={300}>
              <div className="relative shrink-0">
                <Tooltip>
                  <TooltipTrigger asChild>
                    <button
                      aria-expanded={showFunctionSortMenu}
                      aria-haspopup="true"
                      aria-label={`Sort order: ${
                        functionSortOrder === 'workflowNodeCount'
                          ? 'Call graph node count'
                          : 'Alphanumeric'
                      }`}
                      className="inline-flex h-5 w-5 shrink-0 items-center justify-center rounded-md text-vsc-text-faint outline-none transition-colors hover:bg-vsc-hover hover:text-vsc-text focus-visible:ring-2 focus-visible:ring-ring/50"
                      onClick={() => setShowFunctionSortMenu((value) => !value)}
                      type="button"
                    >
                      <ArrowUpDown size={10} />
                    </button>
                  </TooltipTrigger>
                  <TooltipContent side="right">Sort order</TooltipContent>
                </Tooltip>
                {showFunctionSortMenu && (
                  <>
                    <button
                      aria-label="Close sort order menu"
                      className="fixed inset-0 z-40 cursor-default border-none bg-transparent"
                      onClick={() => setShowFunctionSortMenu(false)}
                      type="button"
                    />
                    <div
                      aria-label="Sort order"
                      className="absolute right-0 top-full z-50 mt-1 w-44 rounded border border-vsc-border bg-vsc-surface p-1 shadow-lg"
                      role="radiogroup"
                    >
                      {(
                        [
                          ['workflowNodeCount', 'Call graph node count'],
                          ['alphanumeric', 'Alphanumeric'],
                        ] satisfies Array<[FunctionSortOrder, string]>
                      ).map(([value, label]) => (
                        <label
                          className="flex cursor-pointer items-center gap-2 rounded px-2 py-1.5 text-[11px] font-normal text-vsc-text-muted hover:bg-vsc-hover"
                          key={value}
                        >
                          <input
                            checked={functionSortOrder === value}
                            className="sr-only"
                            name="function-sort-order"
                            onChange={() => {
                              setFunctionSortOrder(value);
                              setShowFunctionSortMenu(false);
                            }}
                            type="radio"
                            value={value}
                          />
                          <Check
                            aria-hidden="true"
                            className={cn(
                              'h-3 w-3',
                              functionSortOrder === value
                                ? 'opacity-100'
                                : 'opacity-0',
                            )}
                          />
                          <span>{label}</span>
                        </label>
                      ))}
                    </div>
                  </>
                )}
              </div>
            </TooltipProvider>
          </div>
          <CollapsibleContent>
            {functionTree.functionCount === 0 && (
              <div className="px-2 py-3 text-center text-vsc-text-faint text-[11px]">
                {emptyFunctionMessage}
              </div>
            )}

            {functionTree.nodes.map((node) => (
              <FunctionTreeNode
                key={node.key}
                node={node}
                selectedFn={selectedFn}
                openFolderKeys={openFolderKeys}
                forcedOpenFolderKeys={functionTree.forcedOpenFolderKeys}
                onFolderOpenChange={setFunctionFolderOpen}
                onSelectFn={onSelectFn}
              />
            ))}
          </CollapsibleContent>
        </Collapsible>

        {/* Tests section — open by default */}
        <div className="border-t border-vsc-border mt-1">
          <Collapsible open={testsOpen} onOpenChange={setTestsOpen}>
            <div className="flex items-center gap-1 px-2 py-1 text-[11px] font-semibold text-vsc-text-muted">
              <CollapsibleTrigger className="flex items-center gap-1 flex-1 min-w-0 cursor-pointer text-left bg-transparent border-none p-0 text-[11px] font-semibold text-vsc-text-muted hover:bg-vsc-hover">
                <ChevronRight
                  className={cn(
                    'h-3 w-3 text-vsc-text-faint transition-transform',
                    testsOpen && 'rotate-90',
                  )}
                />
                <FlaskConical size={12} />
                <span>Tests</span>
              </CollapsibleTrigger>
              {onSelectCollectionView && (
                <Button
                  variant="ghost"
                  size="icon"
                  className={`ml-auto h-5 w-5 ${viewingCollection ? 'text-vsc-accent' : 'text-vsc-text-faint hover:text-vsc-text'}`}
                  onClick={onSelectCollectionView}
                  title={
                    collectionLogCount && collectionLogCount > 0
                      ? `View collection logs (${collectionLogCount} request${collectionLogCount !== 1 ? 's' : ''})`
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
                disabled={runtimeControlsDisabled}
                title="Re-collect tests"
              >
                <RefreshCw size={10} />
              </Button>
            </div>

            <CollapsibleContent>
              {!testTree && previewTests.length === 0 && (
                <div className="px-4 py-2 text-[10px] text-vsc-text-faint italic">
                  No test data yet
                </div>
              )}

              {testTree &&
                treeItems.length === 0 &&
                previewTests.length === 0 && (
                  <div className="px-4 py-2 text-[10px] text-vsc-text-faint italic">
                    No tests found
                  </div>
                )}

              {previewTests.map((test) => {
                const key = previewTestKey(test);
                const duplicateName =
                  (previewNameCounts.get(test.name) ?? 0) > 1;
                return (
                  <button
                    type="button"
                    key={key}
                    className={cn(
                      SIDEBAR_LEAF_ROW_CLASS,
                      'cursor-pointer',
                      selectedPreviewTestKey === key
                        ? 'bg-vsc-accent/15 text-vsc-text font-semibold'
                        : 'text-vsc-text-muted hover:bg-vsc-hover',
                    )}
                    style={{ paddingLeft: getSidebarLeafPaddingLeft() }}
                    onClick={() => onSelectPreviewTest?.(test)}
                    title={`Use ${test.name} args for ${test.functionName}`}
                  >
                    <FlaskConical className={SIDEBAR_LEAF_ICON_CLASS} />
                    <span className="truncate">
                      {test.name}
                      {duplicateName ? ` → ${test.functionName}` : ''}
                    </span>
                    <span className="ml-auto text-[9px] text-vsc-text-faint shrink-0">
                      preview
                    </span>
                  </button>
                );
              })}

              {treeItems.map((def, i) => (
                <TestTreeNode
                  key={`${'name' in def ? def.name : i}-${i}`}
                  def={def}
                  disabled={runtimeControlsDisabled}
                  selectedTestName={selectedTestName}
                  onSelectTest={onSelectTest}
                  onRunTest={onRunTest}
                  testRunResults={testRunResults}
                  failedExpands={failedExpands}
                  onRetryExpand={onRetryExpand}
                />
              ))}
            </CollapsibleContent>
          </Collapsible>
        </div>
      </div>
    </div>
  );
};
