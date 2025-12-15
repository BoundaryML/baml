import { cn } from '@baml/ui/lib/utils';
import { atom, useAtom, useAtomValue, useSetAtom } from 'jotai';
import { useMemo, useState, useEffect, useRef, useId } from 'react';
import React from 'react';
import { displaySettingsAtom } from '../preview-toolbar';
import { getHighlightedParts } from './highlight-utils';
import { TokenEncoderCache } from './render-tokens';
import type { Tiktoken } from 'js-tiktoken/lite';
import {
  promptSearchQueryAtom,
  promptSearchCurrentMatchAtom,
  registerMatchCountAtom,
  unregisterMatchCountAtom,
  matchOffsetsAtom,
} from './search-atoms';

export const showTokensAtom = atom(
  (get) => get(displaySettingsAtom).showTokens,
);

// Global counter for tracking matches across all HighlightedText instances
let globalMatchCounter = 0;
let globalMatchCounterResetKey = '';

const HighlightedText: React.FC<{
  text: string;
  highlightChunks: string[];
  searchQuery?: string;
  currentSearchMatch?: number;
  globalOffset?: number; // Global offset for this component's matches
  onMatchCount?: (count: number) => void;
}> = ({ text, highlightChunks, searchQuery, currentSearchMatch = 0, globalOffset = 0, onMatchCount }) => {
  const matchRefs = useRef<(HTMLElement | null)[]>([]);

  // First apply regular highlighting
  const baseParts = getHighlightedParts(text, highlightChunks);

  // Then apply search highlighting on top
  const partsWithSearch = useMemo(() => {
    if (!searchQuery || searchQuery.trim() === '') {
      return baseParts.map(part => ({ ...part, isSearchMatch: false, localMatchIndex: -1 }));
    }

    const searchLower = searchQuery.toLowerCase();
    let localMatchIndex = 0;

    return baseParts.flatMap(part => {
      const text = part.text;
      const textLower = text.toLowerCase();

      // Find all occurrences of search term in this part
      const segments: Array<{ text: string; highlight: boolean; isSearchMatch: boolean; localMatchIndex: number }> = [];
      let lastIndex = 0;
      let searchIndex = textLower.indexOf(searchLower, lastIndex);

      while (searchIndex !== -1) {
        // Add text before the match
        if (searchIndex > lastIndex) {
          segments.push({
            text: text.slice(lastIndex, searchIndex),
            highlight: part.highlight,
            isSearchMatch: false,
            localMatchIndex: -1,
          });
        }

        // Add the search match
        segments.push({
          text: text.slice(searchIndex, searchIndex + searchQuery.length),
          highlight: part.highlight,
          isSearchMatch: true,
          localMatchIndex: localMatchIndex++,
        });

        lastIndex = searchIndex + searchQuery.length;
        searchIndex = textLower.indexOf(searchLower, lastIndex);
      }

      // Add remaining text
      if (lastIndex < text.length) {
        segments.push({
          text: text.slice(lastIndex),
          highlight: part.highlight,
          isSearchMatch: false,
          localMatchIndex: -1,
        });
      }

      return segments.length > 0 ? segments : [{ ...part, isSearchMatch: false, localMatchIndex: -1 }];
    });
  }, [baseParts, searchQuery]);

  // Count matches and report
  useEffect(() => {
    const matchCount = partsWithSearch.filter(p => p.isSearchMatch).length;
    onMatchCount?.(matchCount);
  }, [partsWithSearch, onMatchCount]);

  // Find which local match index corresponds to the current global match
  const currentLocalMatch = currentSearchMatch - globalOffset;

  // Get the local match count
  const localMatchCount = partsWithSearch.filter(p => p.isSearchMatch).length;

  // Scroll current match into view (only if current match is within this component)
  useEffect(() => {
    if (currentLocalMatch >= 0 && currentLocalMatch < localMatchCount) {
      const currentMatchElement = matchRefs.current[currentLocalMatch];
      if (currentMatchElement) {
        currentMatchElement.scrollIntoView({ behavior: 'smooth', block: 'center' });
      }
    }
  }, [currentLocalMatch, localMatchCount]);

  return (
    <>
      {partsWithSearch.map((part, i) => {
        // Check if this match is the current global match
        const isCurrentMatch = part.isSearchMatch && part.localMatchIndex === currentLocalMatch;

        if (part.isSearchMatch) {
          return (
            <mark
              key={`${i}-search-${part.localMatchIndex}`}
              ref={(el) => {
                if (part.localMatchIndex >= 0) {
                  matchRefs.current[part.localMatchIndex] = el;
                }
              }}
              className={cn(
                'inline whitespace-pre-wrap break-words rounded px-0.5 font-normal text-xs',
                isCurrentMatch
                  ? 'bg-yellow-400 text-black ring-2 ring-yellow-600'
                  : 'bg-yellow-300/70 text-black',
              )}
            >
              {part.text}
            </mark>
          );
        }

        if (part.highlight) {
          return (
            <mark
              key={`${i}-${part.highlight}-${part.text.length}`}
              className={cn(
                'inline whitespace-pre-wrap break-words rounded px-1 py-0.5 font-normal text-xs text-primary-foreground',
                part.text.trim() === '' ? 'bg-chart-5/30' : 'bg-chart-1/40',
              )}
            >
              {part.text}
            </mark>
          );
        }

        return (
          <React.Fragment key={`${i}-normal-${part.text.length}`}>
            {part.text}
          </React.Fragment>
        );
      })}
    </>
  );
};

export const RenderPromptPart: React.FC<{
  text: string;
  highlightChunks?: string[];
  model?: string;
  provider?: string;
}> = ({ text, highlightChunks = [], model, provider }) => {
  const componentId = useId();
  const showTokens = useAtomValue(showTokensAtom);
  const searchQuery = useAtomValue(promptSearchQueryAtom);
  const currentSearchMatch = useAtomValue(promptSearchCurrentMatchAtom);
  const matchOffsets = useAtomValue(matchOffsetsAtom);
  const registerMatchCount = useSetAtom(registerMatchCountAtom);
  const unregisterMatchCount = useSetAtom(unregisterMatchCountAtom);
  const [encoder, setEncoder] = useState<Tiktoken | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [localMatchCount, setLocalMatchCount] = useState(0);

  // Get the global offset for this component's matches
  const globalOffset = matchOffsets.get(componentId) ?? 0;

  // Register/update match count with the global registry
  useEffect(() => {
    registerMatchCount({ id: componentId, count: localMatchCount });
  }, [localMatchCount, componentId, registerMatchCount]);

  // Cleanup on unmount
  useEffect(() => {
    return () => {
      unregisterMatchCount(componentId);
    };
  }, [componentId, unregisterMatchCount]);

  // Load encoder asynchronously when needed
  useEffect(() => {
    if (!showTokens) {
      setEncoder(null);
      return;
    }

    const encodingName = TokenEncoderCache.getEncodingNameForModel(
      'baml-openai-chat',
      'gpt-4o',
    );
    if (!encodingName) return;

    // Check if already cached
    const cached = TokenEncoderCache.INSTANCE.getEncoder(encodingName);
    if (cached) {
      setEncoder(cached);
      return;
    }

    // Load asynchronously
    setIsLoading(true);
    TokenEncoderCache.INSTANCE.getEncoderAsync(encodingName)
      .then(enc => {
        setEncoder(enc);
        setIsLoading(false);
      })
      .catch(err => {
        console.error('Failed to load tokenizer:', err);
        setIsLoading(false);
      });
  }, [showTokens, model, provider]);

  const tokenizer = useMemo(() => {
    if (!showTokens || !encoder) return undefined;
    return { enc: encoder, tokens: encoder.encode(text) };
  }, [text, showTokens, encoder]);

  // Only compute highlighted text if we're not tokenizing
  const renderContent = useMemo(() => {
    // Show loading state while encoder is being loaded
    if (showTokens && isLoading) {
      return <span className="text-muted-foreground">Loading tokenizer...</span>;
    }

    if (tokenizer) {
      const tokenized = Array.from(tokenizer.tokens).map((token) =>
        tokenizer.enc.decode([token]),
      );
      return (
        <>
          {tokenized.map((token, i) => (
            <span
              key={`${i}-token-${token.length}-${token.charCodeAt(0) || 0}`}
              className={cn(
                'text-white',
                // Uncomment and use these classes if you want to color-code tokens
                [
                  'bg-fuchsia-800',
                  'bg-emerald-700',
                  'bg-yellow-600',
                  'bg-red-700',
                  'bg-cyan-700',
                ][i % 5],
              )}
            >
              {token}
            </span>
          ))}
        </>
      );
    }

    // Only do highlighting if we're not tokenizing
    return (
      <HighlightedText
        text={text}
        highlightChunks={highlightChunks}
        searchQuery={searchQuery}
        currentSearchMatch={currentSearchMatch}
        globalOffset={globalOffset}
        onMatchCount={setLocalMatchCount}
      />
    );
  }, [text, highlightChunks, tokenizer, showTokens, isLoading, searchQuery, currentSearchMatch, globalOffset]);

  return (
    <div className="flex flex-col min-w-0">
      <div className="px-3 pb-3 pt-0 bg-accent group max-h-[600px] overflow-y-auto overflow-x-hidden min-w-0">
        <pre
          className={cn(
            'whitespace-pre-wrap text-xs leading-relaxed transition-all text-primary-foreground min-w-0',
          )}
        >
          {renderContent}
        </pre>
      </div>
    </div>
  );
};
