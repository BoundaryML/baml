import { type NodeProps } from '@xyflow/react';
import { type ComponentType, memo } from 'react';
import { nodeBackground, nodeShadow, selectionRing, stateColors } from '../constants';
import type { WorkflowNodeData } from '../types';
import { NodeHandles } from './NodeHandles';
import { NodeOutputPreview } from './NodeOutputPreview';

const StateIcon = ({ state }: { state: string }) => {
  if (state === 'running') {
    return (
      <svg
        width="11"
        height="11"
        viewBox="0 0 24 24"
        fill="none"
        stroke="white"
        strokeWidth="3"
        style={{ animation: 'baml-graph-spin 800ms linear infinite', transformOrigin: 'center' }}
      >
        <circle cx="12" cy="12" r="10" opacity="0.25" />
        <path d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z" fill="white" opacity="0.85" />
      </svg>
    );
  }
  if (state === 'success') {
    return (
      <svg width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="white" strokeWidth="3.5" strokeLinecap="round" strokeLinejoin="round">
        <path d="M5 13l4 4L19 7" />
      </svg>
    );
  }
  if (state === 'error') {
    return (
      <svg width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="white" strokeWidth="3" strokeLinecap="round">
        <path d="M6 18L18 6M6 6l12 12" />
      </svg>
    );
  }
  if (state === 'cancelled') {
    return (
      <svg width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="white" strokeWidth="3.5" strokeLinecap="round">
        <path d="M6 12h12" />
      </svg>
    );
  }
  // Idle: a subtle dot — keeps visual rhythm without competing for attention
  return (
    <svg width="11" height="11" viewBox="0 0 24 24" fill="none">
      <circle cx="12" cy="12" r="3" fill="rgba(255,255,255,0.85)" />
    </svg>
  );
};

export const BaseNode: ComponentType<NodeProps> = memo(({ data }) => {
  const d = data as WorkflowNodeData;
  const isHighlighted = d.selected;
  const colors = stateColors[d.executionState] ?? stateColors['not-started'];

  return (
    <>
      <NodeHandles />
      <div
        style={{
          display: 'flex',
          flexDirection: 'column',
          alignItems: 'stretch',
          gap: 6,
          padding: '7px 11px 7px 9px',
          borderRadius: 8,
          background: nodeBackground(colors),
          border: `1px solid ${isHighlighted ? selectionRing.color : colors.border}`,
          boxShadow: nodeShadow(colors, !!isHighlighted),
          width: '100%',
          height: '100%',
          boxSizing: 'border-box',
          color: colors.text,
          fontFamily: 'ui-sans-serif, system-ui, -apple-system, "Segoe UI", sans-serif',
          transition: 'box-shadow 120ms ease, border-color 120ms ease',
        }}
      >
        <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
          <div
            style={{
              width: 20,
              height: 20,
              borderRadius: 6,
              background: colors.accent,
              boxShadow: `inset 0 1px 0 rgba(255,255,255,0.18), 0 1px 2px rgba(0,0,0,0.25)`,
              display: 'flex',
              alignItems: 'center',
              justifyContent: 'center',
              flexShrink: 0,
            }}
          >
            <StateIcon state={d.executionState} />
          </div>
          <div
            style={{
              fontSize: 12,
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
        <NodeOutputPreview
          result={d.result}
          hasResult={d.hasResult}
          images={d.imageOutputs}
          errorMessage={d.errorMessage}
          customRenderers={d.customRenderers}
        />
      </div>
    </>
  );
});

BaseNode.displayName = 'BaseNode';
