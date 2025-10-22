'use client';

import { useSetAtom, useAtomValue } from 'jotai';
import React from 'react';
import { AddApiKeyForm } from './add-api-key-form';
import { ApiKeysList } from './api-keys-list';
import { syncLocalApiKeysAtom, renderedApiKeysAtom } from './atoms';
import { ImportApiKeyDialog } from './import-api-key-dialog';
import { SaveActionsFooter } from './save-actions-footer';
import { Alert, AlertDescription } from '@baml/ui/alert';
import { Info } from 'lucide-react';
import { isPlaceholderApiKey } from './utils';

export const ApiKeysDialogContent: React.FC = () => {
  const syncLocalApiKeys = useSetAtom(syncLocalApiKeysAtom);
  const apiKeys = useAtomValue(renderedApiKeysAtom);

  // Initialize local API keys on mount
  React.useEffect(() => {
    syncLocalApiKeys();
  }, [syncLocalApiKeys]);

  const hasPlaceholderKeys = apiKeys.some(key => isPlaceholderApiKey(key.value));

  return (
    <div className="space-y-2 max-h-[70vh] overflow-y-auto">
      {/* Welcome message for users with placeholder keys */}
      {hasPlaceholderKeys && (
        <Alert className="mb-4">
          <Info className="h-4 w-4" />
          <AlertDescription>
            <div className="space-y-2">
              <p>Welcome to BAML Playground! We've added placeholder API keys to help you get started.</p>
              <p className="text-sm text-muted-foreground">
                You can explore the playground features immediately. When you're ready to make real LLM calls,
                replace the placeholder values with your actual API keys from OpenAI or Anthropic.
              </p>
            </div>
          </AlertDescription>
        </Alert>
      )}

      {/* Add New Api Key Form */}
      <div className="mb-4 p-4 rounded-md border border-border flex flex-col gap-2">
        <AddApiKeyForm />

        <div className="flex items-center justify-between gap-2 text-sm text-muted-foreground mt-2 border-t border-border pt-4">
          <div className="flex items-center gap-2">
            <ImportApiKeyDialog />
            <span>or paste the .env contents above</span>
          </div>
          <SaveActionsFooter />
        </div>
      </div>

      {/* Env Vars List */}
      <ApiKeysList />
    </div>
  );
};
