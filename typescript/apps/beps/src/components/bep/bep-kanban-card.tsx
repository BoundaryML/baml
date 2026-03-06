"use client";

import { useState, useEffect } from "react";
import Link from "next/link";
import { Card, CardContent } from "@/components/ui/card";
import { MessageSquare, AlertCircle } from "lucide-react";

type BepStatus =
  | "draft"
  | "proposed"
  | "accepted"
  | "implemented"
  | "rejected"
  | "superseded";

interface BepKanbanCardProps {
  number: number;
  title: string;
  status: BepStatus;
  shepherdNames: string[];
  commentCount: number;
  openIssueCount: number;
  updatedAt: number;
}

function formatRelativeTime(timestamp: number, now: number): string {
  const diff = now - timestamp;
  const minutes = Math.floor(diff / 60000);
  const hours = Math.floor(diff / 3600000);
  const days = Math.floor(diff / 86400000);

  if (minutes < 1) return "just now";
  if (minutes < 60) return `${minutes}m ago`;
  if (hours < 24) return `${hours}h ago`;
  if (days < 7) return `${days}d ago`;
  return new Date(timestamp).toLocaleDateString();
}

export function BepKanbanCard({
  number,
  title,
  shepherdNames,
  commentCount,
  openIssueCount,
  updatedAt,
}: BepKanbanCardProps) {
  const [relativeTime, setRelativeTime] = useState<string>("");

  useEffect(() => {
    setRelativeTime(formatRelativeTime(updatedAt, Date.now()));
  }, [updatedAt]);

  return (
    <Link href={`/beps/${number}`}>
      <Card className="hover:bg-accent/50 transition-colors cursor-pointer">
        <CardContent className="p-3">
          <div className="space-y-2">
            <div>
              <span className="text-xs text-muted-foreground font-mono">
                BEP-{String(number).padStart(3, "0")}
              </span>
              <h4 className="text-sm font-medium leading-tight mt-0.5 line-clamp-2">
                {title}
              </h4>
            </div>
            {shepherdNames.length > 0 && (
              <p className="text-xs text-muted-foreground truncate">
                {shepherdNames.join(", ")}
              </p>
            )}
            <div className="flex items-center justify-between text-xs text-muted-foreground pt-1 border-t">
              <div className="flex items-center gap-2">
                <span className="flex items-center gap-0.5">
                  <MessageSquare className="h-3 w-3" />
                  {commentCount}
                </span>
                {openIssueCount > 0 && (
                  <span className="flex items-center gap-0.5 text-yellow-600">
                    <AlertCircle className="h-3 w-3" />
                    {openIssueCount}
                  </span>
                )}
              </div>
              {relativeTime && <span>{relativeTime}</span>}
            </div>
          </div>
        </CardContent>
      </Card>
    </Link>
  );
}
