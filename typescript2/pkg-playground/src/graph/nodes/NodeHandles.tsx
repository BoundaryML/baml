import { Handle, Position } from '@xyflow/react';

/**
 * 8 invisible handles (4 directions × source/target) so that layoutGraph
 * can pick the optimal exit/entry face per edge based on node positions.
 *
 * Handle IDs follow the pattern: `{direction}-{type}`.
 * Edges reference them via `sourceHandle` / `targetHandle`.
 */

const S = { opacity: 0, width: 6, height: 6 } as const;

export function NodeHandles() {
  return (
    <>
      <Handle
        id="left-target"
        type="target"
        position={Position.Left}
        style={S}
      />
      <Handle
        id="right-target"
        type="target"
        position={Position.Right}
        style={S}
      />
      <Handle id="top-target" type="target" position={Position.Top} style={S} />
      <Handle
        id="bottom-target"
        type="target"
        position={Position.Bottom}
        style={S}
      />
      <Handle
        id="left-source"
        type="source"
        position={Position.Left}
        style={S}
      />
      <Handle
        id="right-source"
        type="source"
        position={Position.Right}
        style={S}
      />
      <Handle id="top-source" type="source" position={Position.Top} style={S} />
      <Handle
        id="bottom-source"
        type="source"
        position={Position.Bottom}
        style={S}
      />
    </>
  );
}
