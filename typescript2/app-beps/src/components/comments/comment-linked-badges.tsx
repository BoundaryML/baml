"use client";

import { AlertCircle, Gavel } from "lucide-react";
import { Badge } from "@/components/ui/badge";

interface LinkedItem {
  _id: string;
  title: string;
  resolved?: boolean;
}

interface CommentLinkedBadgesProps {
  issues: LinkedItem[];
  decisions: LinkedItem[];
  onNavigateToIssue?: (issueId: string) => void;
  onNavigateToDecision?: (decisionId: string) => void;
}

export function CommentLinkedBadges({
  issues,
  decisions,
  onNavigateToIssue,
  onNavigateToDecision,
}: CommentLinkedBadgesProps) {
  if (issues.length === 0 && decisions.length === 0) {
    return null;
  }

  return (
    <div className="flex items-center gap-1.5 flex-wrap">
      {issues.map((issue) => (
        <Badge
          key={issue._id}
          variant={issue.resolved ? "secondary" : "concern"}
          className={`cursor-pointer text-xs px-1.5 py-0 h-5 ${
            issue.resolved ? "opacity-60" : ""
          }`}
          onClick={() => onNavigateToIssue?.(issue._id)}
          title={issue.title + (issue.resolved ? " (Resolved)" : "")}
        >
          <AlertCircle className="h-3 w-3 mr-1" />
          Issue
        </Badge>
      ))}

      {decisions.map((decision) => (
        <Badge
          key={decision._id}
          variant="default"
          className="cursor-pointer text-xs px-1.5 py-0 h-5 bg-blue-600 hover:bg-blue-700"
          onClick={() => onNavigateToDecision?.(decision._id)}
          title={decision.title}
        >
          <Gavel className="h-3 w-3 mr-1" />
          Decision
        </Badge>
      ))}
    </div>
  );
}
