import React from 'react';
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from '@baml/ui/tooltip';
import { AlertTriangle, Check, Info } from 'lucide-react';
import { REQUIRED_ENV_VAR_UNSET_WARNING, isPlaceholderApiKey, PLACEHOLDER_ENV_VAR_MESSAGE } from './utils';

export function ApiKeyStatus({
  value,
  required,
}: { value?: string; required: boolean }) {
  const isPlaceholder = isPlaceholderApiKey(value);

  if ((!value || value === '') && required) {
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

  if (isPlaceholder) {
    return (
      <TooltipProvider delayDuration={300}>
        <Tooltip>
          <TooltipTrigger asChild>
            <div className="flex items-center gap-1">
              <Info className="h-4 w-4 text-blue-500 flex-shrink-0" />
              <span className="text-xs text-muted-foreground">(placeholder)</span>
            </div>
          </TooltipTrigger>
          <TooltipContent side="top" className="text-xs">
            {PLACEHOLDER_ENV_VAR_MESSAGE}
          </TooltipContent>
        </Tooltip>
      </TooltipProvider>
    );
  }

  if (required && value) {
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