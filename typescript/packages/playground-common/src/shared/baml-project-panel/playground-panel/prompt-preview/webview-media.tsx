// import Link from "next/link";
import type { WasmChatMessagePartMedia } from '@gloo-ai/baml-schema-wasm-web';
/* eslint-disable @typescript-eslint/require-await */
import { useAtom, useAtomValue, useSetAtom } from 'jotai';
import { ExternalLinkIcon, ImageIcon, Music, FileText, Video, Copy, Check, X, ChevronDown, ChevronUp } from 'lucide-react';
import { useState, useEffect, useRef } from 'react';
import useSWR from 'swr';
import { Button } from '@baml/ui/button';
import { wasmAtom } from '../../atoms';
import { showTokensAtom } from './render-text';
import { imageStatsMapAtom } from './image-stats-atom';
import { mediaCollapsedMapAtom } from './media-collapsed-atom';
import { PdfViewer } from './pdf-viewer';

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

// Helper function to extract file format from URL or data URI
const getFileFormat = (url: string, mediaType: string): string => {
  if (url.startsWith('data:')) {
    const mimeMatch = url.match(/data:([^;]+)/);
    if (mimeMatch && mimeMatch[1]) {
      const mimeType = mimeMatch[1];
      // Extract format from mime type
      if (mimeType.includes('image/')) {
        return mimeType.replace('image/', '').toUpperCase();
      } else if (mimeType.includes('audio/')) {
        return mimeType.replace('audio/', '').toUpperCase();
      } else if (mimeType.includes('application/pdf')) {
        return 'PDF';
      } else if (mimeType.includes('video/')) {
        return mimeType.replace('video/', '').toUpperCase();
      }
    }
  } else {
    // Extract from URL extension
    const urlLower = url.toLowerCase();
    const extensionMatch = urlLower.match(/\.([a-z0-9]+)(?:\?|#|$)/);
    if (extensionMatch && extensionMatch[1]) {
      return extensionMatch[1].toUpperCase();
    }
  }
  return '';
};

// Helper function to get file size from data URI
const getDataUriSize = (url: string): string => {
  if (url.startsWith('data:')) {
    const sizeMatch = url.match(/^data:[^;]+;base64,(.+)$/);
    if (sizeMatch && sizeMatch[1]) {
      const base64Length = sizeMatch[1].length;
      const sizeInBytes = base64Length * 0.75;
      return sizeInBytes > 1048576 
        ? `${(sizeInBytes / 1048576).toFixed(2)} MB` 
        : `${(sizeInBytes / 1024).toFixed(2)} KB`;
    }
  }
  return '';
};

export const WebviewMedia: React.FC<WebviewMediaProps> = ({
  bamlMediaType,
  media,
}) => {
  const wasm = useAtomValue(wasmAtom);
  const isDebugMode = useAtomValue(showTokensAtom);
  const setImageStatsMap = useSetAtom(imageStatsMapAtom);
  const [mediaCollapsedMap, setMediaCollapsedMap] = useAtom(mediaCollapsedMapAtom);
  const [imageStats, setImageStats] = useState<{
    width: number;
    height: number;
    size: string;
  }>();
  
  // Track blob URLs for cleanup
  const blobUrlRef = useRef<string | null>(null);
  const [optimizedMediaUrl, setOptimizedMediaUrl] = useState<string | null>(null);
  
  // Create unique key for this media item and get its collapsed state
  const mediaKey = media.content;
  const collapsed = mediaCollapsedMap.get(mediaKey) ?? false;
  const setCollapsed = (newCollapsed: boolean) => {
    setMediaCollapsedMap((prev) => {
      const newMap = new Map(prev);
      newMap.set(mediaKey, newCollapsed);
      return newMap;
    });
  };
  

  
  // Copy status state
  const [copyStatus, setCopyStatus] = useState<'idle' | 'copying' | 'success' | 'error'>('idle');

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
      <div className="w-full flex justify-center">
        <div className="max-w-4xl w-full border border-[var(--vscode-panel-border)] rounded bg-[var(--vscode-editor-background)] p-4">
          <div className="flex h-[30vh] items-center justify-center">
            <div className="text-center space-y-3 text-[var(--vscode-charts-red)]">
              <div className="flex items-center justify-center gap-2 mb-2">
                {bamlMediaType === 'image' && <ImageIcon className="w-6 h-6" />}
                {bamlMediaType === 'audio' && <Music className="w-6 h-6" />}
                {bamlMediaType === 'pdf' && <FileText className="w-6 h-6" />}
                {bamlMediaType === 'video' && <Video className="w-6 h-6" />}
              </div>
              <p className="text-sm font-medium">Error loading {bamlMediaType}</p>
              <p className="text-xs text-[var(--vscode-charts-red)] bg-[var(--vscode-editor-background)] p-2 rounded border border-[var(--vscode-panel-border)] font-mono max-w-md">
                {error.message}
              </p>
            </div>
          </div>
        </div>
      </div>
    );
  }

  if (isLoading) {
    return (
      <div className="w-full flex justify-center">
        <div className="max-w-4xl w-full border border-[var(--vscode-panel-border)] rounded bg-[var(--vscode-editor-background)] p-4">
          <div className="flex h-[30vh] items-center justify-center">
            <div className="text-center space-y-3">
              <div className="w-8 h-8 border-2 border-[var(--vscode-panel-border)] border-t-[var(--vscode-foreground)] rounded-full animate-spin mx-auto"></div>
              <div className="flex items-center gap-2">
                {bamlMediaType === 'image' && <ImageIcon className="w-4 h-4 text-[var(--vscode-description-foreground)]" />}
                {bamlMediaType === 'audio' && <Music className="w-4 h-4 text-[var(--vscode-description-foreground)]" />}
                {bamlMediaType === 'pdf' && <FileText className="w-4 h-4 text-[var(--vscode-description-foreground)]" />}
                {bamlMediaType === 'video' && <Video className="w-4 h-4 text-[var(--vscode-description-foreground)]" />}
                <p className="text-sm text-[var(--vscode-description-foreground)]">
                  Loading {bamlMediaType}...
                </p>
              </div>
            </div>
          </div>
        </div>
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
          <div className="relative w-full flex items-center justify-center">
            <img
              src={optimizedMediaUrl || ''}
              // biome-ignore lint/a11y/noRedundantAlt: not correct
              alt={'Image Not Found'}
              className="max-w-full h-auto rounded object-contain border border-[var(--vscode-panel-border)]"
              onLoad={onImageLoad}
              style={{ maxHeight: '70vh' }}
            />
            {imageStats && isDebugMode && (
              <div className="max-h-sm absolute bottom-2 left-2 bg-[var(--vscode-editor-background)] text-[var(--vscode-foreground)] text-xs px-2 py-1 rounded border border-[var(--vscode-panel-border)]">
              {imageStats.width}×{imageStats.height} • {imageStats.size}
              </div>
            )}
          </div>
        );
      case 'audio':
        return (
          <div className="w-full max-w-2xl mx-auto bg-[var(--vscode-editor-background)] border border-[var(--vscode-panel-border)] rounded-lg shadow-sm p-4">
            <div className="flex items-center gap-3 mb-3">
              <Music className="w-5 h-5 text-[var(--vscode-description-foreground)]" />
              <span className="text-sm font-medium text-[var(--vscode-foreground)]">Audio Player</span>
            </div>
            {/* biome-ignore lint/a11y/useMediaCaption: not correct */}
            <audio controls className="w-full">
              <source src={optimizedMediaUrl || ''} />
              Your browser does not support the audio element.
            </audio>
          </div>
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
        <div className="flex h-[30vh] items-center justify-center rounded bg-[var(--vscode-editor-background)] border-2 border-dashed border-[var(--vscode-panel-border)]">
          <div className="text-center space-y-2">
            <Video className="w-8 h-8 mx-auto text-[var(--vscode-description-foreground)]" />
            <p className="text-sm text-[var(--vscode-description-foreground)]">No video URL available</p>
          </div>
        </div>
      );
    }

    // Try YouTube first
    const youtubeEmbedUrl = getYouTubeEmbedUrl(url);
    if (youtubeEmbedUrl) {
      return (
        <div className="w-full max-w-3xl mx-auto">
          <div className="aspect-video border border-[var(--vscode-panel-border)] rounded overflow-hidden">
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
        </div>
      );
    }

    // Try Vimeo
    const vimeoEmbedUrl = getVimeoEmbedUrl(url);
    if (vimeoEmbedUrl) {
      return (
        <div className="w-full max-w-3xl mx-auto">
          <div className="aspect-video border border-[var(--vscode-panel-border)] rounded overflow-hidden">
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
        </div>
      );
    }

    // Check if it's a direct video file
    if (isDirectVideoFile(url)) {
      return (
        <div className="w-full max-w-3xl mx-auto border border-[var(--vscode-panel-border)] rounded overflow-hidden bg-black">
          {/* biome-ignore lint/a11y/useMediaCaption: not correct */}
          <video controls className="w-full h-auto max-h-[50vh] object-contain">
            <source src={url} />
            Your browser does not support the video element.
          </video>
        </div>
      );
    }

    // Fallback: try to embed as iframe (for other video platforms)
    return (
      <div className="w-full max-w-3xl mx-auto">
        <div className="aspect-video border border-[var(--vscode-panel-border)] rounded overflow-hidden">
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
      </div>
    );
  };

  const renderPdfContent = (url: string) => {
    return <PdfViewer url={url} />;
  };

  const handleCopyToClipboard = async () => {
    if (mediaUrl) {
      setCopyStatus('copying');
      try {
        await navigator.clipboard.writeText(mediaUrl);
        setCopyStatus('success');
        // Reset to idle after 2 seconds
        setTimeout(() => setCopyStatus('idle'), 2000);
      } catch (err) {
        console.error('Failed to copy to clipboard:', err);
        setCopyStatus('error');
        // Reset to idle after 2 seconds
        setTimeout(() => setCopyStatus('idle'), 2000);
      }
    }
  };



  const isBase64 = mediaUrl?.startsWith('data:');
  const fileFormat = getFileFormat(mediaUrl || '', bamlMediaType);
  const fileSize = isBase64 ? getDataUriSize(mediaUrl || '') : '';

  return (
    <div className="w-full flex justify-center p-4 bg-[var(--vscode-sideBar-background)]">
      <div className={`border border-[var(--vscode-panel-border)] rounded bg-[var(--vscode-editor-background)] space-y-3 ${
        bamlMediaType === 'image' ? 'w-fit max-w-[90vw] min-w-80' : 'max-w-lg w-full'
      }`}>
        {/* Header with file type icon and link/copy */}
        {mediaUrl && (
          <div
            className="flex items-center justify-between px-3 py-2 border-b border-[var(--vscode-panel-border)] bg-[var(--vscode-sideBar-background)] cursor-pointer select-none overflow-hidden"
            onClick={e => {
              setCollapsed(!collapsed);
            }}
            tabIndex={0}
            role="button"
            aria-expanded={!collapsed}
            style={{ userSelect: 'none' }}
          >
            <div className="flex items-center gap-2 min-w-0 flex-1 overflow-hidden">
              {bamlMediaType === 'image' && <ImageIcon className="w-6 h-6 text-blue-400 flex-shrink-0" />}
              {bamlMediaType === 'audio' && <Music className="w-6 h-6 text-purple-400 flex-shrink-0" />}
              {bamlMediaType === 'pdf' && <FileText className="w-6 h-6 text-red-400 flex-shrink-0" />}
              {bamlMediaType === 'video' && <Video className="w-6 h-6 text-green-400 flex-shrink-0" />}
              <div className="flex flex-col min-w-0 overflow-hidden">
                <span className="text-xs font-medium text-[var(--vscode-foreground)] capitalize leading-tight truncate">
                  {bamlMediaType}
                </span>
                <span className="text-xs text-[var(--vscode-description-foreground)] leading-tight truncate">
                  {bamlMediaType === 'image' && imageStats ? 
                    `${fileFormat ? `${fileFormat} • ` : ''}${imageStats.width}×${imageStats.height}${fileSize ? ` • ${fileSize}` : imageStats.size ? ` • ${imageStats.size}` : ''}` :
                   bamlMediaType === 'audio' ? 
                    `${fileFormat || 'Unknown format'}${fileSize ? ` • ${fileSize}` : ''}` :
                   bamlMediaType === 'pdf' ? 
                    `${fileSize || 'Url'}` :
                   bamlMediaType === 'video' ? 
                    `${fileFormat || 'Video url'}${fileSize ? ` • ${fileSize}` : ''}` :
                   'Media file'}
                  {!isBase64 && mediaUrl && ` • ${getDisplayUrl(mediaUrl, bamlMediaType)}`}
                </span>
              </div>
            </div>
            {/* Button area, right-aligned, compact, no overflow */}
            <div className="flex items-center ml-2 flex-nowrap gap-1 h-7" onClick={e => e.stopPropagation()} style={{maxWidth: 'calc(100% - 2rem)'}}>
            {isBase64 ? (
              <Button
                onClick={handleCopyToClipboard}
                disabled={copyStatus === 'copying'}
                variant="outline"
                size="xs"
                className={`flex gap-1 items-center text-xs px-2 py-0 rounded flex-shrink-0 h-7 transition-all duration-200
                  ${copyStatus === 'success'
                    ? 'border-[var(--vscode-charts-green)] text-[var(--vscode-charts-green)] bg-[var(--vscode-editor-background)]'
                    : copyStatus === 'error'
                    ? 'border-[var(--vscode-charts-red)] text-[var(--vscode-charts-red)] bg-[var(--vscode-editor-background)]'
                    : ''
                  }`}
                style={{minWidth: 0, maxWidth: 140}}
              >
                {copyStatus === 'copying' && (
                  <div className="w-3 h-3 border border-[var(--vscode-button-foreground)] border-t-transparent rounded-full animate-spin" />
                )}
                {copyStatus === 'success' && <Check className="w-3 h-3" />}
                {copyStatus === 'error' && <X className="w-3 h-3" />}
                {copyStatus === 'idle' && <Copy className="w-3 h-3" />}
                <span className="truncate">
                  {copyStatus === 'copying' && `Copying...`}
                  {copyStatus === 'success' && `Copied!`}
                  {copyStatus === 'error' && `Failed`}
                  {copyStatus === 'idle' && `Copy Base64`}
                </span>
              </Button>
            ) : (
              <Button
                asChild
                variant="outline"
                size="xs"
                className="flex gap-1 items-center text-xs px-2 py-0 rounded border flex-shrink-0 h-7 transition-all duration-200"
                style={{minWidth: 0, maxWidth: 140}}
              >
                <a
                  href={mediaUrl}
                  target="_blank"
                  rel="noopener noreferrer"
                  className="flex gap-1 items-center w-full h-full truncate"
                  style={{maxWidth: 120}}
                >
                  <ExternalLinkIcon className="w-3 h-3" />
                  <span className="truncate">
                    {(() => {
                      const url = mediaUrl || '';
                      const urlParts = url.split('/');
                      const filename = urlParts[urlParts.length - 1];
                      const cleanFilename = filename?.split('?')[0]?.split('#')[0];
                      if (cleanFilename && cleanFilename.length > 0 && cleanFilename.includes('.')) {
                        return `Open ${cleanFilename.length > 20 ? cleanFilename.substring(0, 17) + '...' : cleanFilename}`;
                      }
                      return 'Open Link';
                    })()}
                  </span>
                </a>
              </Button>
            )}
            {/* Collapse/Expand Button (rightmost) */}
            <Button
              onClick={e => { e.stopPropagation(); setCollapsed(!collapsed); }}
              aria-label={collapsed ? 'Expand media' : 'Collapse media'}
              variant="outline"
              size="xs"
              className="ml-1 flex items-center justify-center px-1.5 py-0 h-7 transition-colors duration-150 flex-shrink-0"
              style={{ outline: 'none', minWidth: 0 }}
              tabIndex={0}
            >
              {collapsed ? (
                <ChevronDown className="w-4 h-4" />
              ) : (
                <ChevronUp className="w-4 h-4" />
              )}
            </Button>
            </div>
          </div>
        )}

        {/* Media content (collapsible) */}
        {!collapsed && (
          <div className="p-4">
            {renderMediaContent()}
          </div>
        )}
      </div>
    </div>
  );
};
