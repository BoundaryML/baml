"use client";

import { useState, useEffect } from "react";
import Link from "next/link";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { BepStatusBadge } from "./bep-status";
import { MessageSquare, AlertCircle } from "lucide-react";

type BepStatus =
  | "draft"
  | "proposed"
  | "accepted"
  | "implemented"
  | "rejected"
  | "superseded";

interface BepCardProps {
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

export function BepCard({
  number,
  title,
  status,
  shepherdNames,
  commentCount,
  openIssueCount,
  updatedAt,
}: BepCardProps) {
  // Use state with effect to avoid hydration mismatch
  const [relativeTime, setRelativeTime] = useState<string>("");

  useEffect(() => {
    setRelativeTime(formatRelativeTime(updatedAt, Date.now()));
  }, [updatedAt]);

  return (
    <Link href={`/beps/${number}`}>
      <Card className="hover:bg-accent/50 transition-colors cursor-pointer">
        <CardHeader className="pb-2">
          <div className="flex items-start justify-between gap-4">
            <CardTitle className="text-lg">
              <span className="text-muted-foreground font-mono">
                BEP-{String(number).padStart(3, "0")}
              </span>{" "}
              {title}
            </CardTitle>
            <BepStatusBadge status={status} />
          </div>
        </CardHeader>
        <CardContent>
          <div className="flex items-center justify-between text-sm text-muted-foreground">
            <div>
              {shepherdNames.length > 0 && (
                <span>Shepherds: {shepherdNames.join(", ")}</span>
              )}
            </div>
            <div className="flex items-center gap-4">
              <span className="flex items-center gap-1">
                <MessageSquare className="h-4 w-4" />
                {commentCount}
              </span>
              {openIssueCount > 0 && (
                <span className="flex items-center gap-1 text-yellow-600">
                  <AlertCircle className="h-4 w-4" />
                  {openIssueCount} open
                </span>
              )}
              {relativeTime && <span>Updated {relativeTime}</span>}
            </div>
          </div>
        </CardContent>
      </Card>
    </Link>
  );
}
