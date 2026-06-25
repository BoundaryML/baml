import { type NodeProps } from '@xyflow/react';
import { Repeat } from 'lucide-react';
import { type ComponentType, memo } from 'react';
import { getChrome, stateStyle } from '../constants';
import { depthScale } from '../lod';
import { useGraphThemeContext } from '../theme';
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
  // Set by the level-of-detail pass when the user expanded this container.
  const expandable = d.expanded === true;
  // Deeper containers get a smaller label chip (semantic-zoom hierarchy).
  const s = depthScale(typeof d.depth === 'number' ? d.depth : 0);
  const theme = useGraphThemeContext();
  const chrome = getChrome(theme);
  const style = stateStyle(theme, d.executionState);
  const stateAccent = style.accent;
  const isStateful =
    d.executionState !== 'not-started' && d.executionState !== 'skipped';

  const borderColor = isHighlighted
    ? chrome.selectionRing.color
    : isStateful
      ? style.border
      : chrome.groupBorderIdle;

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
        border: `1.5px ${isStateful || isHighlighted ? 'solid' : 'dashed'} ${borderColor}`,
        boxShadow: isHighlighted
          ? `0 0 0 1px ${chrome.selectionRing.glow}`
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
          // Manually-expanded containers collapse on click of this chip.
          cursor: expandable ? 'zoom-out' : 'pointer',
          display: 'inline-flex',
          alignItems: 'center',
          gap: 6,
          whiteSpace: 'nowrap',
          padding: `${3 * s}px ${10 * s}px`,
          borderRadius: 999,
          fontWeight: 600,
          fontSize: 11 * s,
          letterSpacing: '-0.005em',
          color: isHighlighted
            ? chrome.groupLabelTextSelected
            : chrome.groupLabelText,
          background: isHighlighted
            ? chrome.groupLabelBgSelected
            : chrome.groupLabelBg,
          border: `1px solid ${isHighlighted ? chrome.selectionRing.color : chrome.groupLabelBorder}`,
          backdropFilter: 'blur(8px)',
          WebkitBackdropFilter: 'blur(8px)',
          boxShadow: isHighlighted
            ? `0 0 0 2px ${chrome.selectionRing.glow}, ${chrome.groupLabelShadow}`
            : chrome.groupLabelShadow,
          transition: 'all 150ms ease',
          fontFamily:
            'ui-sans-serif, system-ui, -apple-system, "Segoe UI", sans-serif',
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
        {expandable && (
          <span
            aria-hidden
            title="Click to collapse"
            style={{
              display: 'inline-flex',
              alignItems: 'center',
              justifyContent: 'center',
              width: 14,
              height: 14,
              borderRadius: '50%',
              fontSize: 13,
              lineHeight: 1,
              fontWeight: 700,
              color: chrome.groupLabelText,
              border: `1px solid ${chrome.groupLabelBorder}`,
            }}
          >
            {'−'}
          </span>
        )}
        {(d.iterationCount ?? 0) > 0 && (
          <span
            style={{
              display: 'inline-flex',
              alignItems: 'center',
              gap: 3,
              marginLeft: 2,
              padding: '1px 6px',
              borderRadius: 999,
              background: chrome.iterationBg,
              color: chrome.iterationText,
              fontSize: 10,
              fontWeight: 600,
              fontVariantNumeric: 'tabular-nums',
              border: `1px solid ${chrome.iterationBorder}`,
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
