// import Link from "next/link";
import type { WasmChatMessagePartMedia } from '@gloo-ai/baml-schema-wasm-web';
/* eslint-disable @typescript-eslint/require-await */
import { useAtomValue, useSetAtom } from 'jotai';
import { ExternalLinkIcon, ImageIcon, Music } from 'lucide-react';
import { useState } from 'react';
import useSWR from 'swr';
import { wasmAtom } from '../../atoms';
import { showTokensAtom } from './render-text';
import { imageStatsMapAtom } from './image-stats-atom';

interface WebviewMediaProps {
  bamlMediaType: 'image' | 'audio';
  media: WasmChatMessagePartMedia;
}

export const WebviewMedia: React.FC<WebviewMediaProps> = ({
  bamlMediaType,
  media,
}) => {
  const wasm = useAtomValue(wasmAtom);
  const isDebugMode = useAtomValue(showTokensAtom);
  const setImageStatsMap = useSetAtom(imageStatsMapAtom);
  const [imageStats, setImageStats] = useState<{
    width: number;
    height: number;
    size: string;
  }>();

  const {
    data: mediaUrl,
    error,
    isLoading,
  } = useSWR(
    { swr: 'WebviewMedia', type: media.type, content: media.content },
    async () => {
      if (!wasm) {
        throw new Error('wasm not loaded');
      }

      switch (media.type) {
        case wasm.WasmChatMessagePartMediaType.File:
          return `${media.content}`
        case wasm.WasmChatMessagePartMediaType.Url:
          return media.content;
        case wasm.WasmChatMessagePartMediaType.Error:
          throw new Error(media.content);
        default:
          throw new Error('unknown media type');
      }
    },
  );

  if (error) {
    return (
      <div className="px-4 py-3 rounded-lg bg-destructive/15 text-destructive">
        <p className="text-sm font-medium">Error loading {bamlMediaType}</p>
        <p className="mt-1 text-xs">{error.message}</p>
      </div>
    );
  }

  if (isLoading) {
    return (
      <div className="flex h-[200px] items-center justify-center rounded-lg bg-accent">
        <p className="text-sm text-muted-foreground">
          Loading {bamlMediaType}...
        </p>
      </div>
    );
  }

  const onImageLoad = (e: React.SyntheticEvent<HTMLImageElement>) => {
    const img = e.currentTarget
    const { naturalWidth, naturalHeight } = img
    let size = 'Unknown'
    if (mediaUrl?.startsWith('data:')) {
      const base64Length = mediaUrl.split(',')[1]?.length
      const sizeInBytes = base64Length ? base64Length * 0.75 : 0
      size =
        sizeInBytes > 1048576 ? `${(sizeInBytes / 1048576).toFixed(2)} MB` : `${(sizeInBytes / 1024).toFixed(2)} KB`
    } else {
    const sizeInBytes = naturalWidth * naturalHeight * 4
      size =
        sizeInBytes > 1048576 ? `${(sizeInBytes / 1048576).toFixed(2)} MB` : `${(sizeInBytes / 1024).toFixed(2)} KB`
    }
    const stats = { width: naturalWidth, height: naturalHeight, size };
    setImageStats(stats);

    // Store in shared atom
    if (mediaUrl) {
      setImageStatsMap((prev) => {
        const newMap = new Map(prev);
        newMap.set(mediaUrl, { ...stats, url: mediaUrl });
        return newMap;
      });
    }
  }

  return (
    <div className="w-full">
      <div className="relative w-full flex flex-col items-center bg-accent py-2 space-y-2">
        {bamlMediaType === 'image' ? (
          <img
            src={mediaUrl}
            // biome-ignore lint/a11y/noRedundantAlt: not correct
            alt={'Image Not Found'}
            className="max-h-[400px] max-w-[400px] rounded-b-lg object-contain"
            onLoad={onImageLoad}
          />
        ) : (
          // biome-ignore lint/a11y/useMediaCaption: not correct
          <audio controls className="p-2 w-full">
            <source src={mediaUrl} />
            Your browser does not support the audio element.
          </audio>
        )}
        {mediaUrl && (
          <a
            href={mediaUrl}
            target="_blank"
            rel="noopener noreferrer"
            className="flex gap-1 items-center transition-colors hover:text-primary text-xs"
          >
            <ExternalLinkIcon className="w-3 h-3" />
            <span className="max-w-[150px] truncate">{mediaUrl}</span>
          </a>
        )}
      </div>
    </div>
  );
};
