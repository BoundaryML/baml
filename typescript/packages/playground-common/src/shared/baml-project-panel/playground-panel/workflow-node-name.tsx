import {
  Breadcrumb,
  BreadcrumbItem,
  BreadcrumbList,
} from '@baml/ui/breadcrumb';
import { Tooltip, TooltipContent, TooltipTrigger } from '@baml/ui/tooltip';
import { Network, Box } from 'lucide-react';
import { useAtomValue } from 'jotai';
import { activeWorkflowAtom } from '../../../sdk/atoms/core.atoms';

interface WorkflowNodeNameProps {
  workflowId: string;
  nodeId: string;
}

export const WorkflowNodeName: React.FC<WorkflowNodeNameProps> = ({
  workflowId,
  nodeId,
}) => {
  const activeWorkflow = useAtomValue(activeWorkflowAtom);

  // Find the node in the active workflow to get its label
  const node = activeWorkflow?.nodes?.find((n) => n.id === nodeId);
  const nodeLabel = node?.label;


  return (
    <Breadcrumb>
      <BreadcrumbList className="flex flex-nowrap overflow-hidden min-w-0">
        <BreadcrumbItem className="flex items-center gap-1 min-w-0">
          <div className="flex items-center gap-1 min-w-0 flex-1">
            <Network className="size-4 mr-2 shrink-0" />
            <Tooltip>
              <TooltipTrigger asChild>
                <span className="truncate min-w-0 whitespace-nowrap text-left flex-1">
                  {workflowId}
                </span>
              </TooltipTrigger>
              <TooltipContent>{workflowId}</TooltipContent>
            </Tooltip>
          </div>
        </BreadcrumbItem>
        <BreadcrumbItem className="flex items-center gap-1 min-w-0">
          <div className="flex items-center gap-1 min-w-0 flex-1">
            <Box className="size-4 mr-2 shrink-0" />
            <Tooltip>
              <TooltipTrigger asChild>
                <span className="truncate min-w-0 whitespace-nowrap text-left flex-1">
                  {nodeLabel}
                </span>
              </TooltipTrigger>
              <TooltipContent>
                {nodeLabel !== nodeId ? `${nodeLabel} (${nodeId})` : nodeId}
              </TooltipContent>
            </Tooltip>
          </div>
        </BreadcrumbItem>
      </BreadcrumbList>
    </Breadcrumb>
  );
};
