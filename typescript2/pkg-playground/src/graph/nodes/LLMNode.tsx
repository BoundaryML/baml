import { type NodeProps } from '@xyflow/react';
import { Sparkles } from 'lucide-react';
import { type ComponentType, memo } from 'react';
import { getChrome, nodeBackground, nodeShadow, stateStyle } from '../constants';
import { depthScale } from '../lod';
import { useGraphThemeContext } from '../theme';
import type { WorkflowNodeData } from '../types';
import { NodeHandles } from './NodeHandles';
import { NodeOutputPreview } from './NodeOutputPreview';

export const LLMNode: ComponentType<NodeProps> = memo(({ data }) => {
  const d = data as WorkflowNodeData;
  const isHighlighted = d.selected;
  const isRunning = d.executionState === 'running';
  const theme = useGraphThemeContext();
  const chrome = getChrome(theme);
  const llm = chrome.llm;
  // Deeper nodes render smaller (semantic-zoom hierarchy) — but not when a
  // preview is shown, so the content matches the (unshrunk) layout box.
  const hasPreview =
    (d.valuePreviews?.length ?? 0) > 0 || !!d.errorMessage || !!d.hasResult;
  const s = hasPreview
    ? 1
    : depthScale(typeof d.depth === 'number' ? d.depth : 0);
  // LLM nodes use a violet accent regardless of state — domain signal first,
  // execution state communicated through the gradient + border tint.
  const base = stateStyle(theme, d.executionState);
  const colors = {
    ...base,
    accent: llm.accent,
    border: isHighlighted ? chrome.selectionRing.color : llm.border,
  };

  return (
    <>
      <NodeHandles />
      <div
        style={{
          display: 'flex',
          flexDirection: 'column',
          gap: 4 * s,
          padding: `${7 * s}px ${11 * s}px ${8 * s}px ${9 * s}px`,
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
        <div style={{ display: 'flex', alignItems: 'center', gap: 7 * s }}>
          <span
            style={{
              display: 'inline-flex',
              alignItems: 'center',
              gap: 4 * s,
              padding: `${2 * s}px ${6 * s}px ${2 * s}px ${5 * s}px`,
              borderRadius: 5,
              fontSize: 9 * s,
              fontWeight: 700,
              letterSpacing: '0.06em',
              background: llm.chipBg,
              color: llm.chipText,
              boxShadow: `inset 0 0 0 1px ${llm.chipRing}`,
            }}
          >
            <Sparkles
              size={9 * s}
              strokeWidth={2.5}
              style={
                isRunning
                  ? { animation: 'baml-graph-spin 900ms linear infinite' }
                  : undefined
              }
            />
            LLM
          </span>
          <span
            style={{
              fontSize: 12 * s,
              fontWeight: 600,
              color: colors.text,
              overflow: 'hidden',
              textOverflow: 'ellipsis',
              whiteSpace: 'nowrap',
              flex: 1,
              letterSpacing: '-0.005em',
            }}
          >
            {d.label}
          </span>
        </div>
        {d.llmClient && (
          <div
            style={{
              fontSize: 9 * s,
              color: colors.textMuted,
              overflow: 'hidden',
              textOverflow: 'ellipsis',
              whiteSpace: 'nowrap',
              fontVariantNumeric: 'tabular-nums',
              opacity: 0.85,
              paddingLeft: 1,
            }}
          >
            {d.llmClient}
          </div>
        )}
        <NodeOutputPreview
          result={d.result}
          hasResult={d.hasResult}
          valuePreviews={d.valuePreviews}
          errorMessage={d.errorMessage}
          customRenderers={d.customRenderers}
        />
      </div>
    </>
  );
});

LLMNode.displayName = 'LLMNode';
