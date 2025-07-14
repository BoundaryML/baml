// import Link from "next/link";
import type { WasmChatMessagePartMedia } from '@gloo-ai/baml-schema-wasm-web';
/* eslint-disable @typescript-eslint/require-await */
import { useAtomValue, useSetAtom } from 'jotai';
import { ExternalLinkIcon, ImageIcon, Music, FileText, Video } from 'lucide-react';
import { useState } from 'react';
import useSWR from 'swr';
import { wasmAtom } from '../../atoms';
import { showTokensAtom } from './render-text';
import { imageStatsMapAtom } from './image-stats-atom';

interface WebviewMediaProps {
  bamlMediaType: 'image' | 'audio' | 'pdf' | 'video';
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

  const renderMediaContent = () => {
    switch (bamlMediaType) {
      case 'image':
        return (
          <img
            src={mediaUrl}
            // biome-ignore lint/a11y/noRedundantAlt: not correct
            alt={'Image Not Found'}
            className="max-h-[400px] max-w-[400px] rounded-b-lg object-contain"
            onLoad={onImageLoad}
          />
        );
      case 'audio':
        return (
          // biome-ignore lint/a11y/useMediaCaption: not correct
          <audio controls className="p-2 w-full">
            <source src={mediaUrl} />
            Your browser does not support the audio element.
          </audio>
        );
      case 'pdf':
        return renderPdfContent(mediaUrl || '');
      case 'video':
        return renderVideoContent(mediaUrl || '');
      default:
        return null;
    }
  };

  const getYouTubeEmbedUrl = (url: string): string | null => {
    if (!url) return null;
    
    // Match various YouTube URL formats
    const patterns = [
      /(?:youtube\.com\/watch\?v=|youtu\.be\/|youtube\.com\/embed\/)([^&\n?#]+)/,
      /youtube\.com\/watch\?.*v=([^&\n?#]+)/
    ];
    
    for (const pattern of patterns) {
      const match = url.match(pattern);
      if (match && match[1]) {
        return `https://www.youtube.com/embed/${match[1]}`;
      }
    }
    
    return null;
  };

  const getVimeoEmbedUrl = (url: string): string | null => {
    if (!url) return null;
    
    const match = url.match(/vimeo\.com\/(?:video\/)?(\d+)/);
    if (match && match[1]) {
      return `https://player.vimeo.com/video/${match[1]}`;
    }
    
    return null;
  };

  const isDirectVideoFile = (url: string): boolean => {
    if (!url) return false;
    
    const videoExtensions = ['.mp4', '.webm', '.ogg', '.mov', '.avi', '.mkv', '.flv'];
    const urlLower = url.toLowerCase();
    
    return videoExtensions.some(ext => urlLower.includes(ext)) || 
           urlLower.startsWith('data:video/');
  };

  const renderVideoContent = (url: string) => {
    if (!url) {
      return (
        <div className="flex h-[300px] items-center justify-center rounded-lg bg-accent border-2 border-dashed border-muted-foreground/30">
          <p className="text-sm text-muted-foreground">No video URL available</p>
        </div>
      );
    }

    // Try YouTube first
    const youtubeEmbedUrl = getYouTubeEmbedUrl(url);
    if (youtubeEmbedUrl) {
      return (
        <div className="w-full max-w-[600px] aspect-video border rounded-lg overflow-hidden">
          <iframe
            src={youtubeEmbedUrl}
            width="100%"
            height="100%"
            className="w-full h-full"
            frameBorder="0"
            allow="accelerometer; autoplay; clipboard-write; encrypted-media; gyroscope; picture-in-picture"
            allowFullScreen
            title="YouTube video"
          />
        </div>
      );
    }

    // Try Vimeo
    const vimeoEmbedUrl = getVimeoEmbedUrl(url);
    if (vimeoEmbedUrl) {
      return (
        <div className="w-full max-w-[600px] aspect-video border rounded-lg overflow-hidden">
          <iframe
            src={vimeoEmbedUrl}
            width="100%"
            height="100%"
            className="w-full h-full"
            frameBorder="0"
            allow="autoplay; fullscreen; picture-in-picture"
            allowFullScreen
            title="Vimeo video"
          />
        </div>
      );
    }

    // Check if it's a direct video file
    if (isDirectVideoFile(url)) {
      return (
        // biome-ignore lint/a11y/useMediaCaption: not correct
        <video controls className="max-h-[400px] max-w-[600px] rounded-lg">
          <source src={url} />
          Your browser does not support the video element.
        </video>
      );
    }

    // Fallback: try to embed as iframe (for other video platforms)
    return (
      <div className="w-full max-w-[600px] space-y-2">
        <div className="aspect-video border rounded-lg overflow-hidden">
          <iframe
            src={url}
            width="100%"
            height="100%"
            className="w-full h-full"
            frameBorder="0"
            allow="autoplay; fullscreen; picture-in-picture"
            allowFullScreen
            title="Video content"
          />
        </div>
        <p className="text-xs text-muted-foreground text-center">
          If the video doesn't load, try opening the link directly
        </p>
      </div>
    );
  };

  const renderPdfContent = (url: string) => {
    if (!url) {
      return (
        <div className="flex h-[300px] items-center justify-center rounded-lg bg-accent border-2 border-dashed border-muted-foreground/30">
          <p className="text-sm text-muted-foreground">No PDF URL available</p>
        </div>
      );
    }

    // Normalize the URL - handle base64 content that might not be in data URL format
    let normalizedUrl = url;
    
    // If it's raw base64 content (not a data URL), convert it to a proper data URL
    if (!url.startsWith('http') && !url.startsWith('data:') && !url.startsWith('file:')) {
      // Assume it's base64 content
      normalizedUrl = `data:application/pdf;base64,${url}`;
    }

    // Use PDF.js web viewer for all PDF content (handles base64 and URLs properly)
    const pdfViewerUrl = `https://mozilla.github.io/pdf.js/web/viewer.html?file=${encodeURIComponent(normalizedUrl)}`;

    return (
      <div className="w-full max-w-[600px] space-y-2">
        <div className="h-[500px] border rounded-lg overflow-hidden bg-white">
          <iframe
            src={pdfViewerUrl}
            width="100%"
            height="100%"
            className="w-full h-full"
            title="PDF Viewer (PDF.js)"
            sandbox="allow-scripts allow-same-origin"
            onError={() => {
              // If PDF.js fails, we could fall back to a basic embed, but PDF.js is very reliable
              console.warn('PDF.js viewer failed to load');
            }}
          />
        </div>
        <div className="flex items-center justify-between text-xs text-muted-foreground">
          {/* <span>Powered by PDF.js</span> */}
          {normalizedUrl.startsWith('data:') && (
            <span className="text-green-600">✓ Base64 content loaded</span>
          )}
        </div>
      </div>
    );
  };

  return (
    <div className="w-full">
      <div className="relative w-full flex flex-col items-center bg-accent py-2 space-y-2">
        {renderMediaContent()}
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
