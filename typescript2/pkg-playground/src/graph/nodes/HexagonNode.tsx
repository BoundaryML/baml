import { type NodeProps } from '@xyflow/react';
import { Repeat } from 'lucide-react';
import { type ComponentType, memo } from 'react';
import { getChrome, nodeBackground, nodeShadow, stateStyle } from '../constants';
import { depthScale } from '../lod';
import { useGraphThemeContext } from '../theme';
import type { WorkflowNodeData } from '../types';
import { NodeHandles } from './NodeHandles';

export const HexagonNode: ComponentType<NodeProps> = memo(({ data }) => {
  const d = data as WorkflowNodeData;
  const isHighlighted = d.selected;
  const theme = useGraphThemeContext();
  const chrome = getChrome(theme);
  const loop = chrome.loop;
  // Deeper nodes render smaller (semantic-zoom hierarchy).
  const s = depthScale(typeof d.depth === 'number' ? d.depth : 0);
  // Loops use the cyan accent regardless of state.
  const base = stateStyle(theme, d.executionState);
  const colors = {
    ...base,
    accent: loop.accent,
    border: isHighlighted ? chrome.selectionRing.color : loop.border,
  };

  return (
    <>
      <NodeHandles />
      <div
        style={{
          display: 'flex',
          alignItems: 'center',
          gap: 8 * s,
          padding: `${7 * s}px ${11 * s}px ${7 * s}px ${9 * s}px`,
          borderRadius: 8,
          background: nodeBackground(colors, theme),
          border: `1px solid ${colors.border}`,
          boxShadow: nodeShadow(colors, !!isHighlighted, theme),
          width: '100%',
          height: '100%',
          boxSizing: 'border-box',
          color: colors.text,
          fontFamily:
            'ui-sans-serif, system-ui, -apple-system, "Segoe UI", sans-serif',
          transition: 'box-shadow 120ms ease, border-color 120ms ease',
        }}
      >
        <div
          style={{
            width: 20 * s,
            height: 20 * s,
            borderRadius: 6,
            background: loop.chipBg,
            boxShadow: `inset 0 0 0 1px ${loop.chipRing}`,
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'center',
            flexShrink: 0,
          }}
        >
          <Repeat size={12 * s} color={loop.icon} />
        </div>
        <div
          style={{
            fontSize: 12 * s,
            fontWeight: 500,
            color: colors.text,
            flex: 1,
            minWidth: 0,
            overflow: 'hidden',
            textOverflow: 'ellipsis',
            whiteSpace: 'nowrap',
            letterSpacing: '-0.005em',
            lineHeight: 1.3,
          }}
        >
          {d.label}
        </div>
      </div>
    </>
  );
});

HexagonNode.displayName = 'HexagonNode';
