import { ChevronLeft, ChevronRight, FileText, ZoomIn, ZoomOut, RotateCcw } from 'lucide-react';
import { useState, useEffect, useRef, useCallback } from 'react';
// @ts-ignore - react-pdf types are handled at runtime
import { Document, Page, pdfjs } from 'react-pdf';
// Note: CSS imports moved to webview-media.tsx to avoid Vite modulepreload issues in VSCode webviews

// Configure PDF.js worker - use CDN to ensure version compatibility
// react-pdf 9.0.0 uses pdfjs-dist API version 4.3.136
pdfjs.GlobalWorkerOptions.workerSrc = `https://unpkg.com/pdfjs-dist@4.3.136/build/pdf.worker.min.mjs`;

interface PdfViewerProps {
  url: string;
}

export const PdfViewer: React.FC<PdfViewerProps> = ({ url }) => {
  // PDF-related state
  const [numPages, setNumPages] = useState<number | null>(null);
  const [pdfError, setPdfError] = useState<string | null>(null);
  const [currentPage, setCurrentPage] = useState<number>(1);
  const [pageInputValue, setPageInputValue] = useState<string>('1');
  
  // Virtual scrolling state
  const [visiblePages, setVisiblePages] = useState<Set<number>>(new Set([1]));
  const [pageHeights, setPageHeights] = useState<{ [key: number]: number }>({});
  
  // Zoom and pan state
  const [zoom, setZoom] = useState<number>(0.8);
  const [zoomInputValue, setZoomInputValue] = useState<string>('80');
  const [isZoomInitialized, setIsZoomInitialized] = useState<boolean>(false);
  const [panOffset, setPanOffset] = useState<{ x: number; y: number }>({ x: 0, y: 0 });
  const [isDragging, setIsDragging] = useState<boolean>(false);
  const [dragStart, setDragStart] = useState<{ x: number; y: number }>({ x: 0, y: 0 });
  const [dragStartOffset, setDragStartOffset] = useState<{ x: number; y: number }>({ x: 0, y: 0 });
  
  // Zoom constants
  const MIN_ZOOM = 0.3;
  const MAX_ZOOM = 3.0;
  const ZOOM_STEP = 0.2;
  
  // Refs for scrolling to pages and pan/zoom
  const pdfContainerRef = useRef<HTMLDivElement>(null);
  const pdfContentRef = useRef<HTMLDivElement>(null);
  const pageRefs = useRef<{ [key: number]: HTMLDivElement | null }>({});

  // Reset PDF state when URL changes
  useEffect(() => {
    setNumPages(null);
    setPdfError(null);
    setCurrentPage(1);
    setPageInputValue('1');
    setZoom(0.8);
    setZoomInputValue('80');
    setPanOffset({ x: 0, y: 0 });
    setIsZoomInitialized(false);
    setVisiblePages(new Set([1]));
    setPageHeights({});
  }, [url]);

  // Zoom functions
  const handleZoomIn = useCallback(() => {
    const newZoom = Math.min(zoom + ZOOM_STEP, MAX_ZOOM);
    setZoom(newZoom);
    setZoomInputValue(Math.round(newZoom * 100).toString());
  }, [zoom]);

  const handleZoomOut = useCallback(() => {
    const newZoom = Math.max(zoom - ZOOM_STEP, MIN_ZOOM);
    setZoom(newZoom);
    setZoomInputValue(Math.round(newZoom * 100).toString());
  }, [zoom]);

  const calculateFitToWidthZoom = useCallback(async (pdf: any) => {
    if (!pdfContainerRef.current) return 0.8;
    
    const containerWidth = pdfContainerRef.current.clientWidth;
    // Account for padding and potential scrollbar
    const availableWidth = containerWidth - 32; // 16px padding on each side
    
    try {
      const page = await pdf.getPage(1);
      const viewport = page.getViewport({ scale: 1.0 });
      const pageWidth = viewport.width;
      
      // Calculate zoom to fit width, but cap it between MIN_ZOOM and 1.0 for better UX
      const calculatedZoom = Math.min(1.0, Math.max(MIN_ZOOM, availableWidth / pageWidth));
      return calculatedZoom;
    } catch (error) {
      console.warn('Failed to calculate fit-to-width zoom:', error);
      return 0.8;
    }
  }, []);

  const handleZoomReset = useCallback(async () => {
    // Reset pan first
    setPanOffset({ x: 0, y: 0 });
    
    // Try to recalculate fit-to-width zoom if we have a PDF loaded
    if (numPages && pdfContainerRef.current) {
      try {
        // We need to get the PDF object to recalculate zoom
        // For now, reset to a conservative zoom that should fit most cases
        const resetZoom = 0.6;
        setZoom(resetZoom);
        setZoomInputValue(Math.round(resetZoom * 100).toString());
      } catch (error) {
        // Fallback to 0.6 if calculation fails
        setZoom(0.6);
        setZoomInputValue('60');
      }
    } else {
      setZoom(0.6);
      setZoomInputValue('60');
    }
  }, [numPages]);

  const handleZoomInputChange = (value: string) => {
    setZoomInputValue(value);
  };

  const handleZoomInputSubmit = () => {
    const zoomPercent = parseInt(zoomInputValue, 10);
    if (!isNaN(zoomPercent)) {
      const clampedPercent = Math.max(MIN_ZOOM * 100, Math.min(MAX_ZOOM * 100, zoomPercent));
      const newZoom = clampedPercent / 100;
      setZoom(newZoom);
      setZoomInputValue(clampedPercent.toString());
    } else {
      // Reset to current zoom if invalid
      setZoomInputValue(Math.round(zoom * 100).toString());
    }
  };

  const handleZoomInputKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter') {
      handleZoomInputSubmit();
    }
  };

  // Pan functions
  const handleMouseDown = useCallback((e: React.MouseEvent) => {
    if (zoom <= 1) return; // Only allow panning when zoomed in
    
    setIsDragging(true);
    setDragStart({ x: e.clientX, y: e.clientY });
    setDragStartOffset(panOffset);
    e.preventDefault();
  }, [zoom, panOffset]);

  const handleMouseMove = useCallback((e: React.MouseEvent) => {
    if (!isDragging || zoom <= 1) return;

    const deltaX = e.clientX - dragStart.x;
    const deltaY = e.clientY - dragStart.y;
    
    setPanOffset({
      x: dragStartOffset.x + deltaX,
      y: dragStartOffset.y + deltaY
    });
  }, [isDragging, dragStart, dragStartOffset, zoom]);

  const handleMouseUp = useCallback(() => {
    setIsDragging(false);
  }, []);

  // Touch event handlers for mobile
  const handleTouchStart = useCallback((e: React.TouchEvent) => {
    if (zoom <= 1 || e.touches.length !== 1) return;
    
    const touch = e.touches[0];
    if (!touch) return;
    
    setIsDragging(true);
    setDragStart({ x: touch.clientX, y: touch.clientY });
    setDragStartOffset(panOffset);
    e.preventDefault();
  }, [zoom, panOffset]);

  const handleTouchMove = useCallback((e: React.TouchEvent) => {
    if (!isDragging || zoom <= 1 || e.touches.length !== 1) return;

    const touch = e.touches[0];
    if (!touch) return;
    
    const deltaX = touch.clientX - dragStart.x;
    const deltaY = touch.clientY - dragStart.y;
    
    setPanOffset({
      x: dragStartOffset.x + deltaX,
      y: dragStartOffset.y + deltaY
    });
    e.preventDefault();
  }, [isDragging, dragStart, dragStartOffset, zoom]);

  const handleTouchEnd = useCallback(() => {
    setIsDragging(false);
  }, []);

  // Wheel zoom
  const handleWheel = useCallback((e: React.WheelEvent) => {
    if (e.ctrlKey || e.metaKey) {
      e.preventDefault();
      const delta = -e.deltaY * 0.01;
      const newZoom = Math.max(MIN_ZOOM, Math.min(MAX_ZOOM, zoom + delta));
      setZoom(newZoom);
      setZoomInputValue(Math.round(newZoom * 100).toString());
    }
  }, [zoom]);

  // Calculate which pages should be visible based on viewport
  const calculateVisiblePages = useCallback(() => {
    if (!pdfContainerRef.current || !numPages) return;

    const container = pdfContainerRef.current;
    const containerTop = container.scrollTop;
    const containerBottom = containerTop + container.clientHeight;
    
    // Add buffer for smoother scrolling (render 2 pages before/after visible area)
    const bufferSize = 2;
    const newVisiblePages = new Set<number>();
    
    // Estimate page height if we don't have exact measurements
    const estimatedPageHeight = 800 * zoom; // Rough estimate
    const padding = 16; // 8px top + 8px bottom from container padding
    
    for (let pageNum = 1; pageNum <= numPages; pageNum++) {
      const pageElement = pageRefs.current[pageNum];
      let pageTop: number;
      let pageBottom: number;
      
      if (pageElement) {
        // Use actual measurements if available
        const rect = pageElement.getBoundingClientRect();
        const containerRect = container.getBoundingClientRect();
        pageTop = container.scrollTop + (rect.top - containerRect.top);
        pageBottom = pageTop + rect.height;
      } else {
        // Estimate position based on page number and estimated height
        const spacing = 16; // Gap between pages
        pageTop = padding + (pageNum - 1) * (estimatedPageHeight + spacing);
        pageBottom = pageTop + estimatedPageHeight;
      }
      
      // Check if page is within buffered viewport
      const isInBuffer = pageBottom >= containerTop - (bufferSize * estimatedPageHeight) &&
                        pageTop <= containerBottom + (bufferSize * estimatedPageHeight);
      
      if (isInBuffer) {
        newVisiblePages.add(pageNum);
      }
    }
    
    // Always include at least the current page
    newVisiblePages.add(currentPage);
    
    setVisiblePages(newVisiblePages);
  }, [numPages, zoom, currentPage]);

  // Intersection observer to track visible pages and current page
  useEffect(() => {
    if (!numPages) return;

    const observer = new IntersectionObserver(
      (entries) => {
        // Find the page that's most visible
        let mostVisiblePage = currentPage;
        let maxIntersectionRatio = 0;

        entries.forEach((entry) => {
          if (entry.isIntersecting && entry.intersectionRatio > maxIntersectionRatio) {
            const pageNumber = parseInt(entry.target.getAttribute('data-page-number') || '1', 10);
            maxIntersectionRatio = entry.intersectionRatio;
            mostVisiblePage = pageNumber;
          }
        });

        if (mostVisiblePage !== currentPage) {
          setCurrentPage(mostVisiblePage);
          setPageInputValue(mostVisiblePage.toString());
        }
        
        // Recalculate visible pages
        calculateVisiblePages();
      },
      {
        root: pdfContainerRef.current,
        rootMargin: '-10% 0px -10% 0px', // Only trigger when page is well into view
        threshold: [0.1, 0.5, 0.9], // Multiple thresholds for better detection
      }
    );

    // Observe all rendered page elements
    Object.values(pageRefs.current).forEach((pageElement) => {
      if (pageElement) {
        observer.observe(pageElement);
      }
    });

    return () => {
      observer.disconnect();
    };
  }, [numPages, currentPage, calculateVisiblePages]);

  // Recalculate visible pages on scroll
  useEffect(() => {
    if (!pdfContainerRef.current) return;
    
    const container = pdfContainerRef.current;
    const handleScroll = () => {
      calculateVisiblePages();
    };
    
    container.addEventListener('scroll', handleScroll);
    return () => container.removeEventListener('scroll', handleScroll);
  }, [calculateVisiblePages]);

  const handlePageChange = (newPage: number) => {
    if (numPages && newPage >= 1 && newPage <= numPages) {
      setCurrentPage(newPage);
      setPageInputValue(newPage.toString());
      
      // Scroll to the specific page within the PDF container only
      const pageElement = pageRefs.current[newPage];
      const container = pdfContainerRef.current;
      if (pageElement && container) {
        const containerRect = container.getBoundingClientRect();
        const pageRect = pageElement.getBoundingClientRect();
        const scrollTop = container.scrollTop + (pageRect.top - containerRect.top) - (containerRect.height / 2) + (pageRect.height / 2);
        
        container.scrollTop = scrollTop;
      }
    }
  };

  const handlePageInputChange = (value: string) => {
    setPageInputValue(value);
  };

  const handlePageInputSubmit = () => {
    const pageNum = parseInt(pageInputValue, 10);
    if (numPages && pageNum >= 1 && pageNum <= numPages) {
      setCurrentPage(pageNum);
      
      // Scroll to the specific page within the PDF container only
      const pageElement = pageRefs.current[pageNum];
      const container = pdfContainerRef.current;
      if (pageElement && container) {
        const containerRect = container.getBoundingClientRect();
        const pageRect = pageElement.getBoundingClientRect();
        const scrollTop = container.scrollTop + (pageRect.top - containerRect.top) - (containerRect.height / 2) + (pageRect.height / 2);
        
        container.scrollTop = scrollTop;
      }
    } else {
      // Reset to current page if invalid
      setPageInputValue(currentPage.toString());
    }
  };

  const handlePageInputKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter') {
      handlePageInputSubmit();
    }
  };

  if (!url) {
    return (
      <div className="flex h-[30vh] items-center justify-center rounded bg-[var(--vscode-editor-background)] border-2 border-dashed border-[var(--vscode-panel-border)]">
        <div className="text-center space-y-2">
          <FileText className="w-8 h-8 mx-auto text-[var(--vscode-description-foreground)]" />
          <p className="text-sm text-[var(--vscode-description-foreground)]">No PDF URL available</p>
        </div>
      </div>
    );
  }

  // Currently using the local pdf viewer for everything.
  if (true) {
    return (
      <div className="h-[70vh] relative bg-[var(--vscode-editor-background)] border border-[var(--vscode-panel-border)] rounded overflow-hidden">
        {pdfError ? (
          <div className="flex items-center justify-center h-full text-[var(--vscode-charts-red)]">
            <div className="text-center space-y-3 max-w-md px-4">
              <FileText className="w-12 h-12 mx-auto" />
              <div className="space-y-2">
                <p className="text-sm font-medium">PDF Loading Error</p>
                <p className="text-xs text-[var(--vscode-description-foreground)] leading-relaxed">
                  {pdfError}
                </p>
                {!url.startsWith('blob:') && !url.startsWith('data:') && (
                  <p className="text-xs text-[var(--vscode-description-foreground)] leading-relaxed">
                    Note that certain URLs may not be accessible to BAML.
                  </p>
                )}
              </div>
            </div>
          </div>
        ) : (
          <>
            {/* PDF Content */}
            <div 
              ref={pdfContainerRef} 
              className="h-full overflow-auto"
              onWheel={handleWheel}
              style={{ cursor: zoom > 1 ? (isDragging ? 'grabbing' : 'grab') : 'default' }}
            >
              <div
                ref={pdfContentRef}
                style={{
                  transform: `translate(${panOffset.x}px, ${panOffset.y}px)`,
                  transition: isDragging ? 'none' : 'transform 0.2s ease-out',
                  display: 'flex',
                  flexDirection: 'column',
                  alignItems: 'center',
                  minWidth: 'fit-content',
                  minHeight: 'fit-content',
                  padding: '8px',
                }}
                onMouseDown={handleMouseDown}
                onMouseMove={handleMouseMove}
                onMouseUp={handleMouseUp}
                onMouseLeave={handleMouseUp}
                onTouchStart={handleTouchStart}
                onTouchMove={handleTouchMove}
                onTouchEnd={handleTouchEnd}
              >
                <Document
                  file={url}
                  onLoadSuccess={async (pdf: any) => {
                    setNumPages(pdf.numPages);
                    setPdfError(null);
                    // Clear existing refs
                    pageRefs.current = {};
                    
                    // Calculate and set appropriate zoom to fit width
                    if (!isZoomInitialized) {
                      const fitZoom = await calculateFitToWidthZoom(pdf);
                      setZoom(fitZoom);
                      setZoomInputValue(Math.round(fitZoom * 100).toString());
                      setIsZoomInitialized(true);
                    }
                  }}
                  onLoadError={(error: any) => {
                    const errorMessage = url.startsWith('blob:') || url.startsWith('data:') 
                      ? 'Failed to load PDF file'
                      : `Cannot download PDF from the given URL: ${url}`;
                    setPdfError(errorMessage);
                  }}
                  loading={
                    <div className="flex items-center justify-center h-full min-h-[200px]">
                      <div className="text-center space-y-2">
                        <div className="w-6 h-6 border-2 border-[var(--vscode-panel-border)] border-t-[var(--vscode-foreground)] rounded-full animate-spin mx-auto"></div>
                        <p className="text-sm text-[var(--vscode-description-foreground)]">Loading PDF...</p>
                      </div>
                    </div>
                  }
                  className="space-y-4"
                >
                  {numPages && Array.from({ length: numPages }, (_, index) => {
                    const pageNumber = index + 1;
                    const isVisible = visiblePages.has(pageNumber);
                    
                    // Estimate page height for non-rendered pages
                    const estimatedPageHeight = 800 * zoom;
                    const spacing = 16;
                    
                    return (
                      <div
                        key={pageNumber}
                        ref={(el) => {
                          pageRefs.current[pageNumber] = el;
                        }}
                        data-page-number={pageNumber}
                        className="relative shadow-sm rounded overflow-hidden bg-white"
                        style={{ 
                          flexShrink: 0,
                          // For non-visible pages, maintain height to preserve scroll position
                          minHeight: isVisible ? 'auto' : `${estimatedPageHeight}px`
                        }}
                      >
                        {isVisible ? (
                          <>
                            <Page
                              pageNumber={pageNumber}
                              scale={zoom}
                              renderTextLayer={true}
                              renderAnnotationLayer={true}
                              className="border border-[var(--vscode-panel-border)]"
                              onLoadSuccess={(page: any) => {
                                // Store actual page height for better estimates
                                setPageHeights(prev => ({
                                  ...prev,
                                  [pageNumber]: page.view[3] * zoom
                                }));
                              }}
                            />
                            <div className="absolute top-1 right-1 bg-[var(--vscode-editor-background)] text-[var(--vscode-foreground)] text-xs px-1.5 py-0.5 rounded border border-[var(--vscode-panel-border)]">
                              {pageNumber}
                            </div>
                          </>
                        ) : (
                          // Placeholder for non-visible pages
                          <div 
                            className="flex items-center justify-center border border-[var(--vscode-panel-border)] bg-gray-100"
                            style={{ height: `${estimatedPageHeight}px` }}
                          >
                            <div className="text-gray-500 text-sm">
                              Page {pageNumber}
                            </div>
                          </div>
                        )}
                      </div>
                    );
                  })}
                </Document>
              </div>
            </div>
            
            {/* Navigation and Zoom Controls Overlay */}
            <div className="absolute bottom-4 left-1/2 transform -translate-x-1/2 z-10 flex items-center gap-2 px-2 py-1 bg-[var(--vscode-editor-background)]/95 backdrop-blur-sm border border-[var(--vscode-panel-border)] rounded-lg shadow-lg pointer-events-auto">
              {/* Page Navigation */}
              {numPages && numPages > 1 && (
                <>
                  <button
                    onClick={() => handlePageChange(currentPage - 1)}
                    disabled={currentPage <= 1}
                    className={`p-1 rounded transition-colors ${
                      currentPage <= 1
                        ? 'text-[var(--vscode-description-foreground)] cursor-not-allowed'
                        : 'text-[var(--vscode-foreground)] hover:bg-[var(--vscode-button-hover-background)]'
                    }`}
                    title="Previous page"
                  >
                    <ChevronLeft className="w-3.5 h-3.5" />
                  </button>
                  
                  <div className="flex items-center gap-1">
                    <span className="text-xs text-[var(--vscode-foreground)] leading-none">Page</span>
                    <input
                      type="text"
                      value={pageInputValue}
                      onChange={(e) => handlePageInputChange(e.target.value)}
                      onKeyDown={handlePageInputKeyDown}
                      onBlur={handlePageInputSubmit}
                      className="w-10 h-5 px-1 text-xs text-center bg-[var(--vscode-input-background)] border border-[var(--vscode-panel-border)] rounded focus:outline-none focus:border-[var(--vscode-focus-border)] leading-none"
                    />
                    <span className="text-xs text-[var(--vscode-description-foreground)] leading-none whitespace-nowrap">/ {numPages}</span>
                  </div>
                  
                  <button
                    onClick={() => handlePageChange(currentPage + 1)}
                    disabled={currentPage >= numPages}
                    className={`p-1 rounded transition-colors ${
                      currentPage >= numPages
                        ? 'text-[var(--vscode-description-foreground)] cursor-not-allowed'
                        : 'text-[var(--vscode-foreground)] hover:bg-[var(--vscode-button-hover-background)]'
                    }`}
                    title="Next page"
                  >
                    <ChevronRight className="w-3.5 h-3.5" />
                  </button>
                  
                  <div className="w-px h-4 bg-[var(--vscode-panel-border)] mx-1"></div>
                </>
              )}
              
              {/* Zoom Controls */}
              <button
                onClick={handleZoomOut}
                disabled={zoom <= MIN_ZOOM}
                className={`p-1 rounded transition-colors ${
                  zoom <= MIN_ZOOM
                    ? 'text-[var(--vscode-description-foreground)] cursor-not-allowed'
                    : 'text-[var(--vscode-foreground)] hover:bg-[var(--vscode-button-hover-background)]'
                }`}
                title="Zoom out"
              >
                <ZoomOut className="w-3.5 h-3.5" />
              </button>
              
              <div className="flex items-center">
                <input
                  type="text"
                  value={zoomInputValue}
                  onChange={(e) => handleZoomInputChange(e.target.value)}
                  onKeyDown={handleZoomInputKeyDown}
                  onBlur={handleZoomInputSubmit}
                  className="w-8 h-5 px-1 text-xs text-center bg-[var(--vscode-input-background)] border border-[var(--vscode-panel-border)] rounded focus:outline-none focus:border-[var(--vscode-focus-border)] leading-none"
                />
                <span className="text-xs text-[var(--vscode-description-foreground)] pl-1">%</span>
              </div>
              
              <button
                onClick={handleZoomIn}
                disabled={zoom >= MAX_ZOOM}
                className={`p-1 rounded transition-colors ${
                  zoom >= MAX_ZOOM
                    ? 'text-[var(--vscode-description-foreground)] cursor-not-allowed'
                    : 'text-[var(--vscode-foreground)] hover:bg-[var(--vscode-button-hover-background)]'
                }`}
                title="Zoom in"
              >
                <ZoomIn className="w-3.5 h-3.5" />
              </button>
              
              <button
                onClick={handleZoomReset}
                className="p-1 rounded transition-colors text-[var(--vscode-foreground)] hover:bg-[var(--vscode-button-hover-background)]"
                title="Reset zoom and pan"
              >
                <RotateCcw className="w-3.5 h-3.5" />
              </button>
            </div>
            
            {/* Pan Instruction for Zoomed View */}
            {zoom > 1 && (
              <div className="absolute top-4 right-4 z-10 px-2 py-1 bg-[var(--vscode-editor-background)]/95 backdrop-blur-sm border border-[var(--vscode-panel-border)] rounded text-xs text-[var(--vscode-description-foreground)]">
                Drag to pan • Ctrl+wheel to zoom
              </div>
            )}
          </>
        )}
      </div>
    );
  }

  // For HTTP URLs, use PDF.js viewer
  const pdfViewerUrl = `https://mozilla.github.io/pdf.js/web/viewer.html?file=${encodeURIComponent(url)}`;

  return (
    <div className="w-full max-w-4xl mx-auto">
      <div className="h-[70vh] border border-[var(--vscode-panel-border)] rounded overflow-hidden bg-[var(--vscode-editor-background)]">
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
    </div>
  );
}; 