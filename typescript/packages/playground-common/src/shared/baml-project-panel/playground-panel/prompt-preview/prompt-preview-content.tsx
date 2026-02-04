import { atom, useAtomValue, useSetAtom } from 'jotai';
import { useState } from 'react';
import { useMemo } from 'react';
import useSWR from 'swr';
import { apiKeysAtom } from '../../../../components/api-keys-dialog/atoms';
import { ctxAtom, diagnosticsAtom } from '../../atoms';
import { areTestsRunningAtom, selectionAtom } from '../atoms';
import { Loader } from './components';
import { vscode } from '../../vscode';
import { RenderPrompt } from './render-prompt';
import { EnhancedErrorRenderer } from './test-panel/components/EnhancedErrorRenderer';
import { runtimeInstanceAtom } from '../../../../sdk/atoms/core.atoms';
import type { PromptInfo } from '../../../../sdk/interface';
import { AlertCircle } from 'lucide-react';

export const renderedPromptAtom = atom<PromptInfo | undefined>(undefined);

export const PromptPreviewContent = () => {
  const runtime = useAtomValue(runtimeInstanceAtom);
  const apiKeys = useAtomValue(apiKeysAtom);
  const ctx = useAtomValue(ctxAtom);
  const { selectedFn, selectedTc } = useAtomValue(selectionAtom);
  const diagnostics = useAtomValue(diagnosticsAtom);
  const setPromptData = useSetAtom(renderedPromptAtom);
  const areTestsRunning = useAtomValue(areTestsRunningAtom);

  // Memoize the generatePreview function to prevent unnecessary re-renders
  // Note: runtime is a new instance each time files change, so including it triggers re-memoization
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
    [runtime, ctx, selectedFn, selectedTc, apiKeys, setPromptData],
  );

  const [lastKnownPreview, setLastKnownPreview] = useState<
    PromptInfo | undefined
  >();


  const {
    data: preview,
    error,
    isLoading,
  } = useSWR(
    // Include runtime in key to re-fetch when runtime is recreated (after file changes)
    // Runtime is a new instance each time files change, so this triggers re-fetch
    runtime && selectedFn && selectedTc
      ? [
        'prompt-preview',
        runtime, // Runtime instance changes when files change
        selectedFn.name,
        selectedTc.name,
        JSON.stringify(apiKeys),
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
      return <RenderPrompt prompt={lastKnownPreview} testCase={selectedTc ?? undefined} />;
    }
    return <Loader message="Loading..." />;
  }


  const hasDiagnosticErrors = diagnostics.some((d) => d.type === 'error');

  // Diagnostic errors: show stale prompt with banner if available
  if (hasDiagnosticErrors) {
    if (lastKnownPreview) {
      const errorCount = diagnostics.filter((d) => d.type === 'error').length;
      return (
        <div className="relative">
          <div className="absolute top-2 left-1/2 -translate-x-1/2 z-10">
            <div className="flex items-center gap-2 rounded border bg-vscode-notifications-background border-vscode-notifications-border px-3 py-1.5 shadow-md text-xs text-foreground">
              <AlertCircle className="h-3.5 w-3.5 text-destructive shrink-0" />
              <span>
                Project has {errorCount} error{errorCount !== 1 ? 's' : ''} — showing last valid preview
              </span>
            </div>
          </div>
          <RenderPrompt prompt={lastKnownPreview} testCase={selectedTc ?? undefined} />
        </div>
      );
    }
    // No stale preview (first load with errors) — show full error
    const errorMessages = diagnostics
      .filter((d) => d.type === 'error')
      .map((d) => `- ${d.message}`)
      .join('\n');
    return (
      <div className="p-3">
        <EnhancedErrorRenderer errorMessage={`${diagnostics.filter((d) => d.type === 'error').length} error(s):\n${errorMessages}`} />
      </div>
    );
  }

  // Non-diagnostic SWR errors (runtime rendering errors unrelated to compilation)
  if (error) {
    return <EnhancedErrorRenderer errorMessage={error instanceof Error ? error.message : 'Unknown Error'} />;
  }

  return <RenderPrompt prompt={preview} testCase={selectedTc ?? undefined} />;
};
