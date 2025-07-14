import React, { useCallback } from 'react';
import { Button } from '@baml/ui/button';
import { Input } from '@baml/ui/input';
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from '@baml/ui/tooltip';
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
  AlertDialogTrigger,
} from '@baml/ui/alert-dialog';
import { AlertTriangle, Eye, EyeOff, Trash2 } from 'lucide-react';
import { escapeValue, unescapeValue, REQUIRED_ENV_VAR_UNSET_WARNING } from './utils';
import type { ApiKeyEntry } from './atoms';
import { useSetAtom } from 'jotai';
import { updateApiKeyAtom, deleteApiKeyAtom, apiKeyVisibilityAtom } from './atoms';

interface ApiKeyListItemProps {
  apiKey: ApiKeyEntry;
}

export const ApiKeyListItem: React.FC<ApiKeyListItemProps> = ({
  apiKey,
}) => {
  const updateApiKey = useSetAtom(updateApiKeyAtom);
  const deleteApiKey = useSetAtom(deleteApiKeyAtom);
  const setVisibility = useSetAtom(apiKeyVisibilityAtom);

  const handleChange = useCallback((e: React.ChangeEvent<HTMLInputElement>) => {
    updateApiKey({ key: apiKey.key, value: unescapeValue(e.target.value) });
  }, [apiKey.key, updateApiKey]);

  const toggleVisibility = useCallback((key: string) => {
    setVisibility((prev) => ({
      ...prev,
      [key]: !prev[key],
    }));
  }, [setVisibility]);

  const handleDelete = useCallback(() => {
    deleteApiKey(apiKey.key);
  }, [apiKey.key, deleteApiKey]);

  console.log('ApiKeyListItem: apiKey:', apiKey);

  return (
    <div className="flex items-center gap-3 rounded-lg border border-border bg-background/70 px-4 py-3">
      <div className="flex-1 grid grid-cols-[minmax(200px,_0.4fr)_1fr] items-center gap-4">
        <div className="overflow-hidden">
          <TooltipProvider delayDuration={300}>
            <Tooltip>
              <TooltipTrigger asChild>
                <span className="font-mono text-sm text-muted-foreground truncate block cursor-pointer">{apiKey.key}</span>
              </TooltipTrigger>
              <TooltipContent side="top" className="text-xs">
                {apiKey.key}
              </TooltipContent>
            </Tooltip>
          </TooltipProvider>
        </div>
        <div className="flex items-center gap-2">
          <div className="relative flex-1">
            <Input
              type={apiKey.hidden ? 'password' : 'text'}
              value={typeof apiKey.value === 'string' ? escapeValue(apiKey.value) : ''}
              onChange={handleChange}
              className="h-8 text-sm font-mono placeholder:font-sans"
              placeholder=""
              autoComplete="off"
              data-1p-ignore
            />
            {apiKey.required && (!apiKey.value || apiKey.value === '') && (
              <div className="absolute right-2 top-1/2 -translate-y-1/2">
                <TooltipProvider delayDuration={300}>
                  <Tooltip>
                    <TooltipTrigger asChild>
                      <AlertTriangle className="h-4 w-4 text-yellow-500" />
                    </TooltipTrigger>
                    <TooltipContent side="top" className="text-xs">
                      {REQUIRED_ENV_VAR_UNSET_WARNING}
                    </TooltipContent>
                  </Tooltip>
                </TooltipProvider>
              </div>
            )}
          </div>
          <Button
            variant="ghost"
            size="sm"
            className="p-1 h-8 w-8 flex-shrink-0"
            onClick={() => toggleVisibility(apiKey.key)}
          >
            {apiKey.hidden ? (
              <Eye className="w-4 h-4 text-muted-foreground" />
            ) : (
              <EyeOff className="w-4 h-4 text-muted-foreground" />
            )}
          </Button>
        </div>
      </div>
      <AlertDialog>
        <AlertDialogTrigger asChild>
          <Button
            variant="ghost"
            size="sm"
            className="p-1 h-8 w-8 flex-shrink-0"
          >
            <Trash2 className="w-4 h-4 text-muted-foreground hover:text-destructive" />
          </Button>
        </AlertDialogTrigger>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Delete API Key</AlertDialogTitle>
            <AlertDialogDescription>
              Are you sure you want to delete the API key "{apiKey.key}"? This action cannot be undone.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction onClick={handleDelete} className="bg-destructive text-destructive-foreground hover:bg-destructive/90">
              Delete
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  );
};