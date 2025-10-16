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
      // Check for all media types
      if (part.is_image?.() || part.is_audio?.() || part.is_pdf?.() || part.is_video?.()) {
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

  // Calculate media tokens - images use dimension-based calculation, others use default estimate
  const mediaTokensInfo = useMemo(() => {
    let totalTokens = 0;
    let imageCount = 0;
    let audioCount = 0;
    let pdfCount = 0;
    let videoCount = 0;

    if (!parts || !wasm) return { totalTokens: 0, imageCount: 0, audioCount: 0, pdfCount: 0, videoCount: 0, totalMediaCount: 0 };

    parts.forEach(part => {
      if (part.is_image?.()) {
        const media = part.as_media();
        if (media) {
          const url = media.content;
          const stats = imageStatsMap.get(url);
          if (stats) {
            // Use the same calculation as in webview-media
            const tokens = Math.ceil((stats.width * stats.height) / 750);
            totalTokens += tokens;
          } else {
            // Default estimate if dimensions not yet loaded
            totalTokens += 85;
          }
          imageCount++;
        }
      } else if (part.is_audio?.()) {
        // Audio default token estimate
        totalTokens += 50;
        audioCount++;
      } else if (part.is_pdf?.()) {
        // PDF default token estimate (higher due to potential text content)
        totalTokens += 200;
        pdfCount++;
      } else if (part.is_video?.()) {
        // Video default token estimate (higher due to visual content)
        totalTokens += 150;
        videoCount++;
      }
    });

    const totalMediaCount = imageCount + audioCount + pdfCount + videoCount;
    return { totalTokens, imageCount, audioCount, pdfCount, videoCount, totalMediaCount };
  }, [parts, wasm, imageStatsMap]);

  const textTokens = Math.ceil(text.length / 4);
  const totalTokens = textTokens + mediaTokensInfo.totalTokens;


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
        {mediaTokensInfo.totalMediaCount > 0 && (
          <div className="flex flex-col items-start min-w-[60px]">
            <span className="text-muted-foreground/60">Media</span>
            <span className="font-medium">
              {numberFormatter.format(mediaTokensInfo.totalMediaCount)}
            </span>
          </div>
        )}
        <div className="flex flex-col items-start min-w-[80px]">
          <span className="text-muted-foreground/60">Tokens (est.)</span>
          <span className="font-medium">
            {numberFormatter.format(totalTokens)}
            {mediaTokensInfo.totalMediaCount > 0 && (
              <span className="text-muted-foreground/80 ml-1 text-[10px]">
                ({numberFormatter.format(textTokens)}+{numberFormatter.format(mediaTokensInfo.totalTokens)})
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

