import { CopyButton } from '@baml/ui/custom/copy-button';
import { useAtomValue, useSetAtom } from 'jotai';
import { atom } from 'jotai';
import { loadable } from 'jotai/utils';
import { useTheme } from 'next-themes';
import { useEffect, useState, memo, useRef, useCallback } from 'react';
import type React from 'react';
import { useMemo } from 'react';
import { apiKeysAtom } from '../../../../components/api-keys-dialog/atoms';
import { ctxAtom, filesAtom, runtimeAtom } from '../../atoms';
import { selectionAtom } from '../atoms';
import { TruncatedString } from './TruncatedString';
import { Loader } from './components';
import { vscode } from '../../vscode';
import { EnhancedErrorRenderer } from './test-panel/components/EnhancedErrorRenderer';
import { runtimeInstanceAtom } from '../../../../sdk/atoms/core.atoms';
import {
  promptSearchQueryAtom,
  promptSearchCurrentMatchAtom,
  registerMatchCountAtom,
  unregisterMatchCountAtom,
  matchOffsetsAtom,
} from './search-atoms';
import { cn } from '@baml/ui/lib/utils';

type CurlResult =
  | {
    curlTextWithoutSecrets: string;
    curlTextWithSecrets: string;
  }
  | undefined
  | Error;

const baseCurlAtom = atom<Promise<CurlResult>>(async (get) => {
  const runtime = get(runtimeInstanceAtom);
  const ctx = get(ctxAtom);
  const envVars = get(apiKeysAtom);
  const files = get(filesAtom); // Add files dependency to track content changes
  const { selectedFn, selectedTc } = get(selectionAtom);


  if (!selectedFn || !runtime || !selectedTc) {
    console.log('[curl] no selectedFn or runtime or selectedTc');
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
    // Use runtime interface method instead of calling WASM directly
    curlTextWithoutSecrets = await runtime.renderCurlForTest(
      selectedFn.name,
      selectedTc.name,
      {
        stream: false,
        expandImages: false,
        exposeSecrets: false,
      },
      {
        apiKeys: envVars,
        loadMediaFile: vscode.loadMediaFile,
      }
    );


    curlTextWithSecrets = await runtime.renderCurlForTest(
      selectedFn.name,
      selectedTc.name,
      {
        stream: false,
        expandImages: false,
        exposeSecrets: true,
      },
      {
        apiKeys: envVars,
        loadMediaFile: vscode.loadMediaFile,
      }
    );
  } catch (error) {
    console.error('[curl] error', error);
    return error as Error;
  }

  return {
    curlTextWithoutSecrets,
    curlTextWithSecrets,
  };
});

export const curlAtom = loadable(baseCurlAtom);

// Helper function to escape HTML entities
const escapeHtml = (text: string): string => {
  return text
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#039;');
};

// Helper function to apply search highlighting to HTML content
const applySearchHighlightToHtml = (
  html: string,
  searchQuery: string,
  currentMatch: number
): { html: string; matchCount: number } => {
  if (!searchQuery || searchQuery.trim() === '') {
    return { html, matchCount: 0 };
  }

  // We need to find and highlight text content while preserving HTML tags
  // Strategy: Extract text nodes, find matches, and wrap them with highlight spans

  const searchLower = searchQuery.toLowerCase();
  let matchCount = 0;
  let currentMatchIndex = 0;

  // Split HTML into tags and text content
  const parts = html.split(/(<[^>]+>)/g);

  const processedParts = parts.map((part) => {
    // If it's an HTML tag, keep it as is
    if (part.startsWith('<')) {
      return part;
    }

    // It's text content - apply search highlighting
    const textLower = part.toLowerCase();
    let result = '';
    let lastIndex = 0;
    let searchIndex = textLower.indexOf(searchLower, lastIndex);

    while (searchIndex !== -1) {
      // Add text before the match
      if (searchIndex > lastIndex) {
        result += part.slice(lastIndex, searchIndex);
      }

      // Add the search match with highlight
      const matchText = part.slice(searchIndex, searchIndex + searchQuery.length);
      const isCurrentMatch = currentMatchIndex === currentMatch;
      const highlightClass = isCurrentMatch
        ? 'search-match search-match-current'
        : 'search-match';

      result += `<mark class="${highlightClass}" data-match-index="${currentMatchIndex}">${matchText}</mark>`;

      matchCount++;
      currentMatchIndex++;
      lastIndex = searchIndex + searchQuery.length;
      searchIndex = textLower.indexOf(searchLower, lastIndex);
    }

    // Add remaining text
    if (lastIndex < part.length) {
      result += part.slice(lastIndex);
    }

    return result || part;
  });

  return { html: processedParts.join(''), matchCount };
};

// Syntax highlighting component for curl commands
const SyntaxHighlightedCurl = memo(({ text }: { text: string }) => {
  const [highlightedHtml, setHighlightedHtml] = useState<string>('');
  const [highlighter, setHighlighter] = useState<any | undefined>(undefined);
  const { theme } = useTheme();
  const searchQuery = useAtomValue(promptSearchQueryAtom);
  const currentSearchMatch = useAtomValue(promptSearchCurrentMatchAtom);
  const matchOffsets = useAtomValue(matchOffsetsAtom);
  const registerMatchCount = useSetAtom(registerMatchCountAtom);
  const unregisterMatchCount = useSetAtom(unregisterMatchCountAtom);
  const containerRef = useRef<HTMLDivElement>(null);
  const componentId = 'curl-highlight';

  // Get the global offset for this component's matches
  const globalOffset = matchOffsets.get(componentId) ?? 0;

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

  // Compute the local match index (which match within this component is current)
  const currentLocalMatch = currentSearchMatch - globalOffset;

  // Apply search highlighting on top of syntax highlighting
  const { finalHtml, matchCount } = useMemo(() => {
    if (!highlightedHtml) {
      return { finalHtml: '', matchCount: 0 };
    }
    // Pass local match index instead of global
    const { html, matchCount } = applySearchHighlightToHtml(
      highlightedHtml,
      searchQuery,
      currentLocalMatch
    );
    return { finalHtml: html, matchCount };
  }, [highlightedHtml, searchQuery, currentLocalMatch]);

  // Register match count with global registry
  useEffect(() => {
    registerMatchCount({ id: componentId, count: matchCount });
  }, [matchCount, registerMatchCount]);

  // Cleanup on unmount
  useEffect(() => {
    return () => {
      unregisterMatchCount(componentId);
    };
  }, [unregisterMatchCount]);

  // Scroll current match into view (only if current match is within this component)
  useEffect(() => {
    if (containerRef.current && searchQuery && currentLocalMatch >= 0 && currentLocalMatch < matchCount) {
      const currentMatchElement = containerRef.current.querySelector(
        '.search-match-current'
      );
      if (currentMatchElement) {
        currentMatchElement.scrollIntoView({ behavior: 'smooth', block: 'center' });
      }
    }
  }, [currentLocalMatch, searchQuery, finalHtml, matchCount]);

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
      ref={containerRef}
      className="w-full rounded-lg border bg-accent p-4 font-mono overflow-auto text-xs"
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
        .curl-highlight .shiki span:not(.search-match) {
          background: transparent !important;
        }
        .curl-highlight .shiki .line {
          background: transparent !important;
          white-space: pre-wrap !important;
          word-wrap: break-word !important;
          overflow-wrap: break-word !important;
        }
        .curl-highlight .search-match {
          background-color: rgba(250, 204, 21, 0.7) !important;
          color: black !important;
          border-radius: 2px;
          padding: 0 2px;
        }
        .curl-highlight .search-match-current {
          background-color: rgb(250, 204, 21) !important;
          color: black !important;
          outline: 2px solid rgb(202, 138, 4);
          outline-offset: -1px;
        }
      `}</style>
      <div
        className="curl-highlight"
        // biome-ignore lint/security/noDangerouslySetInnerHtml: <explanation>
        dangerouslySetInnerHTML={{ __html: finalHtml }}
      />
    </div>
  );
}, (prev, next) => prev.text === next.text);

export const PromptPreviewCurl = () => {
  const curl = useAtomValue(curlAtom);
  const [lastCurl, setLastCurl] = useState<
    | { curlTextWithoutSecrets: string; curlTextWithSecrets: string }
    | undefined
  >(undefined);


  useEffect(() => {
    if (curl.state === 'hasData' && curl.data && !(curl.data instanceof Error)) {
      setLastCurl(curl.data);
    }
  }, [curl]);

  // Memoize the rendered content to prevent unnecessary re-renders
  const renderedContent = useMemo(() => {
    if (curl.state === 'loading') {
      // While loading, show the last known cURL if available, otherwise a loader
      if (lastCurl) {
        return (
          <div className="relative group">
            <CopyButton
              text={lastCurl.curlTextWithoutSecrets}
              className="absolute top-1 right-1 opacity-0 transition-opacity group-hover:opacity-100 z-30"
              size="sm"
              variant="outline"
              showToast={false}
            />
            <SyntaxHighlightedCurl text={lastCurl.curlTextWithoutSecrets} />
          </div>
        );
      }
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
  }, [curl, lastCurl]);

  return renderedContent;
};
