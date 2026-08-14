import { useAtomValue } from 'jotai';
import { useState } from 'react';
import { useCallback } from 'react';
import useSWR from 'swr';
import { ctxAtom, diagnosticsAtom, runtimeAtom } from '../../atoms';
import { functionTestSnippetAtom, selectionAtom } from '../atoms';
import { Loader } from './components';
import { ErrorMessage } from './components';
import { findMediaFile } from './media-utils';
import { RenderPrompt } from './render-prompt';

export const PromptPreviewContent = () => {
  const { rt } = useAtomValue(runtimeAtom);
  const ctx = useAtomValue(ctxAtom);
  const { selectedFn, selectedTc } = useAtomValue(selectionAtom);
  const diagnostics = useAtomValue(diagnosticsAtom);

  const generatePreview = async () => {
    if (
      rt === undefined ||
      ctx === undefined ||
      selectedFn === undefined ||
      selectedTc === undefined
    ) {
      return;
    }
    return selectedFn.render_prompt_for_test(
      rt,
      selectedTc.name,
      ctx,
      findMediaFile,
      { BOUNDARY_PROXY_URL: 'https://fiddle-proxy.fly.dev' },
    );
  };

  const {
    data: preview,
    error,
    isLoading,
  } = useSWR([rt, ctx, selectedFn, selectedTc], generatePreview);

  if (isLoading) {
    return <Loader message="Loading preview..." />;
  }

  if (error) {
    return (
      <ErrorMessage
        error={error instanceof Error ? error.message : 'Unknown Error'}
      />
    );
  }

  if (diagnostics.length > 0 && diagnostics.some((d) => d.type === 'error')) {
    return (
      <div>
        <div className="mb-2 text-sm font-medium text-red-500">
          Syntax Error
        </div>
        <pre className="whitespace-pre-wrap rounded-lg px-2 py-1 font-mono text-red-500">
          <div className="space-y-2">
            <div>
              {diagnostics.filter((d) => d.type === 'error').length} error(s):
            </div>
            {diagnostics
              .filter((d) => d.type === 'error')
              .map((d, i) => (
                <div key={i}>- {d.message}</div>
              ))}
          </div>
        </pre>
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
    <div className="flex flex-col items-center justify-center">
      <div className="mb-4 text-sm font-medium text-gray-500">
        Add a test to see the preview!
      </div>
      <div className="relative w-full max-w-2xl rounded-lg border border-gray-200 bg-gray-50">
        <div className="absolute right-2 top-2">
          <button
            onClick={handleCopy}
            className="rounded bg-white px-2 py-1 text-xs font-medium text-gray-600 shadow-sm hover:bg-gray-50 focus:outline-none focus:ring-2 focus:ring-blue-500 focus:ring-offset-2"
          >
            {copied ? 'Copied!' : 'Copy'}
          </button>
        </div>
        <pre className="overflow-x-auto p-4 font-mono text-sm text-gray-700 text-balance">
          {testSnippet}
        </pre>
      </div>
    </div>
  );
};
