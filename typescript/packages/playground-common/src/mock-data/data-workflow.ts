import type { Graph } from './types';

/**
 * Example workflow with decision nodes (diamond) and loop nodes (hexagon)
 * Based on Mermaid flowchart syntax:
 * - {text} = diamond (decision/if)
 * - {{text}} = hexagon (loop/iteration)
 */
export const workflowData: Graph = {
  nodes: [
    // Main workflow
    { id: 'A', label: 'Start', kind: 'item' },
    { id: 'B', label: 'Fetch user data', kind: 'item' },
    { id: 'C', label: 'Is user active?', kind: 'item', shape: 'diamond' },

    // IF branches
    { id: 'D', label: 'Prepare personalized content', kind: 'item' },
    { id: 'E', label: 'Send reactivation email', kind: 'item' },

    // FOR loop (simulate iteration)
    {
      id: 'F',
      label: 'For each preferred topic',
      kind: 'item',
      shape: 'hexagon',
    },
    { id: 'G', label: 'Generate topic summary', kind: 'item' },
    { id: 'H', label: 'Append to digest', kind: 'item' },
    { id: 'I', label: 'More topics?', kind: 'item', shape: 'diamond' },
    { id: 'J', label: 'Summaries ready', kind: 'item' },

    // Subgraph: Nested workflow call
    { id: 'SUBGRAPH1', label: 'GenerateEmailWorkflow', kind: 'group' },
    {
      id: 'S1',
      label: 'Format email header',
      kind: 'item',
      parent: 'SUBGRAPH1',
    },
    {
      id: 'S2',
      label: 'Assemble email body',
      kind: 'item',
      parent: 'SUBGRAPH1',
    },
    {
      id: 'S3',
      label: 'Run grammar corrections',
      kind: 'item',
      parent: 'SUBGRAPH1',
    },

    // Final steps
    { id: 'K', label: 'Send final email', kind: 'item' },
    { id: 'L', label: 'Log result', kind: 'item' },
    { id: 'M', label: 'End', kind: 'item' },
  ],
  edges: [
    // Main flow
    { id: 'e_A_B', from: 'A', to: 'B', style: 'solid' },
    { id: 'e_B_C', from: 'B', to: 'C', style: 'solid' },

    // IF branch (decision node C)
    { id: 'e_C_D', from: 'C', to: 'D', style: 'solid' }, // Yes path
    { id: 'e_C_E', from: 'C', to: 'E', style: 'solid' }, // No path

    // FOR loop
    { id: 'e_D_F', from: 'D', to: 'F', style: 'solid' },
    { id: 'e_F_G', from: 'F', to: 'G', style: 'solid' },
    { id: 'e_G_H', from: 'G', to: 'H', style: 'solid' },
    { id: 'e_H_I', from: 'H', to: 'I', style: 'solid' },
    { id: 'e_I_F', from: 'I', to: 'F', style: 'dashed' }, // Loop back (Yes)
    { id: 'e_I_J', from: 'I', to: 'J', style: 'solid' }, // Exit loop (No)

    // Subgraph integration
    { id: 'e_J_SUBGRAPH1', from: 'J', to: 'SUBGRAPH1', style: 'solid' },
    { id: 'e_S1_S2', from: 'S1', to: 'S2', style: 'solid' },
    { id: 'e_S2_S3', from: 'S2', to: 'S3', style: 'solid' },
    { id: 'e_SUBGRAPH1_K', from: 'SUBGRAPH1', to: 'K', style: 'solid' },
    { id: 'e_E_K', from: 'E', to: 'K', style: 'solid' },

    // Final steps
    { id: 'e_K_L', from: 'K', to: 'L', style: 'solid' },
    { id: 'e_L_M', from: 'L', to: 'M', style: 'solid' },
  ],
};
