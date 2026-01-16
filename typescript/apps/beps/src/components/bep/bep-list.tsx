"use client";

import { useState } from "react";
import { useQuery } from "convex/react";
import { api } from "../../../convex/_generated/api";
import { BepCard } from "./bep-card";
import { Badge } from "@/components/ui/badge";
import { Input } from "@/components/ui/input";
import { Skeleton } from "@/components/ui/skeleton";
import { Search } from "lucide-react";

type BepStatus =
  | "draft"
  | "proposed"
  | "accepted"
  | "implemented"
  | "rejected"
  | "superseded";

const STATUS_OPTIONS: { value: BepStatus | "all"; label: string }[] = [
  { value: "all", label: "All" },
  { value: "draft", label: "Draft" },
  { value: "proposed", label: "Proposed" },
  { value: "accepted", label: "Accepted" },
  { value: "implemented", label: "Implemented" },
  { value: "rejected", label: "Rejected" },
  { value: "superseded", label: "Superseded" },
];

export function BepList() {
  const [statusFilter, setStatusFilter] = useState<BepStatus | "all">("all");
  const [searchQuery, setSearchQuery] = useState("");

  const beps = useQuery(api.beps.list, {
    status: statusFilter === "all" ? undefined : statusFilter,
  });

  const filteredBeps = beps?.filter((bep) => {
    if (!searchQuery) return true;
    const query = searchQuery.toLowerCase();
    return (
      bep.title.toLowerCase().includes(query) ||
      `bep-${bep.number}`.includes(query) ||
      bep.shepherdNames.some((name) => name.toLowerCase().includes(query))
    );
  });

  return (
    <div className="space-y-6">
      {/* Filters */}
      <div className="flex flex-col sm:flex-row gap-4 items-start sm:items-center justify-between">
        <div className="flex flex-wrap gap-2">
          {STATUS_OPTIONS.map((option) => (
            <Badge
              key={option.value}
              variant={statusFilter === option.value ? "default" : "outline"}
              className="cursor-pointer"
              onClick={() => setStatusFilter(option.value)}
            >
              {option.label}
            </Badge>
          ))}
        </div>
        <div className="relative w-full sm:w-64">
          <Search className="absolute left-3 top-1/2 -translate-y-1/2 h-4 w-4 text-muted-foreground" />
          <Input
            placeholder="Search BEPs..."
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
            className="pl-9"
          />
        </div>
      </div>

      {/* List */}
      <div className="flex flex-col gap-1">
        {beps === undefined ? (
          // Loading state
          <>
            <Skeleton className="h-24 w-full" />
            <Skeleton className="h-24 w-full" />
            <Skeleton className="h-24 w-full" />
          </>
        ) : filteredBeps && filteredBeps.length > 0 ? (
          filteredBeps.map((bep) => (
            <div key={bep._id} className="mb-2">
              <BepCard
                number={bep.number}
                title={bep.title}
                status={bep.status}
                shepherdNames={bep.shepherdNames}
                commentCount={bep.commentCount}
                openIssueCount={bep.openIssueCount}
                updatedAt={bep.updatedAt}
              />
            </div>
          ))
        ) : (
          <div className="text-center py-12">
            <p className="text-muted-foreground">
              {searchQuery
                ? "No BEPs match your search."
                : "No BEPs yet. Create one to get started."}
            </p>
          </div>
        )}
      </div>
    </div>
  );
}
