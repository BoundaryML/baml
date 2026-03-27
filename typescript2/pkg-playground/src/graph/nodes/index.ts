import type { NodeTypes } from '@xyflow/react';
import { BaseNode } from './BaseNode';
import { LLMNode } from './LLMNode';
import { DiamondNode } from './DiamondNode';
import { HexagonNode } from './HexagonNode';
import { GroupNode } from './GroupNode';

export const kNodeTypes: NodeTypes = {
  base: BaseNode,
  group: GroupNode,
  diamond: DiamondNode,
  hexagon: HexagonNode,
  llm: LLMNode,
};
