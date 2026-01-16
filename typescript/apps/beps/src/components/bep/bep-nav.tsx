"use client";

import { Plus } from "lucide-react";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";

interface Section {
  id: string;
  title: string;
  hasContent: boolean;
}

type PageStatus = "modified" | "new" | "deleted";

interface BepNavProps {
  sections: Section[];
  activeSection: string;
  onSectionClick: (id: string) => void;
  commentCounts?: Record<string, number>;
  openIssueCount?: number;
  decisionCount?: number;
  hideMetaSections?: boolean; // Hide issues, decisions (for historical viewing)
  // Edit mode props
  isEditMode?: boolean;
  pageStatuses?: Record<string, PageStatus>; // Maps section id to status
  onAddPage?: () => void;
}

export function BepNav({
  sections,
  activeSection,
  onSectionClick,
  commentCounts = {},
  openIssueCount = 0,
  decisionCount = 0,
  hideMetaSections = false,
  isEditMode = false,
  pageStatuses = {},
  onAddPage,
}: BepNavProps) {
  return (
    <nav className="space-y-1">
      {sections
        .filter((s) => s.hasContent || pageStatuses[s.id] === "new")
        .map((section) => {
          const status = pageStatuses[section.id];
          const isDeleted = status === "deleted";

          return (
            <button
              key={section.id}
              onClick={() => onSectionClick(section.id)}
              className={cn(
                "w-full text-left px-3 py-2 rounded-md text-sm transition-colors",
                "hover:bg-accent hover:text-accent-foreground",
                activeSection === section.id
                  ? "bg-accent text-accent-foreground font-medium"
                  : "text-muted-foreground",
                isDeleted && "opacity-50 line-through"
              )}
            >
              <span className="flex items-center justify-between gap-2">
                <span className="truncate">{section.title}</span>
                <span className="flex items-center gap-1 flex-shrink-0">
                  {/* Edit mode status badge */}
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
                  {/* Comment count (only in view mode) */}
                  {!isEditMode && (commentCounts[section.id] ?? 0) > 0 && (
                    <span className="text-xs bg-muted px-1.5 py-0.5 rounded">
                      {commentCounts[section.id]}
                    </span>
                  )}
                </span>
              </span>
            </button>
          );
        })}

      {/* Add page button (only in edit mode) */}
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

          <button
            onClick={() => onSectionClick("issues")}
            className={cn(
              "w-full text-left px-3 py-2 rounded-md text-sm transition-colors",
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
          </button>

          <button
            onClick={() => onSectionClick("decisions")}
            className={cn(
              "w-full text-left px-3 py-2 rounded-md text-sm transition-colors",
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
          </button>

          <div className="border-t mb-3" />

          <button
            onClick={() => onSectionClick("ai-assistant")}
            className={cn(
              "w-full text-left px-3 py-2 rounded-md text-sm transition-colors",
              "hover:bg-accent hover:text-accent-foreground",
              activeSection === "ai-assistant"
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
          </button>
        </>
      )}
    </nav>
  );
}
