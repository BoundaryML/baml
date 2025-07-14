import { useAtomValue, useSetAtom } from 'jotai';
import type React from 'react';
import { displaySettingsAtom } from '../preview-toolbar';
import { showTokensAtom } from './render-text';
import type { WasmChatMessagePart } from '@gloo-ai/baml-schema-wasm-web';
import { useState, useEffect, useMemo } from 'react';
import { imageStatsMapAtom } from './image-stats-atom';
import useSWR from 'swr';
import { wasmAtom } from '../../atoms';
import { Button } from '@baml/ui/button';

interface ImageStats {
  width: number;
  height: number;
  tokens: number;
}

export const PromptStats: React.FC<{
  text: string;
  parts?: WasmChatMessagePart[];
}> = ({ text, parts }) => {
  const showTokenCounts = useAtomValue(showTokensAtom);
  const setDisplaySettings = useSetAtom(displaySettingsAtom);
  const numberFormatter = new Intl.NumberFormat();
  const imageStatsMap = useAtomValue(imageStatsMapAtom);
  const wasm = useAtomValue(wasmAtom);

  // Extract media URLs from parts
  const mediaUrls = useMemo(() => {
    if (!parts || !wasm) return [];

    const urls: string[] = [];
    parts.forEach(part => {
      if (part.is_image?.()) {
        const media = part.as_media();
        if (media) {
          switch (media.type) {
            case wasm.WasmChatMessagePartMediaType.File:
              urls.push(media.content);
              break;
            case wasm.WasmChatMessagePartMediaType.Url:
              urls.push(media.content);
              break;
          }
        }
      }
    });
    return urls;
  }, [parts, wasm]);

  // Calculate image tokens based on dimensions
  const imageTokensInfo = useMemo(() => {
    let totalTokens = 0;
    let imageCount = 0;

    mediaUrls.forEach(url => {
      const stats = imageStatsMap.get(url);
      if (stats) {
        // Use the same calculation as in webview-media
        const tokens = Math.ceil((stats.width * stats.height) / 750);
        totalTokens += tokens;
        imageCount++;
      } else {
        // Default estimate if dimensions not yet loaded
        totalTokens += 85;
        imageCount++;
      }
    });

    return { totalTokens, imageCount };
  }, [mediaUrls, imageStatsMap]);

  const textTokens = Math.ceil(text.length / 4);
  const totalTokens = textTokens + imageTokensInfo.totalTokens;


  return (
    <>
    {showTokenCounts && (
    <div className="flex flex-row sm:gap-4 justify-between items-stretch px-2 py-2 text-xs border border-border bg-muted text-muted-foreground rounded-b w-full">
      <div className="flex flex-wrap gap-y-2 gap-x-5 sm:gap-x-4 w-full sm:w-auto">
        <div className="flex flex-col items-start min-w-[60px]">
          <span className="text-muted-foreground/60">Characters</span>
          <span className="font-medium">
            {numberFormatter.format(text.length)}
          </span>
        </div>
        <div className="flex flex-col items-start min-w-[60px]">
          <span className="text-muted-foreground/60">Words</span>
          <span className="font-medium">
            {numberFormatter.format(text.split(/\s+/).filter(Boolean).length)}
          </span>
        </div>
        <div className="flex flex-col items-start min-w-[60px]">
          <span className="text-muted-foreground/60">Lines</span>
          <span className="font-medium">
            {numberFormatter.format(text.split('\n').length)}
          </span>
        </div>
        {imageTokensInfo.imageCount > 0 && (
          <div className="flex flex-col items-start min-w-[60px]">
            <span className="text-muted-foreground/60">Images</span>
            <span className="font-medium">
              {numberFormatter.format(imageTokensInfo.imageCount)}
            </span>
          </div>
        )}
        <div className="flex flex-col items-start min-w-[80px]">
          <span className="text-muted-foreground/60">Tokens (est.)</span>
          <span className="font-medium">
            {numberFormatter.format(totalTokens)}
            {imageTokensInfo.imageCount > 0 && (
              <span className="text-muted-foreground/80 ml-1 text-[10px]">
                ({numberFormatter.format(textTokens)}+{numberFormatter.format(imageTokensInfo.totalTokens)})
              </span>
            )}
          </span>
        </div>
      </div>
    </div>
    )}
    
    </>
  );
};

