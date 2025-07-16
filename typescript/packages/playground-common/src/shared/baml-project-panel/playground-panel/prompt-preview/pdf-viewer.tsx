import { ChevronLeft, ChevronRight, FileText } from 'lucide-react';
import { useState, useEffect, useRef } from 'react';
// @ts-ignore - react-pdf types are handled at runtime
import { Document, Page, pdfjs } from 'react-pdf';
import 'react-pdf/dist/esm/Page/AnnotationLayer.css';
import 'react-pdf/dist/esm/Page/TextLayer.css';

// Configure PDF.js worker (recommended approach)
pdfjs.GlobalWorkerOptions.workerSrc = new URL(
  'pdfjs-dist/build/pdf.worker.min.mjs',
  import.meta.url,
).toString();

interface PdfViewerProps {
  url: string;
}

export const PdfViewer: React.FC<PdfViewerProps> = ({ url }) => {
  // PDF-related state
  const [numPages, setNumPages] = useState<number | null>(null);
  const [pdfError, setPdfError] = useState<string | null>(null);
  const [currentPage, setCurrentPage] = useState<number>(1);
  const [pageInputValue, setPageInputValue] = useState<string>('1');
  
  // Refs for scrolling to pages
  const pdfContainerRef = useRef<HTMLDivElement>(null);
  const pageRefs = useRef<{ [key: number]: HTMLDivElement | null }>({});

  // Reset PDF state when URL changes
  useEffect(() => {
    setNumPages(null);
    setPdfError(null);
    setCurrentPage(1);
    setPageInputValue('1');
  }, [url]);

  // Intersection observer to track visible pages
  useEffect(() => {
    if (!numPages || numPages <= 1) return;

    const observer = new IntersectionObserver(
      (entries) => {
        // Find the page that's most visible
        let mostVisiblePage = 1;
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
      },
      {
        root: pdfContainerRef.current,
        rootMargin: '-10% 0px -10% 0px', // Only trigger when page is well into view
        threshold: [0.1, 0.5, 0.9], // Multiple thresholds for better detection
      }
    );

    // Observe all page elements
    Object.values(pageRefs.current).forEach((pageElement) => {
      if (pageElement) {
        observer.observe(pageElement);
      }
    });

    return () => {
      observer.disconnect();
    };
  }, [numPages, currentPage]);

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

  // For blob URLs or data URLs, use react-pdf for local rendering
  if (url.startsWith('blob:') || url.startsWith('data:')) {
    return (
      <div className="h-[70vh] relative bg-[var(--vscode-editor-background)] border border-[var(--vscode-panel-border)] rounded overflow-hidden">
        {pdfError ? (
          <div className="flex items-center justify-center h-full text-[var(--vscode-charts-red)]">
            <div className="text-center space-y-2">
              <FileText className="w-8 h-8 mx-auto" />
              <p className="text-sm">Error loading PDF: {pdfError}</p>
            </div>
          </div>
        ) : (
          <>
            {/* PDF Content */}
            <div ref={pdfContainerRef} className="h-full overflow-auto">
              <Document
                file={url}
                onLoadSuccess={(pdf: any) => {
                  setNumPages(pdf.numPages);
                  setPdfError(null);
                  // Clear existing refs
                  pageRefs.current = {};
                }}
                onLoadError={(error: any) => {
                  setPdfError(error.message || 'Failed to load PDF');
                }}
                loading={
                  <div className="flex items-center justify-center h-full min-h-[200px]">
                    <div className="text-center space-y-2">
                      <div className="w-6 h-6 border-2 border-[var(--vscode-panel-border)] border-t-[var(--vscode-foreground)] rounded-full animate-spin mx-auto"></div>
                      <p className="text-sm text-[var(--vscode-description-foreground)]">Loading PDF...</p>
                    </div>
                  </div>
                }
                className="flex flex-col items-center space-y-4 p-2"
              >
                {numPages && Array.from({ length: numPages }, (_, index) => (
                  <div
                    key={index + 1}
                    ref={(el) => {
                      pageRefs.current[index + 1] = el;
                    }}
                    data-page-number={index + 1}
                    className="relative shadow-sm rounded overflow-hidden bg-white max-w-full"
                  >
                    <Page
                      pageNumber={index + 1}
                      scale={0.8}
                      renderTextLayer={true}
                      renderAnnotationLayer={true}
                      className="border border-[var(--vscode-panel-border)] max-w-full"
                    />
                    <div className="absolute top-1 right-1 bg-[var(--vscode-editor-background)] text-[var(--vscode-foreground)] text-xs px-1.5 py-0.5 rounded border border-[var(--vscode-panel-border)]">
                      {index + 1}
                    </div>
                  </div>
                ))}
              </Document>
            </div>
            
            {/* Navigation Controls Overlay */}
            {numPages && numPages > 1 && (
              <div className="absolute bottom-4 left-1/2 transform -translate-x-1/2 z-10 flex items-center gap-1 px-3 py-1.5 bg-[var(--vscode-editor-background)]/95 backdrop-blur-sm border border-[var(--vscode-panel-border)] rounded-lg shadow-lg pointer-events-auto">
                <button
                  onClick={() => handlePageChange(currentPage - 1)}
                  disabled={currentPage <= 1}
                  className={`p-1.5 rounded transition-colors ${
                    currentPage <= 1
                      ? 'text-[var(--vscode-description-foreground)] cursor-not-allowed'
                      : 'text-[var(--vscode-foreground)] hover:bg-[var(--vscode-button-hover-background)]'
                  }`}
                  title="Previous page"
                >
                  <ChevronLeft className="w-3.5 h-3.5" />
                </button>
                
                <div className="flex items-center gap-1.5">
                  <span className="text-xs text-[var(--vscode-foreground)]">Page</span>
                  <input
                    type="text"
                    value={pageInputValue}
                    onChange={(e) => handlePageInputChange(e.target.value)}
                    onKeyDown={handlePageInputKeyDown}
                    onBlur={handlePageInputSubmit}
                    className="w-10 h-6 px-1.5 text-xs text-center bg-[var(--vscode-input-background)] border border-[var(--vscode-panel-border)] rounded focus:outline-none focus:border-[var(--vscode-focus-border)]"
                  />
                  <span className="text-xs text-[var(--vscode-description-foreground)]">of {numPages}</span>
                </div>
                
                <button
                  onClick={() => handlePageChange(currentPage + 1)}
                  disabled={currentPage >= numPages}
                  className={`p-1.5 rounded transition-colors ${
                    currentPage >= numPages
                      ? 'text-[var(--vscode-description-foreground)] cursor-not-allowed'
                      : 'text-[var(--vscode-foreground)] hover:bg-[var(--vscode-button-hover-background)]'
                  }`}
                  title="Next page"
                >
                  <ChevronRight className="w-3.5 h-3.5" />
                </button>
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