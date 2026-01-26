import type { NodeTypes } from '@xyflow/react';

import { BaseNode } from './BaseNode';
import { DiamondNode } from './DiamondNode';
import { GroupNode } from './GroupNode';
import { HexagonNode } from './HexagonNode';
import { LLMNode } from './LLMNode';

export const kNodeTypes: NodeTypes = {
  base: BaseNode,
  group: GroupNode,
  diamond: DiamondNode,
  hexagon: HexagonNode,
  llm: LLMNode,
};
