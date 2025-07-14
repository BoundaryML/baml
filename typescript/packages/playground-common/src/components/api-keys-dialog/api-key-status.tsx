import React from 'react';
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from '@baml/ui/tooltip';
import { AlertTriangle, Check } from 'lucide-react';
import { REQUIRED_ENV_VAR_UNSET_WARNING } from './utils';

export function ApiKeyStatus({
  value,
  required,
}: { value?: string; required: boolean }) {
  if (!value || value === '') {
    return (
      <TooltipProvider delayDuration={300}>
        <Tooltip>
          <TooltipTrigger asChild>
            <AlertTriangle className="h-4 w-4 text-orange-500 flex-shrink-0" />
          </TooltipTrigger>
          <TooltipContent side="top" className="text-xs">
            {REQUIRED_ENV_VAR_UNSET_WARNING}
          </TooltipContent>
        </Tooltip>
      </TooltipProvider>
    );
  }

  if (required) {
    return (
      <TooltipProvider delayDuration={300}>
        <Tooltip>
          <TooltipTrigger asChild>
            <Check className="h-4 w-4 text-green-500 flex-shrink-0" />
          </TooltipTrigger>
          <TooltipContent side="top" className="text-xs">
            Used by one of your BAML clients
          </TooltipContent>
        </Tooltip>
      </TooltipProvider>
    );
  }

  return <div />;
}