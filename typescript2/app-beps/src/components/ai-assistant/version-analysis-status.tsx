"use client";

import { useQuery, useMutation } from "convex/react";
import { api } from "../../../convex/_generated/api";
import { Id } from "../../../convex/_generated/dataModel";
import {
  CheckCircle2,
  XCircle,
  AlertCircle,
  Clock,
  Loader2,
  ChevronDown,
  ChevronUp,
  RotateCcw,
} from "lucide-react";
import { useState } from "react";
import { Button } from "@/components/ui/button";

interface VersionAnalysisStatusProps {
  versionId: Id<"bepVersions">;
}

export function VersionAnalysisStatus({ versionId }: VersionAnalysisStatusProps) {
  const job = useQuery(api.analysisJobs.getByVersion, { versionId });
  const retryAnalysis = useMutation(api.analysisJobs.retry);
  const [expandedSections, setExpandedSections] = useState<Set<string>>(new Set(["summary"]));

  const toggleSection = (section: string) => {
    setExpandedSections(prev => {
      const next = new Set(prev);
      if (next.has(section)) {
        next.delete(section);
      } else {
        next.add(section);
      }
      return next;
    });
  };

  if (job === undefined) {
    return (
      <div className="flex items-center gap-2 text-sm text-muted-foreground">
        <Loader2 className="h-4 w-4 animate-spin" />
        Loading analysis status...
      </div>
    );
  }

  if (job === null) {
    return (
      <div className="text-sm text-muted-foreground">
        No analysis available for this version.
      </div>
    );
  }

  // Status display
  if (job.status === "pending") {
    return (
      <div className="flex items-center gap-2 text-sm text-muted-foreground">
        <Clock className="h-4 w-4" />
        Analysis queued...
      </div>
    );
  }

  if (job.status === "analyzing") {
    return (
      <div className="flex items-center gap-2 text-sm text-blue-600">
        <Loader2 className="h-4 w-4 animate-spin" />
        Analyzing version changes...
      </div>
    );
  }

  if (job.status === "failed") {
    return (
      <div className="space-y-2">
        <div className="flex items-center gap-2 text-sm text-destructive">
          <XCircle className="h-4 w-4" />
          Analysis failed
        </div>
        {job.error && (
          <div className="text-xs text-muted-foreground bg-destructive/10 p-2 rounded">
            {job.error}
          </div>
        )}
        <Button
          variant="outline"
          size="sm"
          onClick={() => retryAnalysis({ jobId: job._id })}
        >
          <RotateCcw className="h-4 w-4 mr-1" />
          Retry Analysis
        </Button>
      </div>
    );
  }

  // Completed - show results
  const result = job.result;
  if (!result) {
    return (
      <div className="text-sm text-muted-foreground">
        Analysis completed but no results available.
      </div>
    );
  }

  const verdictColor = {
    Excellent: "text-green-600",
    Good: "text-blue-600",
    NeedsWork: "text-amber-600",
  }[result.overallVerdict] || "text-muted-foreground";

  const verdictIcon = {
    Excellent: <CheckCircle2 className="h-5 w-5 text-green-600" />,
    Good: <CheckCircle2 className="h-5 w-5 text-blue-600" />,
    NeedsWork: <AlertCircle className="h-5 w-5 text-amber-600" />,
  }[result.overallVerdict] || <AlertCircle className="h-5 w-5" />;

  return (
    <div className="space-y-4">
      {/* Verdict Header */}
      <div className="flex items-start gap-3 p-3 rounded-lg bg-muted/50">
        {verdictIcon}
        <div className="flex-1 min-w-0">
          <div className={`font-semibold ${verdictColor}`}>
            {result.overallVerdict}
          </div>
          <p className="text-sm text-muted-foreground mt-1">
            {result.summary}
          </p>
        </div>
      </div>

      {/* Stats Row */}
      <div className="grid grid-cols-3 gap-2 text-center">
        <div className="p-2 rounded bg-green-50 dark:bg-green-950/30">
          <div className="text-lg font-semibold text-green-600">
            {result.addressedFeedback.length}
          </div>
          <div className="text-xs text-muted-foreground">Addressed</div>
        </div>
        <div className="p-2 rounded bg-amber-50 dark:bg-amber-950/30">
          <div className="text-lg font-semibold text-amber-600">
            {result.partiallyAddressedFeedback.length}
          </div>
          <div className="text-xs text-muted-foreground">Partial</div>
        </div>
        <div className="p-2 rounded bg-red-50 dark:bg-red-950/30">
          <div className="text-lg font-semibold text-red-600">
            {result.unaddressedFeedback.length}
          </div>
          <div className="text-xs text-muted-foreground">Unaddressed</div>
        </div>
      </div>

      {/* Collapsible Sections */}

      {/* Recommendations */}
      {result.recommendations.length > 0 && (
        <CollapsibleSection
          title={`Recommendations (${result.recommendations.length})`}
          expanded={expandedSections.has("recommendations")}
          onToggle={() => toggleSection("recommendations")}
        >
          <ul className="space-y-2">
            {result.recommendations.map((rec, idx) => (
              <li key={idx} className="flex items-start gap-2 text-sm">
                <span className={`px-1.5 py-0.5 text-xs rounded ${
                  rec.priority === "high"
                    ? "bg-red-100 text-red-700 dark:bg-red-900/50 dark:text-red-300"
                    : rec.priority === "medium"
                    ? "bg-amber-100 text-amber-700 dark:bg-amber-900/50 dark:text-amber-300"
                    : "bg-gray-100 text-gray-700 dark:bg-gray-800 dark:text-gray-300"
                }`}>
                  {rec.priority}
                </span>
                <span>{rec.description}</span>
              </li>
            ))}
          </ul>
        </CollapsibleSection>
      )}

      {/* Addressed Feedback */}
      {result.addressedFeedback.length > 0 && (
        <CollapsibleSection
          title={`Addressed Feedback (${result.addressedFeedback.length})`}
          expanded={expandedSections.has("addressed")}
          onToggle={() => toggleSection("addressed")}
        >
          <div className="space-y-3">
            {result.addressedFeedback.map((item, idx) => (
              <FeedbackItem key={idx} item={item} status="addressed" />
            ))}
          </div>
        </CollapsibleSection>
      )}

      {/* Partially Addressed */}
      {result.partiallyAddressedFeedback.length > 0 && (
        <CollapsibleSection
          title={`Partially Addressed (${result.partiallyAddressedFeedback.length})`}
          expanded={expandedSections.has("partial")}
          onToggle={() => toggleSection("partial")}
        >
          <div className="space-y-3">
            {result.partiallyAddressedFeedback.map((item, idx) => (
              <FeedbackItem key={idx} item={item} status="partial" />
            ))}
          </div>
        </CollapsibleSection>
      )}

      {/* Unaddressed Feedback */}
      {result.unaddressedFeedback.length > 0 && (
        <CollapsibleSection
          title={`Unaddressed Feedback (${result.unaddressedFeedback.length})`}
          expanded={expandedSections.has("unaddressed")}
          onToggle={() => toggleSection("unaddressed")}
        >
          <div className="space-y-3">
            {result.unaddressedFeedback.map((item, idx) => (
              <FeedbackItem key={idx} item={item} status="unaddressed" />
            ))}
          </div>
        </CollapsibleSection>
      )}
    </div>
  );
}

// Helper Components

function CollapsibleSection({
  title,
  expanded,
  onToggle,
  children,
}: {
  title: string;
  expanded: boolean;
  onToggle: () => void;
  children: React.ReactNode;
}) {
  return (
    <div className="border rounded-lg">
      <button
        onClick={onToggle}
        className="w-full flex items-center justify-between p-3 text-sm font-medium hover:bg-muted/50 transition-colors"
      >
        {title}
        {expanded ? (
          <ChevronUp className="h-4 w-4" />
        ) : (
          <ChevronDown className="h-4 w-4" />
        )}
      </button>
      {expanded && (
        <div className="p-3 pt-0 border-t">
          {children}
        </div>
      )}
    </div>
  );
}

function FeedbackItem({
  item,
  status,
}: {
  item: {
    feedbackType: string;
    originalContent: string;
    evidence: string;
    explanation: string;
  };
  status: "addressed" | "partial" | "unaddressed";
}) {
  const statusColors = {
    addressed: "border-l-green-500",
    partial: "border-l-amber-500",
    unaddressed: "border-l-red-500",
  };

  return (
    <div className={`pl-3 border-l-2 ${statusColors[status]} space-y-1`}>
      <div className="flex items-center gap-2">
        <span className="text-xs px-1.5 py-0.5 rounded bg-muted">
          {item.feedbackType}
        </span>
      </div>
      <p className="text-sm text-muted-foreground line-clamp-2">
        {item.originalContent}
      </p>
      <p className="text-sm">{item.explanation}</p>
      {item.evidence && status !== "unaddressed" && (
        <blockquote className="text-xs text-muted-foreground italic border-l-2 pl-2 mt-1">
          &ldquo;{item.evidence}&rdquo;
        </blockquote>
      )}
    </div>
  );
}
