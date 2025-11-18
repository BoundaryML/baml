/**
 * Workflow Indicator Component
 *
 * Displays the current workflow name with an identicon at the center top.
 * If the selected node exists in multiple workflows, shows a dropdown to switch.
 */

import { minidenticon } from 'minidenticons';
import { useEffect, useMemo } from 'react';
import { ChevronDown } from 'lucide-react';
import { useActiveWorkflow, useWorkflows } from '../../../sdk/hooks';
import { flowStore } from '../../../states/reactflow';
import { panToNodeIfNeeded } from '../../../utils/cameraPan';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@baml/ui/dropdown-menu';
import { selectedNodeIdAtom } from '@/sdk/atoms/core.atoms';
import { useAtomValue } from 'jotai';

interface MinidenticonImgProps {
  username: string;
  saturation?: number;
  lightness?: number;
  className?: string;
  alt?: string;
}

const MinidenticonImg = ({ username, saturation = 50, lightness = 50, className, alt }: MinidenticonImgProps) => {
  const svgURI = useMemo(
    () => 'data:image/svg+xml;utf8,' + encodeURIComponent(minidenticon(username, saturation, lightness)),
    [username, saturation, lightness]
  );
  return <img src={svgURI} alt={alt || username} className={className} />;
};

export function WorkflowIndicator() {
  const { activeWorkflow, setActiveWorkflow } = useActiveWorkflow();
  const selectedNodeId = useAtomValue(selectedNodeIdAtom);
  const allWorkflows = useWorkflows();

  // Find all workflows that contain the selected node
  const workflowsWithSelectedNode = useMemo(() => {
    if (!selectedNodeId) return [];

    return allWorkflows.filter(workflow =>
      workflow.nodes.some(node => node.id === selectedNodeId)
    );
  }, [selectedNodeId, allWorkflows]);

  // Determine if we should show the dropdown
  const shouldShowDropdown = workflowsWithSelectedNode.length > 1;
  console.log('aaron: selectedNodeId:', selectedNodeId);



  if (!activeWorkflow) {
    return null;
  }



  return (
    <div className="absolute top-2 left-1/2 -translate-x-1/2 z-[1000]">
      {shouldShowDropdown ? (
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <button className="flex items-center gap-2 bg-card px-3 py-1.5 rounded-lg shadow-md border hover:bg-accent transition-colors">
              <MinidenticonImg
                username={activeWorkflow.id}
                saturation={60}
                lightness={50}
                className="w-5 h-5 rounded"
              />
              <span className="text-sm font-medium">{activeWorkflow.displayName}</span>
              <ChevronDown className="w-4 h-4 text-muted-foreground" />
            </button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="center" className="min-w-[200px]">
            <div className="px-2 py-1.5 text-xs text-muted-foreground font-medium">
              Switch to workflow containing "{selectedNodeId}"
            </div>
            {workflowsWithSelectedNode.map((workflow) => (
              <DropdownMenuItem
                key={workflow.id}
                // onClick={() => handleWorkflowSwitch(workflow.id)}
                className={workflow.id === activeWorkflow.id ? 'bg-accent' : ''}
              >
                <div className="flex items-center gap-2">
                  <MinidenticonImg
                    username={workflow.id}
                    saturation={60}
                    lightness={50}
                    className="w-4 h-4 rounded"
                  />
                  <span>{workflow.displayName}</span>
                  {workflow.id === activeWorkflow.id && (
                    <span className="ml-auto text-xs text-muted-foreground">✓</span>
                  )}
                </div>
              </DropdownMenuItem>
            ))}
          </DropdownMenuContent>
        </DropdownMenu>
      ) : (
        <div className="flex items-center gap-2 bg-card px-3 py-1.5 rounded-lg shadow-md border">
          <MinidenticonImg
            username={activeWorkflow.id}
            saturation={60}
            lightness={50}
            className="w-5 h-5 rounded"
          />
          <span className="text-sm font-medium">{activeWorkflow.displayName}</span>
        </div>
      )}
    </div>
  );
}
