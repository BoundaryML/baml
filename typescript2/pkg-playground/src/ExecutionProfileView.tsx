import { Search } from 'lucide-react';
import { type FC, useMemo, useState } from 'react';

import { Input } from './components/ui/input';
import { ToggleGroup } from './components/ui/toggle-group';
import { cn } from './lib/utils';
import {
  buildExecutionProfileProjection,
  executionProfileColorKey,
  executionProfileSearchFunctionKeys,
  filterExecutionProfileProjection,
  type ExecutionProfileBlock,
  type ExecutionProfileColorMode,
  type ExecutionProfileFunctionRow,
  type ExecutionProfileOrigin,
} from './run-store-projections';
import type { Run } from './worker-protocol';

type ExecutionProfileViewProps = {
  run: Run | undefined;
};

type PositionedProfileBlock = ExecutionProfileBlock & { lane: number };
type ProfileDepthTrack = {
  depth: number;
  laneCount: number;
  blocks: PositionedProfileBlock[];
};
type ActiveProfileFunctions = ReadonlySet<string> | null;

type ProfileColor = {
  background: string;
  border: string;
  text: string;
};

const PROFILE_COLOR_MODE_OPTIONS: Array<{
  value: ExecutionProfileColorMode;
  label: string;
}> = [
  { value: 'function', label: 'Function' },
  { value: 'origin', label: 'Origin' },
  { value: 'thread', label: 'Thread' },
];

const PROFILE_PALETTE: ProfileColor[] = [
  { background: '#2d7dd2', border: '#75b6ff', text: '#f7fbff' },
  { background: '#c45a7a', border: '#f0a1bb', text: '#fff7fa' },
  { background: '#2e8b72', border: '#80d7c0', text: '#f5fffc' },
  { background: '#b7791f', border: '#f0c36c', text: '#fffaf0' },
  { background: '#7c5cc4', border: '#b9a4f0', text: '#fbf8ff' },
  { background: '#c05621', border: '#f3a36f', text: '#fff8f3' },
  { background: '#2b8a9f', border: '#78d2e5', text: '#f3fdff' },
  { background: '#8a6f2b', border: '#d5bf69', text: '#fffbed' },
  { background: '#5c7c2f', border: '#abd06f', text: '#fbfff4' },
  { background: '#9b4c9b', border: '#d796d7', text: '#fff6ff' },
];

const ORIGIN_COLORS: Record<ExecutionProfileOrigin, ProfileColor> = {
  user: { background: '#2d7dd2', border: '#75b6ff', text: '#f7fbff' },
  library: { background: '#f0a44a', border: '#ffd08a', text: '#1f1303' },
  system: { background: '#b26ab3', border: '#dfabe0', text: '#fff6ff' },
  unknown: { background: '#8e8f98', border: '#c5c6cc', text: '#111217' },
};

const ORIGIN_LABELS: Record<ExecutionProfileOrigin, string> = {
  user: 'User',
  library: 'Library',
  system: 'System',
  unknown: 'Unknown',
};

export const ExecutionProfileView: FC<ExecutionProfileViewProps> = ({ run }) => {
  const [search, setSearch] = useState('');
  const [colorMode, setColorMode] =
    useState<ExecutionProfileColorMode>('function');
  const [includeSystemCalls, setIncludeSystemCalls] = useState(false);
  const [selectedFunctionKey, setSelectedFunctionKey] = useState<string | null>(
    null,
  );

  const baseProfile = useMemo(
    () => buildExecutionProfileProjection(run),
    [run?.boundaryId, run?.cursor],
  );
  const visibleProfile = useMemo(
    () =>
      filterExecutionProfileProjection(baseProfile, {
        includeSystemCalls,
      }),
    [baseProfile, includeSystemCalls],
  );
  const searchFunctionKeys = useMemo(
    () => executionProfileSearchFunctionKeys(visibleProfile, search),
    [visibleProfile, search],
  );
  const tracks = useMemo(
    () => buildProfileDepthTracks(visibleProfile.blocks),
    [visibleProfile.blocks],
  );
  const selectedVisible =
    selectedFunctionKey == null ||
    visibleProfile.functionRows.some((row) => row.functionKey === selectedFunctionKey);
  const activeFunctionKeys = useMemo<ActiveProfileFunctions>(() => {
    if (search.trim().length > 0) return new Set(searchFunctionKeys);
    if (selectedVisible && selectedFunctionKey != null) {
      return new Set([selectedFunctionKey]);
    }
    return null;
  }, [search, searchFunctionKeys, selectedVisible, selectedFunctionKey]);

  if (baseProfile.blocks.length === 0) {
    return (
      <div className="flex-1 flex items-center justify-center text-vsc-text-faint text-xs bg-vsc-bg">
        No profile yet
      </div>
    );
  }

  return (
    <div className="flex-1 min-h-0 flex flex-col bg-vsc-bg font-vsc-mono text-xs">
      <div className="shrink-0 flex items-center gap-2 border-b border-vsc-border bg-vsc-surface px-2 py-1.5">
        <div className="relative w-56 shrink-0">
          <Search className="absolute left-2 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-vsc-text-faint" />
          <Input
            value={search}
            onChange={(event) => setSearch(event.target.value)}
            placeholder="Search functions"
            className="h-7 rounded pl-7 pr-2 text-xs"
          />
        </div>
        <span className="text-[11px] text-vsc-text-muted">Mode:</span>
        <ToggleGroup
          value={colorMode}
          onValueChange={setColorMode}
          options={PROFILE_COLOR_MODE_OPTIONS}
          size="sm"
          className="rounded border border-vsc-border-subtle bg-vsc-bg"
        />
        <label className="ml-2 flex items-center gap-1.5 text-[11px] text-vsc-text-muted">
          <input
            type="checkbox"
            checked={includeSystemCalls}
            onChange={(event) => setIncludeSystemCalls(event.target.checked)}
            className="h-3.5 w-3.5 accent-vsc-accent"
          />
          Include system calls
        </label>
        {colorMode === 'origin' && <OriginLegend />}
        <div className="ml-auto text-[10px] text-vsc-text-faint">
          {visibleProfile.blocks.length} calls
          {visibleProfile.totalDurationMs != null
            ? ` / ${formatProfileMs(visibleProfile.totalDurationMs)}`
            : ''}
        </div>
      </div>

      {visibleProfile.blocks.length === 0 ? (
        <div className="flex-1 flex items-center justify-center text-vsc-text-faint">
          No visible calls
        </div>
      ) : (
        <div className="grid flex-1 min-h-0 grid-cols-[minmax(320px,36%)_minmax(420px,1fr)]">
          <ProfileFunctionTable
            rows={visibleProfile.functionRows}
            activeFunctionKeys={activeFunctionKeys}
            selectedFunctionKey={selectedVisible ? selectedFunctionKey : null}
            maxSelfMs={visibleProfile.maxSelfMs}
            maxTotalMs={visibleProfile.maxTotalMs}
            onSelectFunction={setSelectedFunctionKey}
          />
          <ProfileFlameChart
            tracks={tracks}
            colorMode={colorMode}
            activeFunctionKeys={activeFunctionKeys}
            selectedFunctionKey={selectedVisible ? selectedFunctionKey : null}
            onSelectFunction={setSelectedFunctionKey}
          />
        </div>
      )}
    </div>
  );
};

const ProfileFunctionTable: FC<{
  rows: ExecutionProfileFunctionRow[];
  activeFunctionKeys: ActiveProfileFunctions;
  selectedFunctionKey: string | null;
  maxSelfMs: number;
  maxTotalMs: number;
  onSelectFunction: (functionKey: string | null) => void;
}> = ({
  rows,
  activeFunctionKeys,
  selectedFunctionKey,
  maxSelfMs,
  maxTotalMs,
  onSelectFunction,
}) => (
  <div className="min-h-0 overflow-auto border-r border-vsc-border bg-vsc-bg">
    <div className="sticky top-0 z-10 grid grid-cols-[minmax(140px,1fr)_76px_92px_92px] border-b border-vsc-border bg-vsc-surface text-[10px] font-semibold uppercase tracking-wide text-vsc-text-muted">
      <div className="px-2 py-1.5">Function</div>
      <div className="px-2 py-1.5">Origin</div>
      <div className="px-2 py-1.5">Self Time</div>
      <div className="px-2 py-1.5">Total Time</div>
    </div>
    {rows.map((row) => {
      const manuallySelected = selectedFunctionKey === row.functionKey;
      const active = isProfileFunctionActive(row.functionKey, activeFunctionKeys);
      const highlighted = activeFunctionKeys?.has(row.functionKey) ?? false;
      return (
        <button
          key={row.functionKey}
          type="button"
          onClick={() =>
            onSelectFunction(manuallySelected ? null : row.functionKey)
          }
          className={cn(
            'grid w-full grid-cols-[minmax(140px,1fr)_76px_92px_92px] items-center border-b border-vsc-border-subtle text-left text-[11px] text-vsc-text hover:bg-vsc-surface',
            highlighted && 'bg-vsc-accent/15',
            !active && 'opacity-40',
          )}
        >
          <div className="min-w-0 px-2 py-1.5">
            <div className="truncate">{row.functionName}</div>
            {row.callCount > 1 && (
              <div className="text-[10px] text-vsc-text-faint">
                {row.callCount} calls
              </div>
            )}
          </div>
          <div className="px-2 py-1.5">
            <OriginBadge origin={row.origin} />
          </div>
          <MetricCell value={row.selfMs} max={maxSelfMs} />
          <MetricCell value={row.totalMs} max={maxTotalMs} />
        </button>
      );
    })}
  </div>
);

const ProfileFlameChart: FC<{
  tracks: ProfileDepthTrack[];
  colorMode: ExecutionProfileColorMode;
  activeFunctionKeys: ActiveProfileFunctions;
  selectedFunctionKey: string | null;
  onSelectFunction: (functionKey: string | null) => void;
}> = ({
  tracks,
  colorMode,
  activeFunctionKeys,
  selectedFunctionKey,
  onSelectFunction,
}) => (
  <div className="min-h-0 overflow-auto bg-vsc-bg">
    <div className="min-w-[760px] p-2 space-y-1.5">
      {tracks.map((track) => (
        <div
          key={track.depth}
          className="grid grid-cols-[36px_minmax(0,1fr)] gap-2 items-start"
        >
          <div className="pt-1 text-right text-[10px] text-vsc-text-faint">
            {track.depth}
          </div>
          <div
            className="relative rounded bg-vsc-surface border border-vsc-border-subtle overflow-hidden"
            style={{ height: track.laneCount * 24 + 4 }}
          >
            {track.blocks.map((block) => {
              const color = profileColorForBlock(block, colorMode);
              const active = isProfileFunctionActive(block.functionKey, activeFunctionKeys);
              return (
                <button
                  key={block.id}
                  type="button"
                  className={cn(
                    'absolute h-5 rounded border px-1.5 flex items-center gap-1 overflow-hidden shadow-sm text-left',
                    block.status === 'running' && 'opacity-75',
                    block.status !== 'ok' &&
                      block.status !== 'running' &&
                      'border-dashed',
                    !active && 'opacity-40',
                  )}
                  style={{
                    left: `${block.spanLeftPct}%`,
                    width: `${block.spanWidthPct}%`,
                    top: block.lane * 24 + 2,
                    backgroundColor: color.background,
                    borderColor: color.border,
                    color: color.text,
                  }}
                  title={`${block.functionName} | ${ORIGIN_LABELS[block.origin]} | ${block.threadLabel} ${block.threadId} | ${block.status} | self ${formatProfileMs(block.selfMs)} | total ${formatProfileMs(block.durationMs)}`}
                  onClick={() =>
                    onSelectFunction(
                      selectedFunctionKey === block.functionKey
                        ? null
                        : block.functionKey,
                    )
                  }
                >
                  <span className="shrink-0 rounded bg-black/20 px-1 text-[9px] leading-3">
                    {block.threadLabel}
                  </span>
                  <span className="truncate">{block.functionName}</span>
                  <span className="shrink-0 text-[10px] opacity-80">
                    {formatProfileMs(block.durationMs)}
                  </span>
                </button>
              );
            })}
          </div>
        </div>
      ))}
    </div>
  </div>
);

const MetricCell: FC<{ value: number; max: number }> = ({ value, max }) => {
  const width = max > 0 ? Math.max(2, Math.min(100, (value / max) * 100)) : 0;
  return (
    <div className="px-2 py-1.5">
      <div className="relative h-5 overflow-hidden rounded bg-vsc-surface">
        <div
          className="absolute inset-y-0 left-0 bg-vsc-text-muted/35"
          style={{ width: `${width}%` }}
        />
        <div className="absolute inset-0 flex items-center px-1 text-[10px] text-vsc-text">
          {formatProfileMs(value)}
        </div>
      </div>
    </div>
  );
};

const OriginBadge: FC<{ origin: ExecutionProfileOrigin }> = ({ origin }) => {
  const color = ORIGIN_COLORS[origin];
  return (
    <span
      className="inline-flex max-w-full rounded px-1.5 py-0.5 text-[10px] font-semibold"
      style={{ backgroundColor: color.border, color: '#151515' }}
    >
      {ORIGIN_LABELS[origin]}
    </span>
  );
};

const OriginLegend: FC = () => (
  <div className="flex items-center gap-2 text-[10px] text-vsc-text-faint">
    {(['user', 'library', 'system', 'unknown'] as ExecutionProfileOrigin[]).map(
      (origin) => {
        const color = ORIGIN_COLORS[origin];
        return (
          <span key={origin} className="inline-flex items-center gap-1">
            <span
              className="h-2.5 w-2.5 rounded-sm"
              style={{ backgroundColor: color.background }}
            />
            {ORIGIN_LABELS[origin]}
          </span>
        );
      },
    )}
  </div>
);

function isProfileFunctionActive(
  functionKey: string,
  activeFunctionKeys: ActiveProfileFunctions,
): boolean {
  return activeFunctionKeys == null || activeFunctionKeys.has(functionKey);
}

function buildProfileDepthTracks(blocks: ExecutionProfileBlock[]): ProfileDepthTrack[] {
  const blocksByDepth = new Map<number, ExecutionProfileBlock[]>();
  const blockOrder = new Map(blocks.map((block, index) => [block.id, index]));
  for (const block of blocks) {
    const depthBlocks = blocksByDepth.get(block.depth);
    if (depthBlocks) {
      depthBlocks.push(block);
    } else {
      blocksByDepth.set(block.depth, [block]);
    }
  }

  return [...blocksByDepth.entries()]
    .sort(([leftDepth], [rightDepth]) => leftDepth - rightDepth)
    .map(([depth, depthBlocks]) => {
      const laneEnds: number[] = [];
      const positionedBlocks = [...depthBlocks]
        .sort(
          (left, right) =>
            left.spanLeftPct - right.spanLeftPct ||
            (blockOrder.get(left.id) ?? 0) - (blockOrder.get(right.id) ?? 0) ||
            left.id.localeCompare(right.id),
        )
        .map((block) => {
          const left = block.spanLeftPct;
          const right = Math.min(100, block.spanLeftPct + block.spanWidthPct);
          let lane = laneEnds.findIndex((end) => end <= left + 0.25);
          if (lane === -1) {
            lane = laneEnds.length;
          }
          laneEnds[lane] = right;
          return { ...block, lane };
        });

      return {
        depth,
        laneCount: Math.max(1, laneEnds.length),
        blocks: positionedBlocks,
      };
    });
}

function profileColorForBlock(
  block: ExecutionProfileBlock,
  mode: ExecutionProfileColorMode,
): ProfileColor {
  if (mode === 'origin') return ORIGIN_COLORS[block.origin];
  const key = executionProfileColorKey(block, mode);
  return PROFILE_PALETTE[stableHash(key) % PROFILE_PALETTE.length];
}

function stableHash(value: string): number {
  let hash = 0;
  for (let index = 0; index < value.length; index += 1) {
    hash = (hash * 31 + value.charCodeAt(index)) >>> 0;
  }
  return hash;
}

function formatProfileMs(value: number | null): string {
  if (value == null) return '';
  if (value < 1) return `${value.toFixed(2)}ms`;
  if (value < 100) return `${value.toFixed(1)}ms`;
  return `${Math.round(value)}ms`;
}
