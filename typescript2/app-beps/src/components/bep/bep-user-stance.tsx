"use client";

import { useState } from "react";
import { useQuery, useMutation } from "convex/react";
import { api } from "../../../convex/_generated/api";
import { Id } from "../../../convex/_generated/dataModel";
import { useUser } from "@/components/providers/user-provider";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
} from "@/components/ui/dialog";
import { Textarea } from "@/components/ui/textarea";
import { ThumbsUp, ThumbsDown, Minus, Users, BarChart3 } from "lucide-react";
import { cn } from "@/lib/utils";

type UserStance = "support" | "neutral" | "oppose";

interface BepUserStanceProps {
  bepId: Id<"beps">;
  versionId: Id<"bepVersions">;
  versionNumber: number;
  readOnly?: boolean;
}

const STANCE_CONFIG: Record<
  UserStance,
  { icon: typeof ThumbsUp; label: string; color: string; bgColor: string }
> = {
  support: {
    icon: ThumbsUp,
    label: "Support",
    color: "text-green-600",
    bgColor: "bg-green-100 dark:bg-green-900/30",
  },
  neutral: {
    icon: Minus,
    label: "Neutral",
    color: "text-gray-600",
    bgColor: "bg-gray-100 dark:bg-gray-900/30",
  },
  oppose: {
    icon: ThumbsDown,
    label: "Oppose",
    color: "text-red-600",
    bgColor: "bg-red-100 dark:bg-red-900/30",
  },
};

export function BepUserStance({
  bepId,
  versionId,
  versionNumber,
  readOnly = false,
}: BepUserStanceProps) {
  const { userId } = useUser();
  const [showHistoryDialog, setShowHistoryDialog] = useState(false);
  const [comment, setComment] = useState("");
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [popoverOpen, setPopoverOpen] = useState(false);

  const stanceSummary = useQuery(api.userStances.getStanceSummary, {
    bepId,
    versionId,
  });

  const userStance = useQuery(
    api.userStances.getUserStanceForVersion,
    userId
      ? {
          bepId,
          versionId,
          userId,
        }
      : "skip"
  );

  const versionStances = useQuery(api.userStances.getByBepVersion, {
    bepId,
    versionId,
  });

  const historicalStances = useQuery(api.userStances.getHistoricalStances, {
    bepId,
  });

  const setStance = useMutation(api.userStances.setStance);

  const handleSetStance = async (stance: UserStance) => {
    if (!userId || isSubmitting) return;

    setIsSubmitting(true);
    try {
      await setStance({
        bepId,
        versionId,
        userId,
        stance,
        comment: comment.trim() || undefined,
      });
      setComment("");
      setPopoverOpen(false);
    } catch (error) {
      console.error("Failed to set stance:", error);
    } finally {
      setIsSubmitting(false);
    }
  };

  const currentStance = userStance?.stance;

  return (
    <div className="flex items-center gap-2">
      {/* Summary display */}
      {stanceSummary && stanceSummary.total > 0 && (
        <div className="flex items-center gap-1 text-sm text-muted-foreground">
          <Users className="h-3.5 w-3.5" />
          <span className="flex items-center gap-1">
            <span className="text-green-600">{stanceSummary.support}</span>
            <span>/</span>
            <span className="text-gray-600">{stanceSummary.neutral}</span>
            <span>/</span>
            <span className="text-red-600">{stanceSummary.oppose}</span>
          </span>
        </div>
      )}

      {/* Set stance button */}
      {!readOnly && userId && (
        <Popover open={popoverOpen} onOpenChange={setPopoverOpen}>
          <PopoverTrigger asChild>
            <Button variant="outline" size="sm" className="gap-2">
              {currentStance ? (
                <>
                  {(() => {
                    const config = STANCE_CONFIG[currentStance];
                    const Icon = config.icon;
                    return (
                      <>
                        <Icon className={cn("h-4 w-4", config.color)} />
                        <span className={config.color}>{config.label}</span>
                      </>
                    );
                  })()}
                </>
              ) : (
                <>
                  <Users className="h-4 w-4" />
                  Add Stance
                </>
              )}
            </Button>
          </PopoverTrigger>
          <PopoverContent className="w-72" align="end">
            <div className="space-y-3">
              <div className="text-sm font-medium">
                Your stance on Version {versionNumber}
              </div>
              <div className="flex gap-2">
                {(["support", "neutral", "oppose"] as const).map(
                  (stance) => {
                    const config = STANCE_CONFIG[stance];
                    const Icon = config.icon;
                    const isSelected = currentStance === stance;
                    return (
                      <Button
                        key={stance}
                        variant={isSelected ? "default" : "outline"}
                        size="sm"
                        className={cn(
                          "flex-1",
                          isSelected && config.bgColor,
                          isSelected && config.color
                        )}
                        onClick={() => handleSetStance(stance)}
                        disabled={isSubmitting}
                      >
                        <Icon className="h-4 w-4 mr-1" />
                        {config.label}
                      </Button>
                    );
                  }
                )}
              </div>
              <div className="space-y-1">
                <label className="text-xs text-muted-foreground">
                  Optional comment
                </label>
                <Textarea
                  placeholder="Brief reason for your stance..."
                  value={comment}
                  onChange={(e) => setComment(e.target.value)}
                  className="text-sm resize-none"
                  rows={2}
                />
              </div>
            </div>
          </PopoverContent>
        </Popover>
      )}

      {/* History button */}
      {historicalStances && historicalStances.length > 0 && (
        <Button
          variant="ghost"
          size="sm"
          onClick={() => setShowHistoryDialog(true)}
          className="gap-1.5"
        >
          <BarChart3 className="h-4 w-4" />
          <span className="text-xs">History</span>
        </Button>
      )}

      {/* History dialog */}
      <Dialog open={showHistoryDialog} onOpenChange={setShowHistoryDialog}>
        <DialogContent className="max-w-2xl max-h-[80vh] overflow-hidden flex flex-col">
          <DialogHeader>
            <DialogTitle>Stance History</DialogTitle>
            <DialogDescription>
              Historical view of team acceptance across versions
            </DialogDescription>
          </DialogHeader>

          <div className="flex-1 overflow-y-auto space-y-4 mt-4">
            {/* Chart-like visualization */}
            {historicalStances && historicalStances.length > 0 && (
              <div className="space-y-3">
                {historicalStances.map((version: {
                  versionId: Id<"bepVersions">;
                  versionNumber: number;
                  createdAt: number;
                  stanceSummary: { support: number; neutral: number; oppose: number; total: number };
                }) => (
                  <div key={version.versionId} className="space-y-1">
                    <div className="flex items-center justify-between text-sm">
                      <span className="font-medium">
                        Version {version.versionNumber}
                      </span>
                      <span className="text-muted-foreground">
                        {version.stanceSummary.total} response
                        {version.stanceSummary.total !== 1 ? "s" : ""}
                      </span>
                    </div>
                    {version.stanceSummary.total > 0 ? (
                      <div className="flex h-6 rounded-md overflow-hidden">
                        {version.stanceSummary.support > 0 && (
                          <div
                            className="bg-green-500 flex items-center justify-center text-xs text-white font-medium"
                            style={{
                              width: `${(version.stanceSummary.support / version.stanceSummary.total) * 100}%`,
                            }}
                          >
                            {version.stanceSummary.support}
                          </div>
                        )}
                        {version.stanceSummary.neutral > 0 && (
                          <div
                            className="bg-gray-400 flex items-center justify-center text-xs text-white font-medium"
                            style={{
                              width: `${(version.stanceSummary.neutral / version.stanceSummary.total) * 100}%`,
                            }}
                          >
                            {version.stanceSummary.neutral}
                          </div>
                        )}
                        {version.stanceSummary.oppose > 0 && (
                          <div
                            className="bg-red-500 flex items-center justify-center text-xs text-white font-medium"
                            style={{
                              width: `${(version.stanceSummary.oppose / version.stanceSummary.total) * 100}%`,
                            }}
                          >
                            {version.stanceSummary.oppose}
                          </div>
                        )}
                      </div>
                    ) : (
                      <div className="h-6 rounded-md bg-muted flex items-center justify-center text-xs text-muted-foreground">
                        No responses yet
                      </div>
                    )}
                  </div>
                ))}
              </div>
            )}

            {/* Current version stances */}
            {versionStances && versionStances.length > 0 && (
              <div className="border-t pt-4 mt-4">
                <h4 className="font-medium mb-3">
                  Version {versionNumber} Responses
                </h4>
                <div className="space-y-2">
                  {versionStances.map((stance: {
                    _id: Id<"userStances">;
                    stance: UserStance;
                    comment?: string;
                    userName: string;
                    userAvatarUrl?: string;
                  }) => {
                    const config = STANCE_CONFIG[stance.stance];
                    const Icon = config.icon;
                    return (
                      <div
                        key={stance._id}
                        className="flex items-start gap-3 p-2 rounded-md bg-muted/50"
                      >
                        <div className="flex items-center gap-2">
                          {stance.userAvatarUrl ? (
                            <img
                              src={stance.userAvatarUrl}
                              alt={stance.userName}
                              className="h-6 w-6 rounded-full"
                            />
                          ) : (
                            <div className="h-6 w-6 rounded-full bg-muted flex items-center justify-center text-xs font-medium">
                              {stance.userName.charAt(0).toUpperCase()}
                            </div>
                          )}
                          <span className="text-sm font-medium">
                            {stance.userName}
                          </span>
                        </div>
                        <Badge variant={stance.stance} className="gap-1">
                          <Icon className="h-3 w-3" />
                          {config.label}
                        </Badge>
                        {stance.comment && (
                          <span className="text-sm text-muted-foreground flex-1">
                            {stance.comment}
                          </span>
                        )}
                      </div>
                    );
                  })}
                </div>
              </div>
            )}
          </div>
        </DialogContent>
      </Dialog>
    </div>
  );
}
