import type { NodeTypes } from '@xyflow/react';
import { BaseNode } from './BaseNode';
import { DiamondNode } from './DiamondNode';
import { GroupNode } from './group-node';
import { HexagonNode } from './HexagonNode';
import { LLMNode } from './LLMNode';

export const kNodeTypes: NodeTypes = {
  base: BaseNode,
  diamond: DiamondNode,
  group: GroupNode,
  hexagon: HexagonNode,
  llm: LLMNode,
};
