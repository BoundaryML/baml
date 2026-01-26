"use client";

import { useQuery } from "convex/react";
import { api } from "../../../convex/_generated/api";
import { Id } from "../../../convex/_generated/dataModel";
import { DecisionCard } from "./decision-card";
import { DecisionForm } from "./decision-form";
import { Skeleton } from "@/components/ui/skeleton";
import { Gavel } from "lucide-react";

interface DecisionListProps {
  bepId: Id<"beps">;
  currentVersionNumber?: number | null;
  onNavigateToComment?: (
    commentId: Id<"comments">,
    pageId?: Id<"bepPages"> | null,
    versionId?: Id<"bepVersions"> | null
  ) => void;
}

export function DecisionList({ bepId, currentVersionNumber, onNavigateToComment }: DecisionListProps) {
  const decisions = useQuery(api.decisions.byBep, { bepId });

  if (decisions === undefined) {
    return (
      <div className="space-y-4">
        <Skeleton className="h-8 w-48" />
        <Skeleton className="h-24 w-full" />
        <Skeleton className="h-24 w-full" />
      </div>
    );
  }

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-2">
          <Gavel className="h-5 w-5" />
          <h3 className="font-semibold">Decisions ({decisions.length})</h3>
        </div>
        <DecisionForm bepId={bepId} />
      </div>

      {decisions.length > 0 ? (
        <div className="space-y-3">
          {decisions.map((decision) => (
            <DecisionCard
              key={decision._id}
              decision={decision}
              currentVersionNumber={currentVersionNumber}
              onNavigateToComment={onNavigateToComment}
            />
          ))}
        </div>
      ) : (
        <p className="text-muted-foreground text-center py-8">
          No decisions recorded yet.
        </p>
      )}
    </div>
  );
}
