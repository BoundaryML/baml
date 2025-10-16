import React, { useState, useCallback } from 'react';
import { Button } from '@baml/ui/button';
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from '@baml/ui/dialog';
import { Label } from '@baml/ui/label';
import { Textarea } from '@baml/ui/textarea';
import { toast } from '@baml/ui/sonner';
import { parse as parseDotenv } from 'dotenv';
import { FileText } from 'lucide-react';
import { useSetAtom } from 'jotai';
import { importApiKeysAtom } from './atoms';

export const ImportApiKeyDialog: React.FC = () => {
  const importApiKeys = useSetAtom(importApiKeysAtom);
  const [envFileContent, setEnvFileContent] = useState('');

  const handleImport = useCallback(() => {
    try {
      const parsed = parseDotenv(envFileContent);
      const newKeys = Object.keys(parsed);
      importApiKeys(parsed);
      setEnvFileContent('');
      toast.success(
        `Successfully imported ${newKeys.length} variables`,
      );
    } catch (error) {
      toast.error('Error parsing .env file', {
        description: 'Please check the format of your .env file',
      });
    }
  }, [envFileContent, importApiKeys]);

  return (
    <Dialog>
      <DialogTrigger asChild>
        <Button variant="outline" size="sm">
          <FileText className="h-4 w-4 mr-2" />
          Import .env
        </Button>
      </DialogTrigger>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Import from .env file</DialogTitle>
        </DialogHeader>
        <div className="py-4">
          <Label htmlFor="env-file">
            Paste your .env file content below:
          </Label>
          <Textarea
            id="env-file"
            className="min-h-[200px] mt-2 font-mono text-xs"
            placeholder="KEY=value"
            value={envFileContent}
            onChange={(e) => setEnvFileContent(e.target.value)}
          />
        </div>
        <DialogFooter>
          <DialogClose asChild>
            <Button variant="outline">Cancel</Button>
          </DialogClose>
          <DialogClose asChild>
            <Button onClick={handleImport}>Import</Button>
          </DialogClose>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
};