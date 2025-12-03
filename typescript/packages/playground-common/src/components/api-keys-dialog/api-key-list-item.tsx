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
import { AlertTriangle, Eye, EyeOff, Trash2, Loader2 } from 'lucide-react';
import { escapeValue, unescapeValue, REQUIRED_ENV_VAR_UNSET_WARNING, isPlaceholderApiKey, PLACEHOLDER_ENV_VAR_MESSAGE } from './utils';
import type { ApiKeyEntry } from './atoms';
import { useSetAtom } from 'jotai';
import { updateApiKeyAtom, deleteApiKeyAtom, apiKeyVisibilityAtom, saveApiKeyChangesAtom } from './atoms';
import { useDebounceCallback } from '@react-hook/debounce';

interface ApiKeyListItemProps {
  apiKey: ApiKeyEntry;
}

/**
 * API Key list item with auto-save functionality.
 * Changes to the value field are automatically saved after 200ms of inactivity.
 * Optimized for copy-paste workflows.
 */
export const ApiKeyListItem: React.FC<ApiKeyListItemProps> = ({
  apiKey,
}) => {
  const updateApiKey = useSetAtom(updateApiKeyAtom);
  const deleteApiKey = useSetAtom(deleteApiKeyAtom);
  const setVisibility = useSetAtom(apiKeyVisibilityAtom);
  const saveApiKeyChanges = useSetAtom(saveApiKeyChangesAtom);

  // Track the input value locally for immediate UI updates
  const [localValue, setLocalValue] = React.useState(escapeValue(apiKey.value || ''));
  const [isSaving, setIsSaving] = React.useState(false);

  // Debounced save function
  const debouncedSave = useDebounceCallback(
    async (value: string) => {
      const unescapedValue = unescapeValue(value);
      setIsSaving(true);
      updateApiKey({ key: apiKey.key, value: unescapedValue });
      // Auto-save after updating the API key
      await saveApiKeyChanges();
      setIsSaving(false);
    },
    200, // 200ms delay - optimized for copy-paste
    false // don't call on leading edge
  );

  // Update local value when apiKey.value changes (e.g., from external updates)
  React.useEffect(() => {
    setLocalValue(escapeValue(apiKey.value || ''));
  }, [apiKey.value]);

  const handleChange = useCallback((e: React.ChangeEvent<HTMLInputElement>) => {
    const newValue = e.target.value;
    // Update local state immediately for responsive UI
    setLocalValue(newValue);
    // Trigger debounced save
    debouncedSave(newValue);
  }, [debouncedSave]);

  const toggleVisibility = useCallback((key: string) => {
    setVisibility((prev) => ({
      ...prev,
      [key]: !prev[key],
    }));
  }, [setVisibility]);

  const handleDelete = useCallback(() => {
    deleteApiKey(apiKey.key);
  }, [apiKey.key, deleteApiKey]);


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
              type={apiKey.hidden && !isPlaceholderApiKey(localValue) ? 'password' : 'text'}
              value={localValue}
              onChange={handleChange}
              className={`h-8 text-sm font-mono placeholder:font-sans ${isPlaceholderApiKey(localValue) ? 'text-muted-foreground italic' : ''
                }`}
              placeholder={isPlaceholderApiKey(localValue) ? "Replace with your API key" : ""}
              autoComplete="off"
              data-1p-ignore
            />
            {apiKey.required && (!apiKey.value || apiKey.value === '' || isPlaceholderApiKey(apiKey.value)) && (
              <div className="absolute right-2 top-1/2 -translate-y-1/2">
                <TooltipProvider delayDuration={300}>
                  <Tooltip>
                    <TooltipTrigger asChild>
                      <AlertTriangle className="h-4 w-4 text-yellow-500" />
                    </TooltipTrigger>
                    <TooltipContent side="top" className="text-xs">
                      {isPlaceholderApiKey(apiKey.value)
                        ? PLACEHOLDER_ENV_VAR_MESSAGE
                        : REQUIRED_ENV_VAR_UNSET_WARNING}
                    </TooltipContent>
                  </Tooltip>
                </TooltipProvider>
              </div>
            )}
          </div>
          {isSaving && (
            <div className="flex items-center justify-center h-8 w-8">
              <Loader2 className="h-4 w-4 animate-spin text-muted-foreground" />
            </div>
          )}
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