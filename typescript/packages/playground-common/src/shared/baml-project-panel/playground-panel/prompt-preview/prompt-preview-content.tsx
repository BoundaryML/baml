import { atom, useAtomValue, useSetAtom } from 'jotai';
import { useState } from 'react';
import { useMemo } from 'react';
import useSWR from 'swr';
import { apiKeysAtom } from '../../../../components/api-keys-dialog/atoms';
import { ctxAtom, diagnosticsAtom, filesAtom, runtimeAtom } from '../../atoms';
import { areTestsRunningAtom, selectionAtom } from '../atoms';
import { Loader } from './components';
import { vscode } from '../../vscode';
import { RenderPrompt } from './render-prompt';
import { EnhancedErrorRenderer } from './test-panel/components/EnhancedErrorRenderer';
import { runtimeInstanceAtom } from '../../../../sdk/atoms/core.atoms';
import type { PromptInfo } from '../../../../sdk/interface';

export const renderedPromptAtom = atom<PromptInfo | undefined>(undefined);

export const PromptPreviewContent = () => {
  const runtime = useAtomValue(runtimeInstanceAtom);
  const apiKeys = useAtomValue(apiKeysAtom);
  const ctx = useAtomValue(ctxAtom);
  const files = useAtomValue(filesAtom);
  const { selectedFn, selectedTc } = useAtomValue(selectionAtom);
  const diagnostics = useAtomValue(diagnosticsAtom);
  const setPromptData = useSetAtom(renderedPromptAtom);
  const areTestsRunning = useAtomValue(areTestsRunningAtom);

  // Memoize the generatePreview function to prevent unnecessary re-renders
  const generatePreview = useMemo(
    () => async () => {
      if (
        runtime === null ||
        selectedFn === null ||
        selectedTc === null
      ) {
        console.log('[PromptPreview] no runtime or selectedFn or selectedTc');
        return;
      }
      console.log('[PromptPreview] Attempt renderPromptForTest', {
        functionName: selectedFn.name,
        testCaseName: selectedTc.name,
        hasTests: selectedFn.testCases?.length ?? 0,
      });
      try {
        // Use runtime interface method instead of calling WASM directly
        const newPreview = await runtime.renderPromptForTest(
          selectedFn.name,
          selectedTc.name,
          {
            apiKeys,
            loadMediaFile: vscode.loadMediaFile,
          }
        );

        setLastKnownPreview(newPreview);
        setPromptData(newPreview);
        return newPreview;
      } catch (error) {
        console.error('[PromptPreview] renderPromptForTest failed', {
          functionName: selectedFn.name,
          testCaseName: selectedTc.name,
          error,
        });
        throw error;
      }
    },
    [runtime, ctx, selectedFn, selectedTc, apiKeys, files, setPromptData],
  );

  const [lastKnownPreview, setLastKnownPreview] = useState<
    PromptInfo | undefined
  >();


  const {
    data: preview,
    error,
    isLoading,
  } = useSWR(
    // Include file content in the key so updates trigger when typing
    runtime && selectedFn && selectedTc
      ? [
        'prompt-preview',
        selectedFn.name,
        selectedTc.name,
        JSON.stringify(apiKeys),
        JSON.stringify(files), // Add file content to trigger updates on typing
      ]
      : null,
    generatePreview,
    {
      // Less aggressive caching to allow instant updates
      revalidateOnFocus: false,
      revalidateOnReconnect: false,
      dedupingInterval: 100, // Reduced from 1000ms to 100ms for more responsiveness
    },
  );

  if (isLoading && !preview) {
    if (lastKnownPreview) {
      console.log('[PromptPreview] Rendering last known preview');
      return <RenderPrompt prompt={lastKnownPreview} testCase={selectedTc ?? undefined} />;
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
      .filter((d) => d.type === 'error')
      .map((d) => `- ${d.message}`)
      .join('\n');

    const fullErrorMessage = `${diagnostics.filter((d) => d.type === 'error').length} error(s):\n${errorMessages}`;

    return (
      <div className="relative">
        {/* todo: maybe keep rendering the last known prompt? And make this a more condensed error banner in absolute position? */}
        <div className="p-3">
          <EnhancedErrorRenderer errorMessage={fullErrorMessage} />
        </div>
      </div>
    );
  }

  return <RenderPrompt prompt={preview} testCase={selectedTc ?? undefined} />;
};
