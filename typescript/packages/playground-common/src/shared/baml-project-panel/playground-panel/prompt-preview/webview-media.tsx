// import Link from "next/link";
import type { WasmChatMessagePartMedia } from '@gloo-ai/baml-schema-wasm-web';
/* eslint-disable @typescript-eslint/require-await */
import { useAtomValue, useSetAtom } from 'jotai';
import { ExternalLinkIcon, ImageIcon, Music, FileText, Video } from 'lucide-react';
import { useState, useEffect, useRef } from 'react';
import useSWR from 'swr';
import { wasmAtom } from '../../atoms';
import { showTokensAtom } from './render-text';
import { imageStatsMapAtom } from './image-stats-atom';

interface WebviewMediaProps {
  bamlMediaType: 'image' | 'audio' | 'pdf' | 'video';
  media: WasmChatMessagePartMedia;
}

// Helper function to convert base64 data URL to blob URL for better performance
const createBlobUrlFromBase64 = (base64DataUrl: string): string => {
  try {
    // Extract the base64 data and mime type
    const [header, data] = base64DataUrl.split(',');
    if (!header || !data) return base64DataUrl;
    
    const mimeMatch = header.match(/data:([^;]+)/);
    const mimeType = mimeMatch ? mimeMatch[1] : 'application/octet-stream';
    
    // Convert base64 to blob
    const byteCharacters = atob(data);
    const byteNumbers = new Array(byteCharacters.length);
    for (let i = 0; i < byteCharacters.length; i++) {
      byteNumbers[i] = byteCharacters.charCodeAt(i);
    }
    const byteArray = new Uint8Array(byteNumbers);
    const blob = new Blob([byteArray], { type: mimeType });
    
    // Create and return blob URL
    return URL.createObjectURL(blob);
  } catch (error) {
    console.warn('Failed to create blob URL from base64:', error);
    return base64DataUrl; // Fallback to original
  }
};

// Helper function to get user-friendly display text for media URLs
const getDisplayUrl = (url: string, mediaType: string): string => {
  if (url.startsWith('data:')) {
    const sizeMatch = url.match(/^data:[^;]+;base64,(.+)$/);
    if (sizeMatch && sizeMatch[1]) {
      const base64Length = sizeMatch[1].length;
      const sizeInBytes = base64Length * 0.75;
      const sizeFormatted = sizeInBytes > 1048576 
        ? `${(sizeInBytes / 1048576).toFixed(2)} MB` 
        : `${(sizeInBytes / 1024).toFixed(2)} KB`;
      return `Base64 ${mediaType} (${sizeFormatted})`;
    }
    return `Base64 ${mediaType}`;
  }
  return url;
};

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
  
  // Track blob URLs for cleanup
  const blobUrlRef = useRef<string | null>(null);
  const [optimizedMediaUrl, setOptimizedMediaUrl] = useState<string | null>(null);

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

  // Create optimized URL when mediaUrl changes
  useEffect(() => {
    if (!mediaUrl) {
      setOptimizedMediaUrl(null);
      return;
    }

    // Clean up previous blob URL
    if (blobUrlRef.current) {
      URL.revokeObjectURL(blobUrlRef.current);
      blobUrlRef.current = null;
    }

    // For base64 media (images, audio, and PDFs), create blob URL for better performance
    if (mediaUrl.startsWith('data:') && (bamlMediaType === 'image' || bamlMediaType === 'audio' || bamlMediaType === 'pdf')) {
      const blobUrl = createBlobUrlFromBase64(mediaUrl);
      if (blobUrl !== mediaUrl) {
        blobUrlRef.current = blobUrl;
        setOptimizedMediaUrl(blobUrl);
      } else {
        setOptimizedMediaUrl(mediaUrl);
      }
    } else {
      setOptimizedMediaUrl(mediaUrl);
    }
  }, [mediaUrl, bamlMediaType]);

  // Cleanup blob URLs on unmount
  useEffect(() => {
    return () => {
      if (blobUrlRef.current) {
        URL.revokeObjectURL(blobUrlRef.current);
      }
    };
  }, []);

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

    // Store in shared atom using original mediaUrl as key for consistency
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
            src={optimizedMediaUrl || ''}
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
            <source src={optimizedMediaUrl || ''} />
            Your browser does not support the audio element.
          </audio>
        );
      case 'pdf':
        return renderPdfContent(optimizedMediaUrl || mediaUrl || '');
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

    // For blob URLs or data URLs, use direct embed which works better in same-origin context
    if (url.startsWith('blob:') || url.startsWith('data:')) {
      return (
        <div className="w-full max-w-[600px] space-y-2">
          <div className="h-[500px] border rounded-lg overflow-hidden bg-white">
            <embed
              src={url}
              type="application/pdf"
              width="100%"
              height="100%"
              className="w-full h-full"
              title="PDF Document"
            />
          </div>
          <div className="flex items-center justify-between text-xs text-muted-foreground">
            {url.startsWith('data:') && (
              <span className="text-green-600">✓ Base64 content loaded</span>
            )}
            {url.startsWith('blob:') && (
              <span className="text-green-600">✓ Blob content loaded</span>
            )}
          </div>
        </div>
      );
    }

    // For HTTP URLs, use PDF.js viewer
    const pdfViewerUrl = `https://mozilla.github.io/pdf.js/web/viewer.html?file=${encodeURIComponent(url)}`;

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
              console.warn('PDF.js viewer failed to load');
            }}
          />
        </div>
        <div className="flex items-center justify-between text-xs text-muted-foreground">
          <span className="text-blue-600">✓ External PDF loaded</span>
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
            <span className="max-w-[150px] truncate">{getDisplayUrl(mediaUrl, bamlMediaType)}</span>
          </a>
        )}
      </div>
    </div>
  );
};
