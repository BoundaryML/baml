import { useReactFlow } from '@xyflow/react';

import { flowStore } from '../../../states/reactflow';

// Used to mount onto the ReactFlow component to get the corresponding ReactFlowInstance
export const ReactflowInstance = (): any => {
  const instance = useReactFlow();
  flowStore.setState((prev) => {
    return {
      ...prev,
      ...instance,
      initialized: true,
    };
  });
  return null;
};
