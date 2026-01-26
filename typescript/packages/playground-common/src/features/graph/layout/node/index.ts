import type { Reactflow } from '../../../../mock-data/types';
import {
  type ELKMermaidAlgorithm,
  layoutELKMermaid,
} from './algorithms/elk-mermaid';

export type LayoutDirection = 'vertical' | 'horizontal';
export type LayoutVisibility = 'visible' | 'hidden';

export interface LayoutSpacing {
  x: number;
  y: number;
}

// Simplified config - Mermaid defaults only
export type ReactflowLayoutConfig = {
  visibility: LayoutVisibility;
};

export type LayoutAlgorithmProps = Reactflow & {
  direction: LayoutDirection;
  spacing: LayoutSpacing;
  visibility: LayoutVisibility;
};

export type ILayoutReactflow = Reactflow & Partial<ReactflowLayoutConfig> & {
  direction?: LayoutDirection;
};

// Defaults - horizontal layout
const MERMAID_DEFAULTS = {
  direction: 'horizontal' as LayoutDirection,
  spacing: { x: 50, y: 50 },
  algorithm: 'elk.layered' as ELKMermaidAlgorithm,
};

export const layoutReactflow = async (
  options: ILayoutReactflow,
): Promise<Reactflow> => {
  const { nodes = [], edges = [], visibility = 'visible', direction = MERMAID_DEFAULTS.direction } = options;

  // Use Mermaid's ELK layout with defaults
  const result = await layoutELKMermaid({
    nodes,
    edges,
    direction,
    spacing: MERMAID_DEFAULTS.spacing,
    visibility,
    algorithm: MERMAID_DEFAULTS.algorithm,
  });

  if (!result) {
    console.error('❌ Layout failed');
    // Return nodes at origin if layout fails
    return {
      nodes: nodes.map((node) => ({ ...node, position: { x: 0, y: 0 } })),
      edges,
    };
  }

  return result;
};
