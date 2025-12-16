'use client';

import { Input } from '@baml/ui/input';
import { Button } from '@baml/ui/button';
import { cn } from '@baml/ui/lib/utils';
import { useAtom, useAtomValue, useSetAtom } from 'jotai';
import { ChevronUp, ChevronDown, X, Search } from 'lucide-react';
import { useEffect, useRef, useCallback } from 'react';
import {
  promptSearchQueryAtom,
  promptSearchVisibleAtom,
  promptSearchCurrentMatchAtom,
  promptSearchTotalMatchesAtom,
  clearMatchCountsAtom,
} from './search-atoms';

interface PromptSearchBarProps {
  className?: string;
}

export const PromptSearchBar: React.FC<PromptSearchBarProps> = ({ className }) => {
  const [searchQuery, setSearchQuery] = useAtom(promptSearchQueryAtom);
  const [isVisible, setIsVisible] = useAtom(promptSearchVisibleAtom);
  const [currentMatch, setCurrentMatch] = useAtom(promptSearchCurrentMatchAtom);
  const totalMatches = useAtomValue(promptSearchTotalMatchesAtom);
  const clearMatchCounts = useSetAtom(clearMatchCountsAtom);
  const inputRef = useRef<HTMLInputElement>(null);

  // Handle keyboard shortcuts (Ctrl+F / Cmd+F)
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      // Ctrl+F or Cmd+F to open search
      if ((e.ctrlKey || e.metaKey) && e.key === 'f') {
        e.preventDefault();
        setIsVisible(true);
        // Focus input after a short delay to ensure it's rendered
        setTimeout(() => inputRef.current?.focus(), 0);
      }
      // Escape to close search
      if (e.key === 'Escape' && isVisible) {
        e.preventDefault();
        handleClose();
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [isVisible, setIsVisible]);

  // Focus input when search becomes visible
  useEffect(() => {
    if (isVisible) {
      inputRef.current?.focus();
      inputRef.current?.select();
    }
  }, [isVisible]);

  const handleClose = useCallback(() => {
    setIsVisible(false);
    setSearchQuery('');
    setCurrentMatch(0);
    clearMatchCounts();
  }, [setIsVisible, setSearchQuery, setCurrentMatch, clearMatchCounts]);

  const handlePrevMatch = useCallback(() => {
    if (totalMatches > 0) {
      setCurrentMatch((prev) => (prev > 0 ? prev - 1 : totalMatches - 1));
    }
  }, [totalMatches, setCurrentMatch]);

  const handleNextMatch = useCallback(() => {
    if (totalMatches > 0) {
      setCurrentMatch((prev) => (prev < totalMatches - 1 ? prev + 1 : 0));
    }
  }, [totalMatches, setCurrentMatch]);

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter') {
      e.preventDefault();
      if (e.shiftKey) {
        handlePrevMatch();
      } else {
        handleNextMatch();
      }
    }
  };

  if (!isVisible) {
    return null;
  }

  return (
    <div
      className={cn(
        'flex items-center gap-1 px-2 py-1 bg-background border rounded-md shadow-sm',
        className
      )}
    >
      <Search className="size-4 text-muted-foreground flex-shrink-0" />
      <Input
        ref={inputRef}
        type="text"
        placeholder="Search in prompt..."
        value={searchQuery}
        onChange={(e) => {
          setSearchQuery(e.target.value);
          setCurrentMatch(0);
        }}
        onKeyDown={handleKeyDown}
        className="h-7 text-xs border-0 focus-visible:ring-0 focus-visible:ring-offset-0 px-1 min-w-[150px]"
      />
      {searchQuery && (
        <span className="text-xs text-muted-foreground whitespace-nowrap">
          {totalMatches > 0 ? `${currentMatch + 1}/${totalMatches}` : 'No results'}
        </span>
      )}
      <div className="flex items-center gap-0.5">
        <Button
          variant="ghost"
          size="sm"
          className="h-6 w-6 p-0"
          onClick={handlePrevMatch}
          disabled={totalMatches === 0}
          title="Previous match (Shift+Enter)"
        >
          <ChevronUp className="size-3" />
        </Button>
        <Button
          variant="ghost"
          size="sm"
          className="h-6 w-6 p-0"
          onClick={handleNextMatch}
          disabled={totalMatches === 0}
          title="Next match (Enter)"
        >
          <ChevronDown className="size-3" />
        </Button>
        <Button
          variant="ghost"
          size="sm"
          className="h-6 w-6 p-0"
          onClick={handleClose}
          title="Close (Escape)"
        >
          <X className="size-3" />
        </Button>
      </div>
    </div>
  );
};
