'use client';
import { PreviewToolbar } from '../preview-toolbar';
import { ApiKeysDialog } from '../../../../components/api-keys-dialog/dialog';
import { PromptRenderWrapper } from './prompt-render-wrapper';
import TestPanel from './test-panel';
import { useAtomValue } from 'jotai';
import { wasmAtom } from '../../atoms';
import { Loader } from '@baml/ui/custom/loader';


export const PromptPreview = () => {
  const wasm = useAtomValue(wasmAtom)
  return (
    <div className="p-2">
        <div className="flex w-full h-full bg-background text-foreground">
          <div className="flex overflow-y-auto flex-col w-full h-full gap-2">
            {wasm ? (
              <>
              <ApiKeysDialog />
              <PreviewToolbar />
              <PromptRenderWrapper />
              <TestPanel />
            </>
            ) : (
              <Loader message="Loading..." />
            )}
          </div>
        </div>
    </div>
  );
};
