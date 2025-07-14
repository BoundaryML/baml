'use client';

import React from 'react';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from '@baml/ui/dialog';
import { ApiKeysDialogContent } from './dialog-content';
import { useAtom } from 'jotai';
import { showApiKeyDialogAtom } from './atoms';

export const ApiKeysDialog: React.FC = () => {
  const [showDialog, setShowDialog] = useAtom(showApiKeyDialogAtom);
  return (
    <Dialog open={showDialog} onOpenChange={setShowDialog}>
      <DialogContent className="min-w-0 md:min-w-2xl">
        <DialogHeader>
          <DialogTitle>API Keys</DialogTitle>
          <DialogDescription>
            <p>
              Set your own API Keys here.&nbsp;
              <a
                href="https://docs.boundaryml.com/ref/llm-client-providers/overview#fields"
                target="_blank"
                rel="noopener noreferrer"
                className="text-blue-500 hover:underline"
              >
                See supported LLMs
              </a>
            </p>
          </DialogDescription>
        </DialogHeader>
          <ApiKeysDialogContent />
      </DialogContent>
    </Dialog>
  );
};

