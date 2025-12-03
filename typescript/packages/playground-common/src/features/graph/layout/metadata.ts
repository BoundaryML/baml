import { MarkerType, Position } from '@xyflow/react';

import type {
  Reactflow,
  ReactflowEdgeWithData,
  ReactflowNodeWithData,
} from '../../../mock-data/types';
import type { LayoutDirection, LayoutVisibility } from './node';

export const getRootNode = (nodes: Reactflow['nodes']) => {
  return nodes.find((e) => e.type === 'start') ?? nodes[0];
};

export const getNodeSize = (
  node: ReactflowNodeWithData,
  defaultSize = { width: 150, height: 36 },
  allowDefaults = true,
) => {
  const nodeWith = node.measured?.width;
  const nodeHeight = node.measured?.height;
  const hasDimension = [nodeWith, nodeHeight].every((e) => e != null);

  return {
    hasDimension,
    width: nodeWith,
    height: nodeHeight,
    // Only use defaults if explicitly allowed (for ELK compatibility)
    widthWithDefault: allowDefaults ? (nodeWith ?? defaultSize.width) : nodeWith,
    heightWithDefault: allowDefaults ? (nodeHeight ?? defaultSize.height) : nodeHeight,
  };
};

export type IFixPosition = (pros: {
  x: number;
  y: number;
  width: number;
  height: number;
}) => {
  x: number;
  y: number;
};

export const getNodeLayouted = (props: {
  node: ReactflowNodeWithData;
  position: { x: number; y: number };
  direction: LayoutDirection;
  visibility: LayoutVisibility;
  fixPosition?: IFixPosition;
}): ReactflowNodeWithData => {
  const {
    node,
    position,
    direction,
    visibility,
    fixPosition = (p) => ({ x: p.x, y: p.y }),
  } = props;

  const hidden = visibility !== 'visible';
  const isHorizontal = direction === 'horizontal';
  const { width, height, widthWithDefault, heightWithDefault } =
    getNodeSize(node);

  // On first pass (hidden), don't set width/height for regular nodes to allow natural sizing
  // But ALWAYS set width/height for groups since ELK calculates them based on children
  // On second pass (visible), use measured dimensions for all nodes
  const isGroup = (node as any).isGroup;
  const shouldSetDimensions = !hidden || isGroup;

  const nodeWithDimensions = shouldSetDimensions
    ? {
      ...node,
      // Use measured dimensions (or ELK-calculated for groups)
      width,
      height,
    }
    : {
      ...node,
      // Don't set width/height properties - let content determine size
      // Remove any existing width/height to allow natural measurement
    };

  return {
    ...nodeWithDimensions,
    // Preserve the node type (base, diamond, hexagon, group) instead of hardcoding 'base'
    type: node.type,
    position: fixPosition({
      ...position,
      width: widthWithDefault ?? 150,
      height: heightWithDefault ?? 36,
    }),
    data: {
      ...node.data,
      // Keep the existing label from node.data, don't override with ID
      // Store direction so nodes can render handles correctly
      direction,
    },
    style: {
      ...node.style,
      visibility: hidden ? 'hidden' : 'visible',
    },
    targetPosition: isHorizontal ? Position.Left : Position.Top,
    sourcePosition: isHorizontal ? Position.Right : Position.Bottom,
  };
};

export const getEdgeLayouted = (props: {
  edge: ReactflowEdgeWithData;
  visibility: LayoutVisibility;
}): ReactflowEdgeWithData => {
  const { edge, visibility } = props;
  const hidden = visibility !== 'visible';
  return {
    ...edge,
    type: 'base',
    markerEnd: {
      type: MarkerType.ArrowClosed,
    },
    style: {
      ...edge.style,
      visibility: hidden ? 'hidden' : 'visible',
    },
  };
};
