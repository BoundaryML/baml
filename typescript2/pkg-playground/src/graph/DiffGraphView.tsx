import { useCallback, useRef, useState, type FC, type MouseEvent } from 'react';
import type { ControlFlowGraph, GraphDiffResult } from '../worker-protocol';
import type { Viewport } from '@xyflow/react';
import { GraphView } from './GraphView';

interface DiffGraphViewProps {
  baseGraph: ControlFlowGraph | null;
  headGraph: ControlFlowGraph | null;
  diff: GraphDiffResult;
  selectedNodeId: number | null;
  onNodeClick: (nodeId: number) => void;
  baseLabel?: string;
}

const EmptyPane: FC<{ label: string }> = ({ label }) => (
  <div style={{
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'center',
    height: '100%',
    color: '#6b7280',
    fontSize: 12,
  }}>
    {label}
  </div>
);

export const DiffGraphView: FC<DiffGraphViewProps> = ({
  baseGraph,
  headGraph,
  diff,
  selectedNodeId,
  onNodeClick,
  baseLabel = 'Base (main)',
}) => {
  const [splitRatio, setSplitRatio] = useState(0.5);
  const containerRef = useRef<HTMLDivElement>(null);

  // Viewport sync: when one pane pans/zooms, mirror to the other
  const baseViewportRef = useRef<((v: Viewport) => void) | null>(null);
  const headViewportRef = useRef<((v: Viewport) => void) | null>(null);
  const syncingRef = useRef(false); // prevent infinite loops

  const onBaseViewportChange = useCallback((viewport: Viewport) => {
    if (syncingRef.current) return;
    syncingRef.current = true;
    headViewportRef.current?.(viewport);
    syncingRef.current = false;
  }, []);

  const onHeadViewportChange = useCallback((viewport: Viewport) => {
    if (syncingRef.current) return;
    syncingRef.current = true;
    baseViewportRef.current?.(viewport);
    syncingRef.current = false;
  }, []);

  const onSplitterMouseDown = useCallback((e: MouseEvent) => {
    e.preventDefault();
    const container = containerRef.current;
    if (!container) return;

    const onMouseMove = (ev: globalThis.MouseEvent) => {
      const rect = container.getBoundingClientRect();
      const ratio = (ev.clientX - rect.left) / rect.width;
      setSplitRatio(Math.max(0.2, Math.min(0.8, ratio)));
    };
    const onMouseUp = () => {
      document.removeEventListener('mousemove', onMouseMove);
      document.removeEventListener('mouseup', onMouseUp);
      document.body.style.cursor = '';
      document.body.style.userSelect = '';
    };
    document.body.style.cursor = 'col-resize';
    document.body.style.userSelect = 'none';
    document.addEventListener('mousemove', onMouseMove);
    document.addEventListener('mouseup', onMouseUp);
  }, []);

  return (
    <div
      ref={containerRef}
      style={{ display: 'flex', flexDirection: 'column', width: '100%', height: '100%' }}
    >
      {/* Pane labels */}
      <div style={{ display: 'flex', flexShrink: 0 }}>
        <div
          style={{
            flex: splitRatio,
            minWidth: 0,
            padding: '3px 8px',
            fontSize: 10,
            color: '#9ca3af',
            borderBottom: '1px solid var(--vsc-border, rgba(255,255,255,0.08))',
            background: 'var(--vsc-bg-secondary, rgba(24,24,27,0.5))',
          }}
        >
          {baseLabel}
        </div>
        <div style={{ width: 4, flexShrink: 0 }} />
        <div
          style={{
            flex: 1 - splitRatio,
            minWidth: 0,
            padding: '3px 8px',
            fontSize: 10,
            color: '#9ca3af',
            borderBottom: '1px solid var(--vsc-border, rgba(255,255,255,0.08))',
            background: 'var(--vsc-bg-secondary, rgba(24,24,27,0.5))',
          }}
        >
          Current
        </div>
      </div>

      {/* Graph panes */}
      <div style={{ display: 'flex', flex: 1, minHeight: 0 }}>
        {/* Base (left) */}
        <div style={{ flex: splitRatio, minWidth: 0, position: 'relative' }}>
          {baseGraph ? (
            <GraphView
              graph={baseGraph}
              selectedNodeId={selectedNodeId}
              onNodeClick={onNodeClick}
              onViewportChange={onBaseViewportChange}
              viewportRef={baseViewportRef}
              diffAnnotations={{ diff, side: 'base' }}
            />
          ) : (
            <EmptyPane label="Function does not exist on base branch" />
          )}
        </div>

        {/* Draggable splitter */}
        <div
          onMouseDown={onSplitterMouseDown}
          style={{
            width: 4,
            flexShrink: 0,
            cursor: 'col-resize',
            background: 'rgba(255,255,255,0.06)',
            transition: 'background 120ms',
          }}
          onMouseEnter={(e) => { e.currentTarget.style.background = 'rgba(59,130,246,0.3)'; }}
          onMouseLeave={(e) => { e.currentTarget.style.background = 'rgba(255,255,255,0.06)'; }}
        />

        {/* Head (right) */}
        <div style={{ flex: 1 - splitRatio, minWidth: 0, position: 'relative' }}>
          {headGraph ? (
            <GraphView
              graph={headGraph}
              selectedNodeId={selectedNodeId}
              onNodeClick={onNodeClick}
              onViewportChange={onHeadViewportChange}
              viewportRef={headViewportRef}
              diffAnnotations={{ diff, side: 'head' }}
            />
          ) : (
            <EmptyPane label="Function was removed" />
          )}
        </div>
      </div>
    </div>
  );
};
