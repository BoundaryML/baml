import { CopyButton } from '@baml/ui/custom/copy-button';
import { useAtomValue } from 'jotai';
import { atom } from 'jotai';
import { loadable } from 'jotai/utils';
import { useTheme } from 'next-themes';
import { useEffect, useState } from 'react';
import type React from 'react';
import { useMemo } from 'react';
import { apiKeysAtom } from '../../../../components/api-keys-dialog/atoms';
import { ctxAtom, filesAtom, runtimeAtom } from '../../atoms';
import { selectionAtom } from '../atoms';
import { TruncatedString } from './TruncatedString';
import { Loader } from './components';
import { vscode } from '../../vscode';
import { EnhancedErrorRenderer } from './test-panel/components/EnhancedErrorRenderer';

type CurlResult =
  | {
    curlTextWithoutSecrets: string;
    curlTextWithSecrets: string;
  }
  | undefined
  | Error;

const baseCurlAtom = atom<Promise<CurlResult>>(async (get) => {
  const rt = get(runtimeAtom).rt;
  const ctx = get(ctxAtom);
  const envVars = get(apiKeysAtom);
  const files = get(filesAtom); // Add files dependency to track content changes
  const { selectedFn, selectedTc } = get(selectionAtom);

  if (!selectedFn || !rt || !selectedTc || !ctx) {
    return undefined;
  }

  // Check if we have any API keys available
  const hasApiKeys = Object.keys(envVars).some(
    (key) =>
      key !== 'BOUNDARY_PROXY_URL' &&
      envVars[key] &&
      envVars[key].trim() !== '',
  );

  let curlTextWithoutSecrets = '';
  let curlTextWithSecrets = '';

  try {
    curlTextWithoutSecrets = await selectedFn.render_raw_curl_for_test(
      rt,
      selectedTc.name,
      ctx,
      false,
      false,
      vscode.loadMediaFile,
      envVars,
      false, // Pass flag to indicate whether to expose secrets
    );

    curlTextWithSecrets = await selectedFn.render_raw_curl_for_test(
      rt,
      selectedTc.name,
      ctx,
      false,
      false,
      vscode.loadMediaFile,
      envVars,
      true, // Pass flag to indicate whether to expose secrets
    );
  } catch (error) {
    return error as Error;
  }

  return {
    curlTextWithoutSecrets,
    curlTextWithSecrets,
  };
});

const curlAtom = loadable(baseCurlAtom);

// Syntax highlighting component for curl commands
const SyntaxHighlightedCurl = ({ text }: { text: string }) => {
  const [highlightedHtml, setHighlightedHtml] = useState<string>('');
  const [highlighter, setHighlighter] = useState<any | undefined>(undefined);
  const { theme } = useTheme();

  useEffect(() => {
    if (highlighter) return;
    (async () => {
      try {
        const { createHighlighterCore } = await import('shiki/core');
        const highlighter = await createHighlighterCore({
          themes: [
            import('shiki/themes/github-dark-default.mjs'),
            import('shiki/themes/github-light.mjs'),
          ],
          langs: [import('shiki/langs/bash.mjs')],
          loadWasm: import('shiki/wasm'),
        });
        setHighlighter(highlighter);
      } catch (error) {
        console.error('Error creating highlighter:', error);
      }
    })();
  }, []);

  useEffect(() => {
    if (!highlighter || !text) return;

    (async () => {
      try {
        const themeName =
          theme === 'dark' ? 'github-dark-default' : 'github-light';
        const highlighted = highlighter.codeToHtml(text, {
          lang: 'bash',
          theme: themeName,
        });
        setHighlightedHtml(highlighted);
      } catch (error) {
        console.error('Error highlighting code:', error);
        setHighlightedHtml(text);
      }
    })();
  }, [highlighter, text, theme]);

  if (!highlightedHtml) {
    return (
      <div className="w-full rounded-lg border bg-accent p-4 font-mono">
        <TruncatedString
          text={text}
          maxLength={2000}
          headLength={800}
          tailLength={800}
          showStats={false}
        />
      </div>
    );
  }

  return (
    <div
      className="w-full rounded-lg border bg-accent p-4 font-mono overflow-auto"
      style={
        {
          // Use VSCode-themed CSS variables from globals.css
          '--shiki-color-text': 'var(--vscode-editor-foreground)',
          '--shiki-color-background': 'transparent',
          '--shiki-token-constant': 'var(--vscode-terminal-ansiBlue)',
          '--shiki-token-string': 'var(--vscode-terminal-ansiYellow)',
          '--shiki-token-keyword': 'var(--vscode-terminal-ansiRed)',
          '--shiki-token-function': 'var(--vscode-terminal-ansiMagenta)',
          '--shiki-token-parameter': 'var(--vscode-terminal-ansiCyan)',
          '--shiki-token-operator': 'var(--vscode-terminal-ansiRed)',
          '--shiki-token-punctuation': 'var(--vscode-editor-foreground)',
          '--shiki-token-property': 'var(--vscode-terminal-ansiGreen)',
          '--shiki-token-comment': 'var(--vscode-description-foreground)',
          '--shiki-token-variable': 'var(--vscode-terminal-ansiCyan)',
          '--shiki-token-number': 'var(--vscode-terminal-ansiBlue)',
          '--shiki-token-regexp': 'var(--vscode-terminal-ansiYellow)',
          '--shiki-token-escape': 'var(--vscode-terminal-ansiYellow)',
          '--shiki-token-symbol': 'var(--vscode-terminal-ansiBlue)',
          '--shiki-token-other': 'var(--vscode-editor-foreground)',
        } as React.CSSProperties
      }
    >
      <style>{`
        .curl-highlight pre {
          margin: 0 !important;
          padding: 0 !important;
          background: transparent !important;
          border: none !important;
          border-radius: 0 !important;
          font-family: var(--vscode-editor-font-family) !important;
          font-size: inherit !important;
          line-height: inherit !important;
          white-space: pre-wrap !important;
          word-wrap: break-word !important;
          overflow-wrap: break-word !important;
        }
        .curl-highlight code {
          background: transparent !important;
          border: none !important;
          border-radius: 0 !important;
          font-family: var(--vscode-editor-font-family) !important;
          font-size: inherit !important;
          line-height: inherit !important;
          white-space: pre-wrap !important;
          word-wrap: break-word !important;
          overflow-wrap: break-word !important;
        }
        .curl-highlight .shiki {
          background: transparent !important;
          border: none !important;
          border-radius: 0 !important;
          font-family: var(--vscode-editor-font-family) !important;
          font-size: inherit !important;
          line-height: inherit !important;
          white-space: pre-wrap !important;
          word-wrap: break-word !important;
          overflow-wrap: break-word !important;
          margin: 0 !important;
          padding: 0 !important;
        }
        .curl-highlight .shiki * {
          background: transparent !important;
        }
        .curl-highlight .shiki span {
          background: transparent !important;
        }
        .curl-highlight .shiki .line {
          background: transparent !important;
          white-space: pre-wrap !important;
          word-wrap: break-word !important;
          overflow-wrap: break-word !important;
        }
      `}</style>
      <div
        className="curl-highlight"
        // biome-ignore lint/security/noDangerouslySetInnerHtml: <explanation>
        dangerouslySetInnerHTML={{ __html: highlightedHtml }}
      />
    </div>
  );
};

export const PromptPreviewCurl = () => {
  const curl = useAtomValue(curlAtom);

  // Memoize the rendered content to prevent unnecessary re-renders
  const renderedContent = useMemo(() => {
    if (curl.state === 'loading') {
      return <Loader />;
    }

    if (curl.state === 'hasError') {
      return (
        <EnhancedErrorRenderer
          errorMessage={JSON.stringify(curl.error) || 'Unknown error'}
        />
      );
    }

    const value = curl.data;
    if (value === undefined) {
      return null;
    }

    if (value instanceof Error) {
      return (
        <EnhancedErrorRenderer
          errorMessage={value.message || 'Unknown error'}
        />
      );
    }
    return (
      <div className="relative group">
        <CopyButton
          text={value.curlTextWithoutSecrets}
          className="absolute top-1 right-1 opacity-0 transition-opacity group-hover:opacity-100 z-30"
          size="sm"
          variant="outline"
          showToast={false}
        />
        <SyntaxHighlightedCurl text={value.curlTextWithoutSecrets} />
      </div>
    );
  }, [curl]);

  return renderedContent;
};
