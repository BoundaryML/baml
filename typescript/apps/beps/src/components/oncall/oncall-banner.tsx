"use client";

import { useQuery } from "convex/react";
import { api } from "../../../convex/_generated/api";
import { Badge } from "@/components/ui/badge";
import { Phone } from "lucide-react";

export function OnCallBanner() {
  const onCall = useQuery(api.onCall.getCurrent);

  // Don't render anything if no on-call data or still loading
  if (onCall === undefined || onCall === null || !onCall.currentUser) {
    return null;
  }

  const { currentUser, nextUser, weekNumber, year } = onCall;

  return (
    <div className="bg-gradient-to-r from-amber-50 to-orange-50 dark:from-amber-950/30 dark:to-orange-950/30 border-b border-amber-200 dark:border-amber-800">
      <div className="max-w-[1600px] mx-auto px-4 py-2">
        <div className="flex items-center justify-between gap-4">
          <div className="flex items-center gap-3">
            <Badge
              variant="outline"
              className="bg-amber-100 dark:bg-amber-900/50 border-amber-300 dark:border-amber-700 text-amber-800 dark:text-amber-200 gap-1.5"
            >
              <Phone className="h-3 w-3" />
              On-Call
            </Badge>
            <div className="flex items-center gap-2">
              {currentUser.avatarUrl ? (
                <img
                  src={currentUser.avatarUrl}
                  alt={currentUser.name}
                  className="w-6 h-6 rounded-full ring-2 ring-amber-400 dark:ring-amber-600"
                />
              ) : (
                <div className="w-6 h-6 rounded-full bg-amber-200 dark:bg-amber-800 flex items-center justify-center ring-2 ring-amber-400 dark:ring-amber-600">
                  <span className="text-xs font-medium text-amber-800 dark:text-amber-200">
                    {currentUser.name.charAt(0).toUpperCase()}
                  </span>
                </div>
              )}
              <span className="font-medium text-amber-900 dark:text-amber-100">
                {currentUser.name}
              </span>
              <span className="text-amber-600 dark:text-amber-400 text-sm">
                is on-call this week
              </span>
            </div>
          </div>
          <div className="flex items-center gap-4 text-sm text-amber-700 dark:text-amber-300">
            <span className="hidden sm:inline">
              Week {weekNumber} of {year}
            </span>
            {nextUser && (
              <span className="hidden md:inline text-amber-600 dark:text-amber-400">
                Next: {nextUser.name}
              </span>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
