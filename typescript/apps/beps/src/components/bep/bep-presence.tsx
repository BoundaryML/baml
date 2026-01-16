"use client";

import { useEffect } from "react";
import { useQuery, useMutation } from "convex/react";
import { api } from "../../../convex/_generated/api";
import { Id } from "../../../convex/_generated/dataModel";
import { cn } from "@/lib/utils";

interface BepPresenceProps {
  bepId: Id<"beps">;
  userId: Id<"users">;
}

// Heartbeat interval in milliseconds (every 15 seconds)
const HEARTBEAT_INTERVAL = 15000;

function getInitials(name: string): string {
  return name
    .split(" ")
    .map((n) => n[0])
    .join("")
    .toUpperCase()
    .slice(0, 2);
}

// Generate a consistent color from a string
function stringToColor(str: string): string {
  const colors = [
    "bg-red-500",
    "bg-orange-500",
    "bg-amber-500",
    "bg-yellow-500",
    "bg-lime-500",
    "bg-green-500",
    "bg-emerald-500",
    "bg-teal-500",
    "bg-cyan-500",
    "bg-sky-500",
    "bg-blue-500",
    "bg-indigo-500",
    "bg-violet-500",
    "bg-purple-500",
    "bg-fuchsia-500",
    "bg-pink-500",
  ];
  let hash = 0;
  for (let i = 0; i < str.length; i++) {
    hash = str.charCodeAt(i) + ((hash << 5) - hash);
  }
  return colors[Math.abs(hash) % colors.length];
}

export function BepPresence({ bepId, userId }: BepPresenceProps) {
  const viewers = useQuery(api.presence.getViewers, { bepId });
  const heartbeat = useMutation(api.presence.heartbeat);
  const leave = useMutation(api.presence.leave);

  // Send heartbeat on mount and periodically
  useEffect(() => {
    // Initial heartbeat
    heartbeat({ bepId, userId });

    // Periodic heartbeat
    const interval = setInterval(() => {
      heartbeat({ bepId, userId });
    }, HEARTBEAT_INTERVAL);

    // Cleanup on unmount
    return () => {
      clearInterval(interval);
      leave({ bepId, userId });
    };
  }, [bepId, userId, heartbeat, leave]);

  // Filter out current user from viewers
  const otherViewers = viewers?.filter((v) => v.userId !== userId) ?? [];

  if (otherViewers.length === 0) {
    return null;
  }

  return (
    <div className="flex items-center gap-1">
      <span className="text-xs text-muted-foreground mr-1">Viewing:</span>
      <div className="flex -space-x-2">
        {otherViewers.slice(0, 5).map((viewer) => (
          <div
            key={viewer.userId}
            className={cn(
              "w-7 h-7 rounded-full flex items-center justify-center text-xs font-medium text-white ring-2 ring-background",
              stringToColor(viewer.name)
            )}
            title={viewer.name}
          >
            {getInitials(viewer.name)}
          </div>
        ))}
        {otherViewers.length > 5 && (
          <div className="w-7 h-7 rounded-full flex items-center justify-center text-xs font-medium bg-muted text-muted-foreground ring-2 ring-background">
            +{otherViewers.length - 5}
          </div>
        )}
      </div>
    </div>
  );
}
