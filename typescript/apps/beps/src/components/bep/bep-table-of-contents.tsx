"use client";

import { useState, useEffect, useMemo, useCallback, useRef } from "react";
import { cn } from "@/lib/utils";
import { ChevronDown, ChevronUp, List } from "lucide-react";

interface TocItem {
  id: string;
  text: string;
  level: number;
}

interface BepTableOfContentsProps {
  content: string;
  className?: string;
}

function slugifyHeading(value: string): string {
  return value
    .toLowerCase()
    .trim()
    .replace(/[^\w\s-]/g, "")
    .replace(/[\s_]+/g, "-")
    .replace(/-+/g, "-")
    .replace(/^-|-$/g, "");
}

function extractHeadings(content: string): TocItem[] {
  if (!content) return [];

  const contentWithoutFrontmatter = content.replace(/^---[\s\S]*?---\n*/, "");
  const headingRegex = /^(#{1,4})\s+(.+)$/gm;
  const headings: TocItem[] = [];
  const slugCounts = new Map<string, number>();

  let match;
  while ((match = headingRegex.exec(contentWithoutFrontmatter)) !== null) {
    const level = match[1].length;
    const text = match[2].trim();
    const baseSlug = slugifyHeading(text);

    if (!baseSlug) continue;

    const count = (slugCounts.get(baseSlug) ?? 0) + 1;
    slugCounts.set(baseSlug, count);
    const id = count === 1 ? baseSlug : `${baseSlug}-${count}`;

    headings.push({ id, text, level });
  }

  return headings;
}

function TocList({
  headings,
  activeId,
  minLevel,
  onItemClick,
}: {
  headings: TocItem[];
  activeId: string | null;
  minLevel: number;
  onItemClick: (id: string) => void;
}) {
  return (
    <ul className="space-y-1 text-sm">
      {headings.map(({ id, text, level }) => {
        const indent = (level - minLevel) * 12;
        const isActive = activeId === id;

        return (
          <li key={id} style={{ paddingLeft: indent }}>
            <button
              onClick={() => onItemClick(id)}
              className={cn(
                "text-left w-full py-1 px-2 rounded-sm transition-colors text-[13px] leading-snug cursor-pointer",
                "hover:text-foreground hover:bg-accent/50",
                isActive
                  ? "text-primary font-medium bg-accent/30"
                  : "text-muted-foreground"
              )}
            >
              {text}
            </button>
          </li>
        );
      })}
    </ul>
  );
}

export function BepTableOfContents({ content, className }: BepTableOfContentsProps) {
  const [activeId, setActiveId] = useState<string | null>(null);
  const [isDesktopCollapsed, setIsDesktopCollapsed] = useState(false);
  const [isMobileExpanded, setIsMobileExpanded] = useState(false);
  const observerRef = useRef<IntersectionObserver | null>(null);
  const headingElementsRef = useRef<Map<string, IntersectionObserverEntry>>(new Map());

  const headings = useMemo(() => extractHeadings(content), [content]);

  const getActiveHeading = useCallback(() => {
    const entries = Array.from(headingElementsRef.current.values());
    const visibleEntries = entries.filter((entry) => entry.isIntersecting);

    if (visibleEntries.length > 0) {
      const sorted = visibleEntries.sort(
        (a, b) => a.boundingClientRect.top - b.boundingClientRect.top
      );
      return sorted[0].target.id;
    }

    const aboveViewport = entries
      .filter((entry) => entry.boundingClientRect.top < 100)
      .sort((a, b) => b.boundingClientRect.top - a.boundingClientRect.top);

    if (aboveViewport.length > 0) {
      return aboveViewport[0].target.id;
    }

    return null;
  }, []);

  useEffect(() => {
    if (headings.length === 0) return;

    const headingElements = headingElementsRef.current;

    observerRef.current = new IntersectionObserver(
      (entries) => {
        entries.forEach((entry) => {
          headingElements.set(entry.target.id, entry);
        });

        const activeHeading = getActiveHeading();
        if (activeHeading) {
          setActiveId(activeHeading);
        }
      },
      {
        rootMargin: "-80px 0px -70% 0px",
        threshold: [0, 1],
      }
    );

    const observer = observerRef.current;

    headings.forEach(({ id }) => {
      const element = document.getElementById(id);
      if (element) {
        observer.observe(element);
      }
    });

    return () => {
      observer.disconnect();
      headingElements.clear();
    };
  }, [headings, getActiveHeading]);

  const handleClick = useCallback((id: string) => {
    const element = document.getElementById(id);
    if (!element) return;

    // Scroll the element into view with proper offset
    // Using scrollIntoView with block: "start" and the CSS scroll-mt-24 class on headings
    element.scrollIntoView({ 
      behavior: "smooth", 
      block: "start",
      inline: "nearest"
    });

    // Update active state immediately for better UX
    setActiveId(id);
    
    // Close mobile menu after navigation
    setIsMobileExpanded(false);

    // Also update the URL hash for better navigation history
    // This doesn't cause a scroll jump because we already scrolled
    window.history.replaceState(null, "", `#${id}`);
  }, []);

  if (headings.length === 0) {
    return null;
  }

  const minLevel = Math.min(...headings.map((h) => h.level));

  return (
    <>
      {/* Mobile TOC - collapsible at top */}
      <div className={cn("xl:hidden mb-6", className)}>
        <div className="border rounded-lg bg-card">
          <button
            onClick={() => setIsMobileExpanded(!isMobileExpanded)}
            className="flex items-center justify-between w-full px-4 py-3 text-sm font-medium text-muted-foreground hover:text-foreground"
          >
            <span className="flex items-center gap-2">
              <List className="h-4 w-4" />
              On this page
            </span>
            {isMobileExpanded ? (
              <ChevronUp className="h-4 w-4" />
            ) : (
              <ChevronDown className="h-4 w-4" />
            )}
          </button>

          {isMobileExpanded && (
            <div className="px-4 pb-4 border-t">
              <div className="pt-3">
                <TocList
                  headings={headings}
                  activeId={activeId}
                  minLevel={minLevel}
                  onItemClick={handleClick}
                />
              </div>
            </div>
          )}
        </div>
      </div>

      {/* Desktop TOC - fixed sidebar on right */}
      <nav
        className="hidden xl:block fixed right-8 top-32 w-56 max-h-[calc(100vh-10rem)] overflow-y-auto"
        aria-label="Table of contents"
      >
        <div className="border-l border-border pl-4">
          <button
            onClick={() => setIsDesktopCollapsed(!isDesktopCollapsed)}
            className="flex items-center gap-2 text-sm font-medium text-muted-foreground hover:text-foreground mb-3 w-full"
          >
            <List className="h-4 w-4" />
            <span>On this page</span>
          </button>

          {!isDesktopCollapsed && (
            <TocList
              headings={headings}
              activeId={activeId}
              minLevel={minLevel}
              onItemClick={handleClick}
            />
          )}
        </div>
      </nav>
    </>
  );
}
