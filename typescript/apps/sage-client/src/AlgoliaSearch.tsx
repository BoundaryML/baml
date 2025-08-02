'use client';

import { liteClient as algoliasearch } from 'algoliasearch/lite';
import type React from 'react';
import { useEffect, useRef, useState } from 'react';
import {
  Configure,
  InstantSearch,
  useHits,
  useSearchBox,
} from 'react-instantsearch';
import { z } from 'zod';
import BamlLambWhite from './baml-lamb-white.svg';

const SEARCH_INDEX_NAME = 'fern_docs_search';
const API_ENDPOINT = 'https://docs.boundaryml.com/api/fern-docs/search/v2/key';

// Zod schema for API response validation
const SearchCredentialsSchema = z.object({
  appId: z.string(),
  apiKey: z.string(),
});

type SearchCredentials = z.infer<typeof SearchCredentialsSchema>;

// Function to fetch Algolia search credentials from API
async function fetchSearchCredentials(): Promise<SearchCredentials> {
  try {
    const response = await fetch(API_ENDPOINT);
    if (!response.ok) {
      throw new Error(`Failed to fetch search credentials: ${response.status}`);
    }
    const data = await response.json();

    // Validate the response with Zod
    const credentials = SearchCredentialsSchema.parse(data);
    return credentials;
  } catch (error) {
    if (error instanceof z.ZodError) {
      console.error('Invalid API response format:', error);
      throw new Error('Invalid search credentials response format');
    }
    console.error('Error fetching search credentials:', error);
    throw error;
  }
}

// Create search filters for the domain
const createSearchFilters = () => {
  return 'domain:docs.boundaryml.com AND NOT type:navigation';
};

// Modern document type icon component using SVG icons
function DocumentIcon({ type }: { type: string }) {
  const iconStyle = {
    width: '18px',
    height: '18px',
    flexShrink: 0,
    color: '#6366f1',
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'center',
  };

  const getIcon = () => {
    switch (type) {
      case 'guide':
        return (
          <svg fill="currentColor" viewBox="0 0 24 24" aria-label="Guide">
            <title>Guide Document</title>
            <path d="M19 3H5c-1.1 0-2 .9-2 2v14c0 1.1.9 2 2 2h14c1.1 0 2-.9 2-2V5c0-1.1-.9-2-2-2zm-5 14H7v-2h7v2zm3-4H7v-2h10v2zm0-4H7V7h10v2z" />
          </svg>
        );
      case 'reference':
        return (
          <svg fill="currentColor" viewBox="0 0 24 24" aria-label="Reference">
            <title>Reference Document</title>
            <path d="M9.4 16.6L4.8 12l4.6-4.6L8 6l-6 6 6 6 1.4-1.4zm5.2 0L19.2 12l-4.6-4.6L16 6l6 6-6 6-1.4-1.4z" />
          </svg>
        );
      case 'example':
        return (
          <svg fill="currentColor" viewBox="0 0 24 24" aria-label="Example">
            <title>Example Document</title>
            <path d="M3 13h2v-2H3v2zm0 4h2v-2H3v2zm0-8h2V7H3v2zm4 4h14v-2H7v2zm0 4h14v-2H7v2zM7 7v2h14V7H7z" />
          </svg>
        );
      default:
        return (
          <svg fill="currentColor" viewBox="0 0 24 24" aria-label="Document">
            <title>Document</title>
            <path d="M14,2H6A2,2 0 0,0 4,4V20A2,2 0 0,0 6,22H18A2,2 0 0,0 20,20V8L14,2M18,20H6V4H13V9H18V20Z" />
          </svg>
        );
    }
  };

  return <div style={iconStyle}>{getIcon()}</div>;
}

// Function to process highlights for search results
const processHighlights = (text: string) => {
  return text
    .replace(
      /__ais-highlight__/g,
      '<mark style="background: #e9d5ff; color: #6b21a8; font-weight: 600; padding: 0 2px; border-radius: 2px;">',
    )
    .replace(/__\/ais-highlight__/g, '</mark>');
};

// Custom Hit component to display search results with icons and descriptions
function Hit({
  hit,
  isKeyboardSelected,
}: { hit: any; isKeyboardSelected?: boolean }) {
  const [showTooltip, setShowTooltip] = useState(false);

  const highlightedTitle =
    hit._highlightResult?.title?.value || hit.title || 'Untitled';
  const highlightedDescription =
    hit._highlightResult?.content?.value ||
    hit._snippetResult?.content?.value ||
    hit.content ||
    '';

  // Determine document type based on URL path
  const getDocumentType = (pathname: string): string => {
    if (pathname.includes('/guide/')) return 'guide';
    if (pathname.includes('/reference/')) return 'reference';
    if (pathname.includes('/examples/')) return 'example';
    return 'document';
  };

  const documentType = getDocumentType(
    hit.pathname || hit.canonicalPathname || '',
  );

  // Truncate description for display
  const truncateText = (text: string, maxLength: number) => {
    if (text.length <= maxLength) return text;
    return `${text.substring(0, maxLength)}...`;
  };

  const processedDescription = highlightedDescription.replace(/<[^>]*>/g, ''); // Remove HTML tags for length calculation
  const displayDescription = truncateText(processedDescription, 150);

  const handleMouseEnter = (e: React.MouseEvent) => {
    if (!isKeyboardSelected) {
      (e.currentTarget as HTMLElement).style.backgroundColor = '#f3e8ff';
    }
    setShowTooltip(true);
  };

  const handleMouseLeave = (e: React.MouseEvent) => {
    if (!isKeyboardSelected) {
      (e.currentTarget as HTMLElement).style.backgroundColor = 'transparent';
    }
    setShowTooltip(false);
  };

  return (
    <div style={{ position: 'relative' }}>
      <a
        href={hit.pathname || hit.canonicalPathname || '#'}
        style={{
          display: 'flex',
          alignItems: 'flex-start',
          gap: '10px',
          padding: '10px 14px',
          textDecoration: 'none',
          color: '#374151',
          borderBottom: '1px solid #f3f4f6',
          transition: 'background-color 0.2s ease',
          cursor: 'pointer',
          background: isKeyboardSelected ? '#ede9fe' : 'transparent',
        }}
        onMouseEnter={handleMouseEnter}
        onMouseLeave={handleMouseLeave}
      >
        {/* Document Icon */}
        <div style={{ paddingTop: '2px' }}>
          <DocumentIcon type={documentType} />
        </div>

        {/* Content */}
        <div style={{ flex: 1, minWidth: 0 }}>
          <div
            style={{
              fontWeight: 600,
              fontSize: '15px',
              marginBottom: '4px',
              color: '#111827',
              lineHeight: '1.3',
            }}
          >
            <span
              // eslint-disable-next-line react/no-danger
              dangerouslySetInnerHTML={{
                __html: processHighlights(highlightedTitle),
              }}
            />
          </div>

          {highlightedDescription && (
            <div
              style={{
                fontSize: '13px',
                color: '#6b7280',
                lineHeight: '1.5',
                wordBreak: 'break-word',
              }}
            >
              <span
                // eslint-disable-next-line react/no-danger
                dangerouslySetInnerHTML={{
                  __html: processHighlights(displayDescription),
                }}
              />
            </div>
          )}

          {hit.breadcrumb && hit.breadcrumb.length > 0 && (
            <div
              style={{
                fontSize: '11px',
                color: '#9ca3af',
                marginTop: '6px',
                display: 'flex',
                alignItems: 'center',
                gap: '4px',
              }}
            >
              <svg
                width="12"
                height="12"
                fill="currentColor"
                viewBox="0 0 20 20"
                aria-label="Breadcrumb"
              >
                <title>Breadcrumb Navigator</title>
                <path
                  fillRule="evenodd"
                  d="M7.293 14.707a1 1 0 010-1.414L10.586 10 7.293 6.707a1 1 0 011.414-1.414l4 4a1 1 0 010 1.414l-4 4a1 1 0 01-1.414 0z"
                  clipRule="evenodd"
                />
              </svg>
              <span>
                {hit.breadcrumb.map((crumb: any, index: number) => (
                  <span key={crumb.title}>
                    {index > 0 && ' › '}
                    {crumb.title}
                  </span>
                ))}
              </span>
            </div>
          )}
        </div>
      </a>

      {/* Simple Tooltip */}
      {showTooltip && highlightedDescription && (
        <div
          style={{
            position: 'absolute',
            top: '0',
            left: '100%',
            marginLeft: '12px',
            width: '280px',
            padding: '10px',
            backgroundColor: '#ffffff',
            border: '1px solid #e5e7eb',
            borderRadius: '8px',
            fontSize: '13px',
            lineHeight: '1.4',
            color: '#374151',
            zIndex: 1000,
            boxShadow: 'none',
            pointerEvents: 'none',
            maxHeight: '200px',
            overflowY: 'auto',
          }}
        >
          {/* Full description */}
          <div
            style={{
              marginBottom: hit.breadcrumb?.length > 0 ? '8px' : '0',
            }}
            // eslint-disable-next-line react/no-danger
            dangerouslySetInnerHTML={{
              __html: processHighlights(highlightedDescription),
            }}
          />

          {/* Breadcrumb */}
          {hit.breadcrumb && hit.breadcrumb.length > 0 && (
            <div
              style={{
                fontSize: '11px',
                color: '#9ca3af',
                paddingTop: '8px',
                borderTop: '1px solid #f3f4f6',
              }}
            >
              {hit.breadcrumb.map((crumb: any, index: number) => (
                <span key={crumb.title}>
                  {index > 0 && ' › '}
                  {crumb.title}
                </span>
              ))}
            </div>
          )}
        </div>
      )}
    </div>
  );
}

// Search Icon Component
function SearchIcon() {
  return (
    <svg
      width="16"
      height="16"
      fill="none"
      stroke="currentColor"
      viewBox="0 0 24 24"
      aria-label="Search"
    >
      <title>Search</title>
      <path
        strokeLinecap="round"
        strokeLinejoin="round"
        strokeWidth={2}
        d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z"
      />
    </svg>
  );
}

// Close X Icon Component
function CloseIcon() {
  return (
    <svg
      width="16"
      height="16"
      fill="none"
      stroke="currentColor"
      viewBox="0 0 24 24"
      aria-label="Close"
    >
      <title>Close</title>
      <path
        strokeLinecap="round"
        strokeLinejoin="round"
        strokeWidth={2}
        d="M6 18L18 6M6 6l12 12"
      />
    </svg>
  );
}

// Slash Icon Component
function SlashIcon() {
  return (
    <div
      style={{
        padding: '2px 6px',
        background: '#f3f4f6',
        borderRadius: '4px',
        fontSize: '11px',
        fontWeight: 600,
        color: '#6b7280',
        fontFamily: 'monospace',
      }}
    >
      /
    </div>
  );
}

// AI Icon Component
function AIIcon() {
  return (
    <div
      style={{
        width: '20px',
        height: '20px',
        borderRadius: '3px',
        background: '#7d47e3',
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'center',
        padding: '1px',
      }}
    >
      <img
        src={BamlLambWhite}
        alt="BAML Logo"
        style={{
          width: '17px',
          height: '17px',
          filter: 'none',
        }}
      />
    </div>
  );
}

// Ask with AI component
function AskWithAIOption({
  isSelected,
  onClick,
  query,
}: {
  isSelected: boolean;
  onClick: () => void;
  query: string;
}) {
  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter' || e.key === ' ') {
      e.preventDefault();
      e.stopPropagation();
      onClick();
    }
  };

  const handleClick = (e: React.MouseEvent) => {
    e.preventDefault();
    e.stopPropagation();
    onClick();
  };

  return (
    <button
      type="button"
      onClick={handleClick}
      onKeyDown={handleKeyDown}
      onMouseEnter={(e) => {
        if (!isSelected) {
          e.currentTarget.style.backgroundColor = '#f3e8ff';
        }
      }}
      onMouseLeave={(e) => {
        if (!isSelected) {
          e.currentTarget.style.backgroundColor = 'transparent';
        }
      }}
      style={{
        display: 'block',
        width: '100%',
        padding: '10px 14px',
        textDecoration: 'none',
        color: '#374151',
        borderBottom: '1px solid #f3f4f6',
        border: 'none',
        textAlign: 'left',
        transition: 'background-color 0.2s ease',
        cursor: 'pointer',
        background: isSelected ? '#ede9fe' : 'transparent',
        borderRadius: '0px',
        margin: '0',
      }}
    >
      <div
        style={{
          display: 'flex',
          alignItems: 'center',
          gap: '10px',
          fontWeight: 600,
          fontSize: '15px',
          color: '#6366f1',
          marginBottom: '2px',
        }}
      >
        <AIIcon />
        Ask Baaaaml about "{query}"
      </div>
    </button>
  );
}

// Custom SearchBox with integrated controls
function CustomSearchBox({
  onAskAI,
  onToggleAI,
  isAIOpen,
}: {
  onAskAI: (query: string) => void;
  onToggleAI?: () => void;
  isAIOpen?: boolean;
}) {
  const { query, refine } = useSearchBox();
  const { hits } = useHits();
  const [inputValue, setInputValue] = useState(query);
  const [isFocused, setIsFocused] = useState(false);
  const [selectedIndex, setSelectedIndex] = useState(-1);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    setInputValue(query);
  }, [query]);

  const handleInputChange = (event: React.ChangeEvent<HTMLInputElement>) => {
    const value = event.target.value;
    setInputValue(value);
    refine(value);
    setSelectedIndex(-1); // Reset selection when typing
  };

  const handleFocus = () => {
    setIsFocused(true);
  };

  const handleBlur = (e: React.FocusEvent) => {
    // Only blur if focus is moving outside the search container
    const currentTarget = e.currentTarget;
    setTimeout(() => {
      if (!currentTarget.contains(document.activeElement)) {
        setIsFocused(false);
        setSelectedIndex(-1);
      }
    }, 200); // Increased delay for better UX
  };

  const handleClear = () => {
    setInputValue('');
    refine('');
    setSelectedIndex(-1);
    inputRef.current?.focus();
  };

  const handleAskAI = () => {
    onAskAI(inputValue);
    setIsFocused(false);
  };

  const handleToggleAI = () => {
    if (onToggleAI) {
      onToggleAI();
    }
  };

  // Calculate total selectable items: Ask Baaaaml option (when query exists) + search results
  const totalSelectableItems = (inputValue.trim() ? 1 : 0) + hits.length;

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (!inputValue.trim() && e.key !== 'Escape') return;

    if (e.key === 'ArrowDown') {
      e.preventDefault();
      setSelectedIndex((prev) =>
        prev < totalSelectableItems - 1 ? prev + 1 : -1,
      );
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      setSelectedIndex((prev) =>
        prev > -1 ? prev - 1 : totalSelectableItems - 1,
      );
    } else if (e.key === 'Enter') {
      e.preventDefault();
      if (selectedIndex === 0 && inputValue.trim()) {
        // Ask Baaaaml option is selected
        handleAskAI();
      } else if (selectedIndex > 0) {
        // A search result is selected
        const hitIndex = selectedIndex - 1;
        if (hits[hitIndex]) {
          const hit = hits[hitIndex];
          window.location.href = hit.pathname || hit.canonicalPathname || '#';
        }
      } else if (selectedIndex === -1 && inputValue.trim()) {
        // No selection, trigger Ask Baaaaml by default
        handleAskAI();
      }
    } else if (e.key === 'Escape') {
      setIsFocused(false);
      setSelectedIndex(-1);
      inputRef.current?.blur();
    }
  };

  // Handle slash key shortcut
  useEffect(() => {
    const handleGlobalKeyDown = (e: KeyboardEvent) => {
      if (
        e.key === '/' &&
        !isFocused &&
        document.activeElement?.tagName !== 'INPUT' &&
        document.activeElement?.tagName !== 'TEXTAREA'
      ) {
        e.preventDefault();
        inputRef.current?.focus();
      }
    };

    document.addEventListener('keydown', handleGlobalKeyDown);
    return () => document.removeEventListener('keydown', handleGlobalKeyDown);
  }, [isFocused]);

  return (
    <div style={{ position: 'relative', width: '100%' }}>
      <div
        style={{
          position: 'relative',
          display: 'flex',
          alignItems: 'center',
          background: '#ffffff',
          border: `1px solid ${isFocused ? '#6366f1' : '#d1d5db'}`,
          borderRadius: '8px',
          transition: 'all 0.15s ease',
          boxShadow: isFocused
            ? '0 0 0 3px rgba(99, 102, 241, 0.08), 0 1px 3px rgba(0, 0, 0, 0.1)'
            : '0 1px 2px rgba(0, 0, 0, 0.05)',
          height: '36px',
        }}
      >
        {/* Search Icon */}
        <div
          style={{
            position: 'absolute',
            left: '10px',
            color: isFocused ? '#6366f1' : '#9ca3af',
            display: 'flex',
            alignItems: 'center',
            transition: 'color 0.15s ease',
          }}
        >
          <SearchIcon />
        </div>

        {/* Input Field */}
        <input
          ref={inputRef}
          type="text"
          value={inputValue}
          onChange={handleInputChange}
          onFocus={handleFocus}
          onBlur={handleBlur}
          onKeyDown={handleKeyDown}
          placeholder="Search BAML docs…"
          style={{
            flex: 1,
            padding: '0 140px 0 36px',
            border: 'none',
            outline: 'none',
            background: 'transparent',
            fontSize: '14px',
            color: '#111827',
            fontFamily: 'inherit',
            height: '100%',
            lineHeight: 1,
          }}
        />

        {/* Right side controls */}
        <div
          style={{
            position: 'absolute',
            right: '6px',
            display: 'flex',
            alignItems: 'center',
            gap: '6px',
            height: '100%',
          }}
        >
          {/* Clear button */}
          {inputValue && (
            <button
              type="button"
              onClick={handleClear}
              style={{
                padding: '4px',
                background: 'none',
                border: 'none',
                cursor: 'pointer',
                color: '#9ca3af',
                borderRadius: '4px',
                display: 'flex',
                alignItems: 'center',
                fontSize: '12px',
                transition: 'all 0.15s ease',
                width: '24px',
                height: '24px',
                justifyContent: 'center',
              }}
              onMouseEnter={(e) => {
                e.currentTarget.style.color = '#ef4444';
                e.currentTarget.style.backgroundColor = '#fef2f2';
              }}
              onMouseLeave={(e) => {
                e.currentTarget.style.color = '#9ca3af';
                e.currentTarget.style.backgroundColor = 'transparent';
              }}
            >
              ✕
            </button>
          )}

          {/* Slash shortcut indicator */}
          {!isFocused && !inputValue && (
            <div
              style={{
                padding: '2px 5px',
                background: '#f9fafb',
                border: '1px solid #e5e7eb',
                borderRadius: '3px',
                fontSize: '10px',
                fontWeight: 500,
                color: '#6b7280',
                fontFamily: 'monospace',
                lineHeight: 1,
              }}
            >
              /
            </div>
          )}

          {/* Ask Baaaaml / Close button */}
          <button
            type="button"
            onClick={handleToggleAI}
            style={{
              padding: '6px 10px',
              background: isAIOpen ? '#6b7280' : '#7c3aed',
              border: 'none',
              borderRadius: '6px',
              color: 'white',
              fontSize: '11px',
              fontWeight: 600,
              cursor: 'pointer',
              transition: 'all 0.15s ease',
              display: 'flex',
              alignItems: 'center',
              gap: '4px',
              minWidth: '80px',
              justifyContent: 'center',
              height: '24px',
              boxShadow: '0 1px 2px rgba(0, 0, 0, 0.1)',
            }}
            onMouseEnter={(e) => {
              e.currentTarget.style.background = isAIOpen
                ? '#4b5563'
                : '#6d28d9';
              e.currentTarget.style.transform = 'translateY(-0.5px)';
              e.currentTarget.style.boxShadow = '0 2px 4px rgba(0, 0, 0, 0.15)';
            }}
            onMouseLeave={(e) => {
              e.currentTarget.style.background = isAIOpen
                ? '#6b7280'
                : '#7c3aed';
              e.currentTarget.style.transform = 'translateY(0)';
              e.currentTarget.style.boxShadow = '0 1px 2px rgba(0, 0, 0, 0.1)';
            }}
          >
            {isAIOpen ? (
              <>
                <CloseIcon />
                Close
              </>
            ) : (
              <>
                <AIIcon />
                Ask Baaaaml
              </>
            )}
          </button>
        </div>
      </div>

      {/* Pass selectedIndex and handlers to hits */}
      <CustomHits
        selectedIndex={selectedIndex}
        onAskAI={() => handleAskAI()}
        query={inputValue}
        isFocused={isFocused}
      />
    </div>
  );
}

// Custom Hits component with conditional visibility and Ask Baaaaml option
function CustomHits({
  selectedIndex,
  onAskAI,
  query,
  isFocused,
}: {
  selectedIndex?: number;
  onAskAI?: () => void;
  query?: string;
  isFocused?: boolean;
}) {
  const { hits } = useHits();
  const { query: searchQuery } = useSearchBox();

  const actualQuery = query || searchQuery;

  // Only show results when there's a query and focus, and the query is not empty
  if (!isFocused || !actualQuery.trim()) {
    return null;
  }

  return (
    <div
      style={{
        position: 'absolute',
        top: '100%',
        left: 0,
        right: 0,
        marginTop: '4px',
        background: '#ffffff',
        border: '1px solid #e5e7eb',
        borderRadius: '12px',
        boxShadow: 'none',
        zIndex: 1000,
        maxHeight: '480px',
        overflowY: 'auto',
        overflowX: 'hidden',
      }}
    >
      {/* Ask with AI option */}
      {onAskAI && actualQuery.trim() && (
        <AskWithAIOption
          isSelected={selectedIndex === 0}
          onClick={onAskAI}
          query={actualQuery}
        />
      )}

      {/* Search results */}
      {hits.map((hit: any, index: number) => {
        const adjustedIndex = actualQuery.trim() ? index + 1 : index;
        const isSelected = selectedIndex === adjustedIndex;

        return (
          <Hit key={hit.objectID} hit={hit} isKeyboardSelected={isSelected} />
        );
      })}

      {/* No results message when there are no hits but there's a query */}
      {hits.length === 0 && actualQuery.trim() && (
        <div
          style={{
            padding: '16px',
            textAlign: 'center',
            color: '#6b7280',
            fontSize: '14px',
          }}
        >
          <div style={{ marginBottom: '6px' }}>
            No results found for "{actualQuery}"
          </div>
          {onAskAI && (
            <button
              type="button"
              onClick={onAskAI}
              style={{
                color: '#6366f1',
                background: 'none',
                border: 'none',
                textDecoration: 'underline',
                cursor: 'pointer',
                fontSize: '14px',
                fontWeight: '500',
              }}
            >
              Ask Baaaaml about this instead
            </button>
          )}
        </div>
      )}
    </div>
  );
}

export default function AlgoliaSearch({
  onAskAI,
  onToggleAI,
  isAIOpen,
}: {
  onAskAI?: (query: string) => void;
  onToggleAI?: () => void;
  isAIOpen?: boolean;
}) {
  const containerRef = useRef<HTMLDivElement>(null);
  const [searchClient, setSearchClient] = useState<any>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let mounted = true;

    const initializeSearch = async () => {
      try {
        setIsLoading(true);
        setError(null);

        const credentials = await fetchSearchCredentials();

        if (mounted) {
          const client = algoliasearch(credentials.appId, credentials.apiKey);
          setSearchClient(client);
          setIsLoading(false);
        }
      } catch (err) {
        if (mounted) {
          setError(
            err instanceof Error ? err.message : 'Failed to initialize search',
          );
          setIsLoading(false);
        }
      }
    };

    initializeSearch();

    return () => {
      mounted = false;
    };
  }, []);

  const handleAskAI = (query: string) => {
    if (onAskAI) {
      onAskAI(query);
    }
  };

  const handleToggleAI = () => {
    if (onToggleAI) {
      onToggleAI();
    }
  };

  if (isLoading) {
    return (
      <div style={{ position: 'relative', width: '100%' }}>
        <div
          style={{
            display: 'flex',
            alignItems: 'center',
            background: '#ffffff',
            border: '1.5px solid #e5e7eb',
            borderRadius: '12px',
            padding: '12px 14px',
            color: '#9ca3af',
            fontSize: '14px',
          }}
        >
          <SearchIcon />
          <span style={{ marginLeft: '12px' }}>Loading search...</span>
        </div>
      </div>
    );
  }

  if (error || !searchClient) {
    return (
      <div style={{ position: 'relative', width: '100%' }}>
        <div
          style={{
            display: 'flex',
            alignItems: 'center',
            background: '#ffffff',
            border: '1.5px solid #ef4444',
            borderRadius: '12px',
            padding: '12px 14px',
            color: '#ef4444',
            fontSize: '14px',
          }}
        >
          <SearchIcon />
          <span style={{ marginLeft: '12px' }}>Search unavailable</span>
        </div>
      </div>
    );
  }

  return (
    <div ref={containerRef} style={{ position: 'relative', width: '100%' }}>
      <InstantSearch
        indexName={SEARCH_INDEX_NAME}
        searchClient={searchClient}
        future={{
          preserveSharedStateOnUnmount: true,
        }}
      >
        <Configure
          filters={createSearchFilters()}
          hitsPerPage={6}
          attributesToHighlight={['title', 'description', 'content']}
          attributesToSnippet={['description:80', 'content:60']}
          highlightPreTag="__ais-highlight__"
          highlightPostTag="__/ais-highlight__"
          distinct={true}
          analytics={true}
          analyticsTags={[
            'desktop',
            'docs.boundaryml.com',
            'search-v3-enhanced',
          ]}
        />

        <CustomSearchBox
          onAskAI={handleAskAI}
          onToggleAI={handleToggleAI}
          isAIOpen={isAIOpen}
        />
      </InstantSearch>
    </div>
  );
}
