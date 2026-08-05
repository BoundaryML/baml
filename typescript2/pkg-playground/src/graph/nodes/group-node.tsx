import type { NodeProps } from '@xyflow/react';
import { Repeat } from 'lucide-react';
import { type ComponentType, memo, useState } from 'react';
import { getChrome, stateStyle } from '../constants';
import { depthScale } from '../lod';
import { useGraphThemeContext } from '../theme';
import type { WorkflowNodeData } from '../types';
import { NodeHandles } from './NodeHandles';
import { NodeOutputPreview } from './NodeOutputPreview';

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
  const label = d.label || id;
  const iterationCount = d.iterationCount ?? 0;
  const title =
    iterationCount > 0
      ? `${label} (${iterationCount} iteration${iterationCount === 1 ? '' : 's'})`
      : label;
  const [isLabelHovered, setIsLabelHovered] = useState(false);
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
  const hasValuePreviews = (d.valuePreviews?.length ?? 0) > 0;

  const borderColor = isHighlighted
    ? chrome.selectionRing.color
    : isStateful
      ? style.border
      : chrome.groupBorderIdle;

  return (
    <div
      style={{
        // Frame-only: no fill, so the canvas reads through. Nested groups
        // never stack into a darker patch — they just compose hairline frames.
        background: 'transparent',
        border: `1.5px ${isStateful || isHighlighted ? 'solid' : 'dashed'} ${borderColor}`,
        borderRadius: 12,
        boxShadow: isHighlighted
          ? `0 0 0 1px ${chrome.selectionRing.glow}`
          : undefined,
        height: '100%',
        pointerEvents: 'none',
        position: 'relative',
        transition: 'border-color 150ms ease, box-shadow 150ms ease',
        width: '100%',
      }}
    >
      <NodeHandles />

      {/* Floating label chip — sits on the top edge of the frame */}
      <div
        className="baml-graph-group-label"
        onMouseEnter={() => setIsLabelHovered(true)}
        onMouseLeave={() => setIsLabelHovered(false)}
        style={{
          alignItems: 'center',
          backdropFilter: 'blur(8px)',
          background: isHighlighted
            ? chrome.groupLabelBgSelected
            : chrome.groupLabelBg,
          border: `1px solid ${isHighlighted ? chrome.selectionRing.color : chrome.groupLabelBorder}`,
          borderRadius: 999,
          boxShadow: isHighlighted
            ? `0 0 0 2px ${chrome.selectionRing.glow}, ${chrome.groupLabelShadow}`
            : chrome.groupLabelShadow,
          boxSizing: 'border-box',
          color: isHighlighted
            ? chrome.groupLabelTextSelected
            : chrome.groupLabelText,
          // Manually-expanded containers collapse on click of this chip.
          cursor: expandable ? 'zoom-out' : 'pointer',
          display: 'inline-flex',
          fontFamily:
            'ui-sans-serif, system-ui, -apple-system, "Segoe UI", sans-serif',
          fontSize: 11 * s,
          fontWeight: 600,
          gap: 6,
          left: 14,
          letterSpacing: '-0.005em',
          maxWidth: isLabelHovered ? 'none' : 'calc(100% - 28px)',
          overflow: isLabelHovered ? 'visible' : 'hidden',
          padding: `${3 * s}px ${10 * s}px`,
          pointerEvents: 'auto',
          position: 'absolute',
          top: 0,
          transform: 'translateY(-50%)',
          transition: 'all 150ms ease',
          WebkitBackdropFilter: 'blur(8px)',
          whiteSpace: 'nowrap',
          width: 'max-content',
          zIndex: 5,
        }}
        title={title}
      >
        {/* State accent dot (only for stateful runs) */}
        {isStateful && (
          <span
            style={{
              background: stateAccent,
              borderRadius: '50%',
              boxShadow: `0 0 6px ${stateAccent}`,
              flexShrink: 0,
              height: 6,
              width: 6,
            }}
          />
        )}
        <span
          className="baml-graph-group-label__text"
          style={{
            flex: '1 1 auto',
            minWidth: 0,
            overflow: isLabelHovered ? 'visible' : 'hidden',
            textOverflow: isLabelHovered ? 'clip' : 'ellipsis',
          }}
        >
          {label}
        </span>
        {expandable && (
          <span
            aria-hidden
            style={{
              alignItems: 'center',
              border: `1px solid ${chrome.groupLabelBorder}`,
              borderRadius: '50%',
              color: chrome.groupLabelText,
              display: 'inline-flex',
              flexShrink: 0,
              fontSize: 13,
              fontWeight: 700,
              height: 14,
              justifyContent: 'center',
              lineHeight: 1,
              width: 14,
            }}
            title="Click to collapse"
          >
            {'−'}
          </span>
        )}
        {iterationCount > 0 && (
          <span
            style={{
              alignItems: 'center',
              background: chrome.iterationBg,
              border: `1px solid ${chrome.iterationBorder}`,
              borderRadius: 999,
              color: chrome.iterationText,
              display: 'inline-flex',
              flexShrink: 0,
              fontSize: 10,
              fontVariantNumeric: 'tabular-nums',
              fontWeight: 600,
              gap: 3,
              marginLeft: 2,
              padding: '1px 6px',
            }}
          >
            <Repeat size={9} strokeWidth={2.5} />
            {iterationCount}
          </span>
        )}
      </div>
      {hasValuePreviews ? (
        <div
          style={{
            left: 14,
            pointerEvents: 'auto',
            position: 'absolute',
            top: 16,
            zIndex: 4,
          }}
        >
          <NodeOutputPreview
            customRenderers={d.customRenderers}
            valuePreviews={d.valuePreviews}
          />
        </div>
      ) : null}
    </div>
  );
});

GroupNode.displayName = 'GroupNode';
