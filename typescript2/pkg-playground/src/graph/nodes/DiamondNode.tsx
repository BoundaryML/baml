import { type NodeProps } from '@xyflow/react';
import { GitBranch } from 'lucide-react';
import { type ComponentType, memo } from 'react';
import { getChrome, nodeBackground, nodeShadow, stateStyle } from '../constants';
import { depthScale } from '../lod';
import { useGraphThemeContext } from '../theme';
import type { WorkflowNodeData } from '../types';
import { NodeHandles } from './NodeHandles';

export const DiamondNode: ComponentType<NodeProps> = memo(({ data }) => {
  const d = data as WorkflowNodeData;
  const isHighlighted = d.selected;
  const theme = useGraphThemeContext();
  const chrome = getChrome(theme);
  const branch = chrome.branch;
  // Deeper nodes render smaller (semantic-zoom hierarchy).
  const s = depthScale(typeof d.depth === 'number' ? d.depth : 0);
  // Conditionals are visually distinct via amber accent regardless of state.
  const base = stateStyle(theme, d.executionState);
  const colors = {
    ...base,
    accent: branch.accent,
    border: isHighlighted ? chrome.selectionRing.color : branch.border,
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
        <span
          style={{
            display: 'inline-flex',
            alignItems: 'center',
            justifyContent: 'center',
            width: 20 * s,
            height: 20 * s,
            borderRadius: 6,
            background: branch.chipBg,
            boxShadow: `inset 0 0 0 1px ${branch.chipRing}`,
          }}
        >
          <GitBranch
            size={12 * s}
            color={branch.icon}
            style={{ transform: 'rotate(180deg)' }}
          />
        </span>
        <div
          style={{
            fontSize: 12 * s,
            fontWeight: 500,
            color: colors.text,
            lineHeight: 1.3,
            letterSpacing: '-0.005em',
          }}
        >
          {d.label}
        </div>
      </div>
    </>
  );
});

DiamondNode.displayName = 'DiamondNode';
