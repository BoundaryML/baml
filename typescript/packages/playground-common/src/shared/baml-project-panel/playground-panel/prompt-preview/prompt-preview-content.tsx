import type { WasmError, WasmPrompt } from '@gloo-ai/baml-schema-wasm-web';
import { atom, useAtomValue, useSetAtom } from 'jotai';
import { useState } from 'react';
import { useMemo } from 'react';
import useSWR from 'swr';
import { apiKeysAtom } from '../../../../components/api-keys-dialog/atoms';
import { ctxAtom, diagnosticsAtom, filesAtom, runtimeAtom } from '../../atoms';
import { areTestsRunningAtom, selectionAtom } from '../atoms';
import { Loader } from './components';
import { findMediaFile } from './media-utils';
import { RenderPrompt } from './render-prompt';
import { EnhancedErrorRenderer } from './test-panel/components/EnhancedErrorRenderer';

export const renderedPromptAtom = atom<WasmPrompt | undefined>(undefined);

export const PromptPreviewContent = () => {
  const { rt } = useAtomValue(runtimeAtom);
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
    },
    [rt, ctx, selectedFn, selectedTc, apiKeys, files, setPromptData],
  );

  const [lastKnownPreview, setLastKnownPreview] = useState<
    WasmPrompt | undefined
  >();

  const {
    data: preview,
    error,
    isLoading,
  } = useSWR(
    // Include file content in the key so updates trigger when typing
    rt && ctx && selectedFn && selectedTc
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
          <EnhancedErrorRenderer errorMessage={fullErrorMessage} />
        </div>
      </div>
    );
  }

  return <RenderPrompt prompt={preview} testCase={selectedTc} />;
};
