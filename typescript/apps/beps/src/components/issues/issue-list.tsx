"use client";

import { useState } from "react";
import { useQuery } from "convex/react";
import { api } from "../../../convex/_generated/api";
import { Id } from "../../../convex/_generated/dataModel";
import { IssueCard } from "./issue-card";
import { IssueForm } from "./issue-form";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { AlertCircle, Eye, EyeOff } from "lucide-react";

interface IssueListProps {
  bepId: Id<"beps">;
  currentVersionNumber?: number | null;
  onNavigateToComment?: (
    commentId: Id<"comments">,
    pageId?: Id<"bepPages"> | null,
    versionId?: Id<"bepVersions"> | null
  ) => void;
}

export function IssueList({ bepId, currentVersionNumber, onNavigateToComment }: IssueListProps) {
  const [showResolved, setShowResolved] = useState(false);
  const issues = useQuery(api.issues.byBep, { bepId });

  if (issues === undefined) {
    return (
      <div className="space-y-4">
        <Skeleton className="h-8 w-48" />
        <Skeleton className="h-24 w-full" />
        <Skeleton className="h-24 w-full" />
      </div>
    );
  }

  const openIssues = issues.filter((i) => !i.resolved);
  const resolvedIssues = issues.filter((i) => i.resolved);
  const displayedIssues = showResolved ? issues : openIssues;

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-2">
          <AlertCircle className="h-5 w-5" />
          <h3 className="font-semibold">
            Open Issues ({openIssues.length})
            {resolvedIssues.length > 0 && (
              <span className="text-muted-foreground font-normal">
                {" "}
                + {resolvedIssues.length} resolved
              </span>
            )}
          </h3>
        </div>
        <div className="flex items-center gap-2">
          {resolvedIssues.length > 0 && (
            <Button
              variant="ghost"
              size="sm"
              onClick={() => setShowResolved(!showResolved)}
            >
              {showResolved ? (
                <>
                  <EyeOff className="h-4 w-4 mr-1" />
                  Hide resolved
                </>
              ) : (
                <>
                  <Eye className="h-4 w-4 mr-1" />
                  Show resolved
                </>
              )}
            </Button>
          )}
          <IssueForm bepId={bepId} />
        </div>
      </div>

      {displayedIssues.length > 0 ? (
        <div className="space-y-3">
          {displayedIssues.map((issue) => (
            <IssueCard
              key={issue._id}
              issue={issue}
              currentVersionNumber={currentVersionNumber}
              onNavigateToComment={onNavigateToComment}
            />
          ))}
        </div>
      ) : (
        <p className="text-muted-foreground text-center py-8">
          {showResolved
            ? "No issues recorded yet."
            : "No open issues. Great job!"}
        </p>
      )}
    </div>
  );
}
