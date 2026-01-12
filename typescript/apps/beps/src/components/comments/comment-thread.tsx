"use client";

import { useState } from "react";
import { useQuery } from "convex/react";
import { api } from "../../../convex/_generated/api";
import { Id } from "../../../convex/_generated/dataModel";
import { CommentItem } from "./comment-item";
import { CommentForm } from "./comment-form";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { MessageSquare, Filter, Check } from "lucide-react";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";

type CommentType =
  | "discussion"
  | "concern"
  | "suggestion"
  | "question"
  | "decision";

interface CommentThreadProps {
  bepId: Id<"beps">;
  versionId: Id<"bepVersions">;
  pageId?: Id<"bepPages">;
  viewingVersionId?: Id<"bepVersions">; // The version being viewed (for filtering)
  readOnly?: boolean; // When viewing historical versions
  onNavigateToIssue?: (issueId: string) => void;
  onNavigateToDecision?: (decisionId: string) => void;
}

export function CommentThread({
  bepId,
  versionId,
  pageId,
  viewingVersionId,
  readOnly = false,
  onNavigateToIssue,
  onNavigateToDecision,
}: CommentThreadProps) {
  const [typeFilter, setTypeFilter] = useState<CommentType | "all">("all");
  const [showResolved, setShowResolved] = useState(false);

  // Use page-aware query with version filtering
  // Always filter by version - use versionId (current) if no viewingVersionId (historical)
  const comments = useQuery(api.comments.byBepPage, {
    bepId,
    pageId,
    versionId: viewingVersionId ?? versionId,
  });

  // Get linked items for all comments in this BEP
  const linkedItemsBatch = useQuery(api.comments.getLinkedItemsBatch, { bepId });

  if (comments === undefined) {
    return (
      <div className="space-y-4">
        <Skeleton className="h-8 w-48" />
        <Skeleton className="h-24 w-full" />
        <Skeleton className="h-24 w-full" />
      </div>
    );
  }

  // Filter to only top-level, non-inline comments (no parentId, no anchor)
  const topLevelComments = comments.filter((c) => !c.parentId && !c.anchor);

  // Apply filters
  const filteredComments = topLevelComments.filter((comment) => {
    if (typeFilter !== "all" && comment.type !== typeFilter) return false;
    if (!showResolved && comment.resolved) return false;
    return true;
  });

  // Sort by newest first
  const sortedComments = [...filteredComments].sort(
    (a, b) => b.createdAt - a.createdAt
  );

  const unresolvedCount = topLevelComments.filter((c) => !c.resolved).length;
  const resolvedCount = topLevelComments.filter((c) => c.resolved).length;

  return (
    <div className="space-y-6">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-2">
          <MessageSquare className="h-5 w-5" />
          <h3 className="font-semibold">
            Comments ({unresolvedCount}
            {resolvedCount > 0 && (
              <span className="text-muted-foreground">
                {" "}
                + {resolvedCount} resolved
              </span>
            )}
            )
          </h3>
        </div>

        <div className="flex items-center gap-2">
          <Select
            value={typeFilter}
            onValueChange={(v) => setTypeFilter(v as CommentType | "all")}
          >
            <SelectTrigger className="w-36 h-8">
              <Filter className="h-3 w-3 mr-2" />
              <SelectValue placeholder="Filter" />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="all">All types</SelectItem>
              <SelectItem value="discussion">Discussion</SelectItem>
              <SelectItem value="concern">Concern</SelectItem>
              <SelectItem value="question">Question</SelectItem>
            </SelectContent>
          </Select>

          <Button
            variant={showResolved ? "secondary" : "outline"}
            size="sm"
            onClick={() => setShowResolved(!showResolved)}
            className="h-8"
          >
            <Check className="h-3 w-3 mr-1" />
            {showResolved ? "Hide resolved" : "Show resolved"}
          </Button>
        </div>
      </div>

      {/* Comment form */}
      {readOnly ? (
        <div className="border rounded-lg p-4 bg-muted/30 text-center text-muted-foreground">
          Comments are read-only when viewing historical versions.
        </div>
      ) : (
        <div className="border rounded-lg p-4 bg-muted/30">
          <CommentForm bepId={bepId} versionId={versionId} pageId={pageId} />
        </div>
      )}

      {/* Comments list */}
      <div className="divide-y">
        {sortedComments.length > 0 ? (
          sortedComments.map((comment) => (
            <CommentItem
              key={comment._id}
              comment={comment}
              versionId={versionId}
              readOnly={readOnly}
              linkedItems={linkedItemsBatch?.[comment._id]}
              onNavigateToIssue={onNavigateToIssue}
              onNavigateToDecision={onNavigateToDecision}
            />
          ))
        ) : (
          <div className="text-center py-8 text-muted-foreground">
            {typeFilter !== "all" || !showResolved
              ? "No comments match your filters."
              : "No comments yet. Be the first to comment!"}
          </div>
        )}
      </div>
    </div>
  );
}
