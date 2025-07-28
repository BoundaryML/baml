import type { WasmError, WasmPrompt } from '@gloo-ai/baml-schema-wasm-web';
import { atom, useAtomValue, useSetAtom } from 'jotai';
import { useState } from 'react';
import { useCallback } from 'react';
import useSWR from 'swr';
import React, { useMemo } from 'react';
import {
  ctxAtom,
  diagnosticsAtom,
  runtimeAtom,
} from '../../atoms';
import {
  areTestsRunningAtom,
  functionTestSnippetAtom,
  selectionAtom,
} from '../atoms';
import { Loader } from './components';
import { EnhancedErrorRenderer } from './test-panel/components/EnhancedErrorRenderer';
import { findMediaFile } from './media-utils';
import { RenderPrompt } from './render-prompt';
import { apiKeysAtom } from '../../../../components/api-keys-dialog/atoms';

export const renderedPromptAtom = atom<WasmPrompt | undefined>(undefined);

export const PromptPreviewContent = () => {
  const { rt } = useAtomValue(runtimeAtom);
  const apiKeys = useAtomValue(apiKeysAtom);
  const ctx = useAtomValue(ctxAtom);
  const { selectedFn, selectedTc } = useAtomValue(selectionAtom);
  const diagnostics = useAtomValue(diagnosticsAtom);
  const setPromptData = useSetAtom(renderedPromptAtom);
  const areTestsRunning = useAtomValue(areTestsRunningAtom);
  
  // Memoize the generatePreview function to prevent unnecessary re-renders
  const generatePreview = useMemo(() => async () => {
    if (
      rt === undefined ||
      ctx === undefined ||
      selectedFn === undefined ||
      selectedTc === undefined
    ) {
      return;
    }
    const newPreview = await selectedFn.render_prompt_for_test(
      rt,
      selectedTc.name,
      ctx,
      findMediaFile,
      apiKeys,
    );
    setLastKnownPreview(newPreview);
    setPromptData(newPreview);
    return newPreview;
  }, [rt, ctx, selectedFn, selectedTc, apiKeys, setPromptData]);

  const [lastKnownPreview, setLastKnownPreview] = useState<
    WasmPrompt | undefined
  >();

  const {
    data: preview,
    error,
    isLoading,
  } = useSWR(
    // Remove areTestsRunning to prevent constant re-renders
    // The key should be stable and only change when actual dependencies change
    rt && ctx && selectedFn && selectedTc 
      ? [
          'prompt-preview',
          selectedFn.name, 
          selectedTc.name, 
          JSON.stringify(apiKeys)
        ]
      : null,
    generatePreview,
    {
      // Add configuration to prevent unnecessary refreshes
      revalidateOnFocus: false,
      revalidateOnReconnect: false,
      dedupingInterval: 1000, // Prevent duplicate requests within 1 second
    }
  );

  if (isLoading && !preview) {
    if (lastKnownPreview) {
      return <RenderPrompt prompt={lastKnownPreview} testCase={selectedTc} />;
    }
    return <Loader message="Loading..." />;
  }

  if (error) {
    return (
      <EnhancedErrorRenderer
        errorMessage={error instanceof Error ? error.message : 'Unknown Error'}
      />
    );
  }

  if (diagnostics.length > 0 && diagnostics.some((d) => d.type === 'error')) {
    const errorMessages = diagnostics
      .filter((d: WasmError) => d.type === 'error')
      .map((d) => `- ${d.message}`)
      .join('\n');
    
    const fullErrorMessage = `${diagnostics.filter((d: WasmError) => d.type === 'error').length} error(s):\n${errorMessages}`;
    
    return (
      <div className="relative">
        {/* todo: maybe keep rendering the last known prompt? And make this a more condensed error banner in absolute position? */}
        <div className="p-3">
          <EnhancedErrorRenderer
            errorMessage={fullErrorMessage}
          />
        </div>
      </div>
    );
  }
  if (preview === undefined) {
    return <NoTestsContent />;
  }

  return <RenderPrompt prompt={preview} testCase={selectedTc} />;
};

export const NoTestsContent = () => {
  const { selectedFn } = useAtomValue(selectionAtom);
  const testSnippet = useAtomValue(
    functionTestSnippetAtom(selectedFn?.name ?? ''),
  );
  const [copied, setCopied] = useState(false);

  const handleCopy = useCallback(() => {
    void navigator.clipboard.writeText(testSnippet ?? '');
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  }, [testSnippet]);

  return (
    <div className="flex flex-col justify-center items-center">
      <div className="mb-4 text-sm font-medium text-muted-foreground">
        Add a test to see the preview!
      </div>
      <div className="relative w-full max-w-2xl rounded-lg border border-border bg-muted">
        <div className="absolute top-2 right-2">
          <button
            onClick={handleCopy}
            type="button"
            className="px-2 py-1 text-xs font-medium rounded shadow-xs bg-background text-muted-foreground hover:bg-muted focus:outline-hidden focus:ring-2 focus:ring-ring focus:ring-offset-2"
          >
            {copied ? 'Copied!' : 'Copy'}
          </button>
        </div>
        <pre className="overflow-x-auto p-4 text-sm text-balance text-foreground">
          {testSnippet}
        </pre>
      </div>
    </div>
  );
};
