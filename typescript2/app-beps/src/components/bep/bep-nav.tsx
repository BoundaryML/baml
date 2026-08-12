"use client";

import { Plus, MessageSquare } from "lucide-react";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import { buildBepPath, MAIN_CONTENT_ID } from "@/lib/bep-routes";
import { useEffect, useRef, type MouseEvent } from "react";

interface Section {
  id: string;
  title: string;
  hasContent: boolean;
  parentSlug?: string;
}

type PageStatus = "modified" | "new" | "deleted";

interface BepNavProps {
  sections: Section[];
  activeSection: string;
  onSectionClick: (id: string) => void;
  bepNumber: number;
  versionNumber?: number | null;
  commentCounts?: Record<string, number>;
  totalCommentCount?: number;
  openIssueCount?: number;
  decisionCount?: number;
  hideMetaSections?: boolean;
  isEditMode?: boolean;
  pageStatuses?: Record<string, PageStatus>;
  onAddPage?: () => void;
}

function sectionHref(
  sectionId: string,
  bepNumber: number,
  versionNumber?: number | null
): string {
  if (sectionId === MAIN_CONTENT_ID) {
    return buildBepPath({ bepNumber, section: "readme", versionNumber });
  }
  if (sectionId === "issues") {
    return buildBepPath({ bepNumber, section: "issues" });
  }
  if (sectionId === "decisions") {
    return buildBepPath({ bepNumber, section: "decisions" });
  }
  if (sectionId === "ai") {
    return buildBepPath({ bepNumber, section: "ai" });
  }
  if (sectionId === "comments") {
    return buildBepPath({ bepNumber, section: "comments" });
  }
  return buildBepPath({
    bepNumber,
    section: "page",
    pageSlug: sectionId,
    versionNumber,
  });
}

function handleNavClick(
  e: MouseEvent,
  sectionId: string,
  onSectionClick: (id: string) => void
) {
  if (e.metaKey || e.ctrlKey || e.shiftKey || e.button === 1) return;
  e.preventDefault();
  onSectionClick(sectionId);
}

export function BepNav({
  sections,
  activeSection,
  onSectionClick,
  bepNumber,
  versionNumber,
  commentCounts = {},
  totalCommentCount = 0,
  openIssueCount = 0,
  decisionCount = 0,
  hideMetaSections = false,
  isEditMode = false,
  pageStatuses = {},
  onAddPage,
}: BepNavProps) {
  // Arrange sections as a tree: children (parentSlug) render under their
  // parent, indented one step per ancestor. Children whose parent isn't
  // visible stay at top level.
  const visibleSections = sections.filter(
    (s) => s.hasContent || pageStatuses[s.id] === "new"
  );
  const visibleIds = new Set(visibleSections.map((s) => s.id));
  const childrenByParent = new Map<string, Section[]>();
  const rootSections: Section[] = [];
  for (const section of visibleSections) {
    // Self-referential pages (parentSlug === own id) render at top level
    if (
      section.parentSlug &&
      section.parentSlug !== section.id &&
      visibleIds.has(section.parentSlug)
    ) {
      const siblings = childrenByParent.get(section.parentSlug) ?? [];
      siblings.push(section);
      childrenByParent.set(section.parentSlug, siblings);
    } else {
      rootSections.push(section);
    }
  }
  const orderedSections: Array<Section & { depth: number }> = [];
  const placed = new Set<string>();
  const appendSubtree = (section: Section, depth: number) => {
    if (placed.has(section.id)) return;
    placed.add(section.id);
    orderedSections.push({ ...section, depth });
    for (const child of childrenByParent.get(section.id) ?? []) {
      appendSubtree(child, depth + 1);
    }
  };
  for (const section of rootSections) appendSubtree(section, 0);
  // Sections trapped in parent cycles have no root; render them at top level
  for (const section of visibleSections) appendSubtree(section, 0);

  // Keep the active item visible inside the sidebar's own scroll container
  // without touching window scroll (scrollIntoView would also scroll
  // ancestors, which is exactly the jarring jump we're avoiding).
  const navRef = useRef<HTMLElement>(null);
  useEffect(() => {
    const nav = navRef.current;
    if (!nav) return;
    const active = nav.querySelector<HTMLElement>('[aria-current="page"]');
    if (!active) return;
    // The scrollable ancestor is the sticky wrapper around this nav
    const container = nav.parentElement;
    if (!container || container.scrollHeight <= container.clientHeight) return;
    const containerTop = container.getBoundingClientRect().top;
    const activeRect = active.getBoundingClientRect();
    const top = activeRect.top - containerTop + container.scrollTop;
    const bottom = top + activeRect.height;
    if (top < container.scrollTop) {
      container.scrollTop = top - 8;
    } else if (bottom > container.scrollTop + container.clientHeight) {
      container.scrollTop = bottom - container.clientHeight + 8;
    }
  }, [activeSection]);

  return (
    <nav ref={navRef} className="space-y-1">
      {orderedSections
        .map((section) => {
          const status = pageStatuses[section.id];
          const isDeleted = status === "deleted";

          return (
            <a
              key={section.id}
              href={sectionHref(section.id, bepNumber, versionNumber)}
              onClick={(e) => handleNavClick(e, section.id, onSectionClick)}
              aria-current={activeSection === section.id ? "page" : undefined}
              className={cn(
                "block w-full text-left px-3 py-2 rounded-md text-sm transition-colors",
                "hover:bg-accent hover:text-accent-foreground",
                activeSection === section.id
                  ? "bg-accent text-accent-foreground font-medium"
                  : "text-muted-foreground",
                isDeleted && "opacity-50 line-through",
                section.depth > 0 && "border-l pl-3"
              )}
              style={
                section.depth > 0
                  ? { marginLeft: `${section.depth}rem` }
                  : undefined
              }
            >
              <span className="flex items-center justify-between gap-2">
                <span className="truncate">{section.title}</span>
                <span className="flex items-center gap-1 shrink-0">
                  {isEditMode && status && (
                    <span
                      className={cn(
                        "w-2 h-2 rounded-full",
                        status === "modified" && "bg-blue-500",
                        status === "new" && "bg-green-500",
                        status === "deleted" && "bg-red-500"
                      )}
                      title={status}
                    />
                  )}
                  {!isEditMode && (commentCounts[section.id] ?? 0) > 0 && (
                    <span className="text-xs bg-muted px-1.5 py-0.5 rounded">
                      {commentCounts[section.id]}
                    </span>
                  )}
                </span>
              </span>
            </a>
          );
        })}

      {isEditMode && onAddPage && (
        <Button
          variant="ghost"
          size="sm"
          onClick={onAddPage}
          className="w-full justify-start gap-2 text-muted-foreground mt-2"
        >
          <Plus className="h-4 w-4" />
          Add page
        </Button>
      )}

      {!hideMetaSections && (
        <>
          <div className="border-t my-3" />

          <a
            href={sectionHref("issues", bepNumber)}
            onClick={(e) => handleNavClick(e, "issues", onSectionClick)}
            className={cn(
              "block w-full text-left px-3 py-2 rounded-md text-sm transition-colors",
              "hover:bg-accent hover:text-accent-foreground",
              activeSection === "issues"
                ? "bg-accent text-accent-foreground font-medium"
                : "text-muted-foreground"
            )}
          >
            <span className="flex items-center justify-between">
              Open Issues
              {openIssueCount > 0 && (
                <span className="text-xs bg-yellow-100 text-yellow-800 px-1.5 py-0.5 rounded">
                  {openIssueCount}
                </span>
              )}
            </span>
          </a>

          <a
            href={sectionHref("decisions", bepNumber)}
            onClick={(e) => handleNavClick(e, "decisions", onSectionClick)}
            className={cn(
              "block w-full text-left px-3 py-2 rounded-md text-sm transition-colors",
              "hover:bg-accent hover:text-accent-foreground",
              activeSection === "decisions"
                ? "bg-accent text-accent-foreground font-medium"
                : "text-muted-foreground"
            )}
          >
            <span className="flex items-center justify-between">
              Decisions
              {decisionCount > 0 && (
                <span className="text-xs bg-muted px-1.5 py-0.5 rounded">
                  {decisionCount}
                </span>
              )}
            </span>
          </a>

          <a
            href={sectionHref("comments", bepNumber)}
            onClick={(e) => handleNavClick(e, "comments", onSectionClick)}
            className={cn(
              "block w-full text-left px-3 py-2 rounded-md text-sm transition-colors",
              "hover:bg-accent hover:text-accent-foreground",
              activeSection === "comments"
                ? "bg-accent text-accent-foreground font-medium"
                : "text-muted-foreground"
            )}
          >
            <span className="flex items-center justify-between">
              <span className="flex items-center gap-1.5">
                <MessageSquare className="h-4 w-4" />
                All Comments
              </span>
              {totalCommentCount > 0 && (
                <span className="text-xs bg-muted px-1.5 py-0.5 rounded">
                  {totalCommentCount}
                </span>
              )}
            </span>
          </a>

          <div className="border-t mb-3" />

          <a
            href={sectionHref("ai", bepNumber)}
            onClick={(e) => handleNavClick(e, "ai", onSectionClick)}
            className={cn(
              "block w-full text-left px-3 py-2 rounded-md text-sm transition-colors",
              "hover:bg-accent hover:text-accent-foreground",
              activeSection === "ai"
                ? "bg-accent text-accent-foreground font-medium"
                : "text-muted-foreground"
            )}
          >
            <span className="flex items-center justify-between">
              AI Assistant
              <span className="text-xs bg-purple-100 text-purple-800 px-1.5 py-0.5 rounded">
                Beta
              </span>
            </span>
          </a>
        </>
      )}
    </nav>
  );
}
