import { type NodeProps } from '@xyflow/react';
import { Repeat } from 'lucide-react';
import { type ComponentType, memo } from 'react';
import { selectionRing, stateColors } from '../constants';
import type { WorkflowNodeData } from '../types';
import { NodeHandles } from './NodeHandles';

/**
 * Visual frame for a subgraph (function root, branch arm, loop body, scope).
 *
 * Design intent:
 *  - Stays out of the way when not selected (1px, subtle border, transparent fill).
 *  - Nested groups stack visually because each layer adds a small +alpha tint —
 *    nesting depth is encoded in the canvas itself, no per-depth styling needed.
 *  - Floating label chip with backdrop blur reads cleanly over edges/nodes.
 */
export const GroupNode: ComponentType<NodeProps> = memo(({ data, id }) => {
  const d = data as WorkflowNodeData;
  const isHighlighted = d.selected;
  const stateStyle = stateColors[d.executionState] ?? stateColors['not-started'];
  const stateAccent = stateStyle.accent;
  const isStateful = d.executionState !== 'not-started' && d.executionState !== 'skipped';

  const borderColor = isHighlighted
    ? selectionRing.color
    : isStateful
      ? stateStyle.border
      : 'rgba(255,255,255,0.10)';

  return (
    <div
      style={{
        width: '100%',
        height: '100%',
        position: 'relative',
        pointerEvents: 'none',
        borderRadius: 12,
        // Frame-only: no fill, so the canvas reads through. Nested groups
        // never stack into a darker patch — they just compose hairline frames.
        background: 'transparent',
        border: `1px solid ${borderColor}`,
        boxShadow: isHighlighted
          ? `0 0 0 1px ${selectionRing.glow}`
          : undefined,
        transition: 'border-color 150ms ease, box-shadow 150ms ease',
      }}
    >
      <NodeHandles />

      {/* Floating label chip — sits on the top edge of the frame */}
      <div
        style={{
          position: 'absolute',
          top: 0,
          left: 14,
          transform: 'translateY(-50%)',
          zIndex: 5,
          pointerEvents: 'auto',
          display: 'inline-flex',
          alignItems: 'center',
          gap: 6,
          whiteSpace: 'nowrap',
          padding: '3px 10px',
          borderRadius: 999,
          fontWeight: 600,
          fontSize: 11,
          letterSpacing: '-0.005em',
          color: isHighlighted ? '#ecfeff' : '#e4e4e7',
          background: isHighlighted
            ? 'rgba(8,47,73,0.85)'
            : 'rgba(24,24,27,0.85)',
          border: `1px solid ${isHighlighted ? selectionRing.color : 'rgba(255,255,255,0.10)'}`,
          backdropFilter: 'blur(8px)',
          WebkitBackdropFilter: 'blur(8px)',
          boxShadow: isHighlighted
            ? `0 0 0 2px ${selectionRing.glow}, 0 1px 2px rgba(0,0,0,0.4)`
            : '0 1px 2px rgba(0,0,0,0.4)',
          transition: 'all 150ms ease',
          fontFamily: 'ui-sans-serif, system-ui, -apple-system, "Segoe UI", sans-serif',
        }}
      >
        {/* State accent dot (only for stateful runs) */}
        {isStateful && (
          <span
            style={{
              width: 6,
              height: 6,
              borderRadius: '50%',
              background: stateAccent,
              boxShadow: `0 0 6px ${stateAccent}`,
              flexShrink: 0,
            }}
          />
        )}
        <span>{d.label || id}</span>
        {(d.iterationCount ?? 0) > 0 && (
          <span
            style={{
              display: 'inline-flex',
              alignItems: 'center',
              gap: 3,
              marginLeft: 2,
              padding: '1px 6px',
              borderRadius: 999,
              background: 'rgba(59,130,246,0.18)',
              color: '#93c5fd',
              fontSize: 10,
              fontWeight: 600,
              fontVariantNumeric: 'tabular-nums',
              border: '1px solid rgba(59,130,246,0.30)',
            }}
          >
            <Repeat size={9} strokeWidth={2.5} />
            {d.iterationCount}
          </span>
        )}
      </div>
    </div>
  );
});

GroupNode.displayName = 'GroupNode';
