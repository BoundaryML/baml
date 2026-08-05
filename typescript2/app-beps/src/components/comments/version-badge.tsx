"use client";

import { Badge } from "@/components/ui/badge";
import { cn } from "@/lib/utils";

interface VersionBadgeProps {
  versionNumber: number | null;
  currentVersionNumber?: number | null;
  className?: string;
}

export function VersionBadge({
  versionNumber,
  currentVersionNumber,
  className,
}: VersionBadgeProps) {
  if (versionNumber === null) {
    return null;
  }

  const isCurrentVersion = currentVersionNumber != null && versionNumber === currentVersionNumber;
  const isOutdated = currentVersionNumber != null && versionNumber < currentVersionNumber;

  return (
    <Badge
      variant="outline"
      className={cn(
        "text-[10px] px-1.5 py-0 h-4 font-normal",
        isCurrentVersion && "border-green-500 text-green-700 dark:text-green-400",
        isOutdated && "border-amber-500 text-amber-700 dark:text-amber-400",
        !isCurrentVersion && !isOutdated && "border-muted-foreground text-muted-foreground",
        className
      )}
    >
      v{versionNumber}
      {isOutdated && " (outdated)"}
    </Badge>
  );
}
