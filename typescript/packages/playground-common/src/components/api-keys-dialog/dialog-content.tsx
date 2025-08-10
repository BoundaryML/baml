'use client';

import { useSetAtom } from 'jotai';
import React from 'react';
import { AddApiKeyForm } from './add-api-key-form';
import { ApiKeysList } from './api-keys-list';
import { syncLocalApiKeysAtom } from './atoms';
import { ImportApiKeyDialog } from './import-api-key-dialog';
import { SaveActionsFooter } from './save-actions-footer';

export const ApiKeysDialogContent: React.FC = () => {
  const syncLocalApiKeys = useSetAtom(syncLocalApiKeysAtom);

  // Initialize local API keys on mount
  React.useEffect(() => {
    syncLocalApiKeys();
  }, [syncLocalApiKeys]);
  return (
    <div className="space-y-2 max-h-[70vh] overflow-y-auto">
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
