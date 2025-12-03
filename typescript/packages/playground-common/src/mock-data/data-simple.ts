import type { Graph } from './types';

/**
 * Simple example with a diamond decision node
 * Perfect for testing node sizing and layout
 */
export const simpleData: Graph = {
  nodes: [
    { id: 'start', label: 'Start', kind: 'item' },
    { id: 'check', label: 'Valid?', kind: 'item', shape: 'diamond' },
    { id: 'process', label: 'Process Data', kind: 'item' },
    { id: 'end', label: 'End', kind: 'item' },
  ],
  edges: [
    { id: 'e1', from: 'start', to: 'check', style: 'solid' },
    { id: 'e2', from: 'check', to: 'process', style: 'solid' },
    { id: 'e3', from: 'process', to: 'end', style: 'solid' },
  ],
};