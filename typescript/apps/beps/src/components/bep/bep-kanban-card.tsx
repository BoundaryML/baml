"use client";

import { useState, useEffect } from "react";
import Link from "next/link";
import { useSortable } from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";
import { Card, CardContent } from "@/components/ui/card";
import { MessageSquare, AlertCircle, GripVertical } from "lucide-react";
import { Id } from "../../../convex/_generated/dataModel";

type BepStatus =
  | "draft"
  | "proposed"
  | "accepted"
  | "implemented"
  | "rejected"
  | "superseded";

interface BepKanbanCardProps {
  id: Id<"beps">;
  number: number;
  title: string;
  status: BepStatus;
  shepherdNames: string[];
  commentCount: number;
  openIssueCount: number;
  updatedAt: number;
  isDragging?: boolean;
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
  id,
  number,
  title,
  shepherdNames,
  commentCount,
  openIssueCount,
  updatedAt,
}: BepKanbanCardProps) {
  const [relativeTime, setRelativeTime] = useState<string>("");

  const {
    attributes,
    listeners,
    setNodeRef,
    transform,
    transition,
    isDragging,
  } = useSortable({
    id: id,
    data: {
      type: "bep",
      bepId: id,
      number,
    },
  });

  const style = {
    transform: CSS.Transform.toString(transform),
    transition,
    opacity: isDragging ? 0.5 : 1,
  };

  useEffect(() => {
    setRelativeTime(formatRelativeTime(updatedAt, Date.now()));
  }, [updatedAt]);

  return (
    <div ref={setNodeRef} style={style}>
      <Card
        className={`hover:bg-accent/50 transition-colors ${
          isDragging ? "shadow-lg ring-2 ring-primary" : ""
        }`}
      >
        <CardContent className="p-3">
          <div className="space-y-2">
            <div className="flex items-start gap-1">
              <button
                className="mt-0.5 cursor-grab touch-none text-muted-foreground hover:text-foreground active:cursor-grabbing"
                {...attributes}
                {...listeners}
              >
                <GripVertical className="h-4 w-4" />
              </button>
              <Link
                href={`/beps/${number}`}
                className="flex-1 min-w-0"
                onClick={(e) => {
                  if (isDragging) {
                    e.preventDefault();
                  }
                }}
              >
                <span className="text-xs text-muted-foreground font-mono">
                  BEP-{String(number).padStart(3, "0")}
                </span>
                <h4 className="text-sm font-medium leading-tight mt-0.5 line-clamp-2">
                  {title}
                </h4>
              </Link>
            </div>
            {shepherdNames.length > 0 && (
              <p className="text-xs text-muted-foreground truncate pl-5">
                {shepherdNames.join(", ")}
              </p>
            )}
            <div className="flex items-center justify-between text-xs text-muted-foreground pt-1 border-t pl-5">
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
    </div>
  );
}
