"use client";

import { useState } from "react";
import { useQuery } from "convex/react";
import { api } from "../../../convex/_generated/api";
import { BepCard } from "./bep-card";
import { BepKanbanCard } from "./bep-kanban-card";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Skeleton } from "@/components/ui/skeleton";
import { ArrowUpDown, Search, LayoutList, Columns3 } from "lucide-react";

type BepStatus =
  | "draft"
  | "proposed"
  | "accepted"
  | "implemented"
  | "rejected"
  | "superseded";

type ViewMode = "list" | "kanban";

const STATUS_OPTIONS: { value: BepStatus | "all"; label: string }[] = [
  { value: "all", label: "All" },
  { value: "draft", label: "Draft" },
  { value: "proposed", label: "Proposed" },
  { value: "accepted", label: "Accepted" },
  { value: "implemented", label: "Implemented" },
  { value: "rejected", label: "Rejected" },
  { value: "superseded", label: "Superseded" },
];

const KANBAN_COLUMNS: { status: BepStatus; label: string; color: string }[] = [
  { status: "draft", label: "Draft", color: "bg-slate-500" },
  { status: "proposed", label: "Proposed", color: "bg-blue-500" },
  { status: "accepted", label: "Accepted", color: "bg-green-500" },
  { status: "implemented", label: "Implemented", color: "bg-purple-500" },
  { status: "rejected", label: "Rejected", color: "bg-red-500" },
  { status: "superseded", label: "Superseded", color: "bg-orange-500" },
];

export function BepList() {
  const [statusFilter, setStatusFilter] = useState<BepStatus | "all">("all");
  const [searchQuery, setSearchQuery] = useState("");
  const [showOldestFirst, setShowOldestFirst] = useState(false);
  const [viewMode, setViewMode] = useState<ViewMode>("kanban");

  const beps = useQuery(api.beps.list, {
    status: viewMode === "kanban" ? undefined : statusFilter === "all" ? undefined : statusFilter,
  });

  const filteredBeps = beps
    ?.filter((bep) => {
      if (!searchQuery) return true;
      const query = searchQuery.toLowerCase();
      return (
        bep.title.toLowerCase().includes(query) ||
        `bep-${bep.number}`.includes(query) ||
        bep.shepherdNames.some((name) => name.toLowerCase().includes(query))
      );
    })
    .sort((a, b) =>
      showOldestFirst ? a.number - b.number : b.number - a.number
    );

  const getBepsByStatus = (status: BepStatus) => {
    return filteredBeps?.filter((bep) => bep.status === status) || [];
  };

  return (
    <div className="space-y-6">
      {/* Filters */}
      <div className="flex flex-col gap-4">
        <div className="flex flex-col sm:flex-row gap-4 items-start sm:items-center justify-between">
          {viewMode === "list" && (
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
          )}
          <div className={`flex w-full ${viewMode === "kanban" ? "" : "sm:w-auto"} items-center gap-2`}>
            <div className="relative flex-1 sm:flex-none sm:w-64">
              <Search className="absolute left-3 top-1/2 -translate-y-1/2 h-4 w-4 text-muted-foreground" />
              <Input
                placeholder="Search BEPs..."
                value={searchQuery}
                onChange={(e) => setSearchQuery(e.target.value)}
                className="pl-9"
              />
            </div>
            {viewMode === "list" && (
              <Button
                type="button"
                variant="outline"
                size="sm"
                onClick={() => setShowOldestFirst((prev) => !prev)}
                className="shrink-0"
              >
                <ArrowUpDown className="h-4 w-4" />
                {showOldestFirst ? "Newest first" : "Oldest first"}
              </Button>
            )}
            <div className="flex border rounded-md">
              <Button
                type="button"
                variant={viewMode === "list" ? "default" : "ghost"}
                size="sm"
                onClick={() => setViewMode("list")}
                className="rounded-r-none"
              >
                <LayoutList className="h-4 w-4" />
              </Button>
              <Button
                type="button"
                variant={viewMode === "kanban" ? "default" : "ghost"}
                size="sm"
                onClick={() => setViewMode("kanban")}
                className="rounded-l-none"
              >
                <Columns3 className="h-4 w-4" />
              </Button>
            </div>
          </div>
        </div>
      </div>

      {/* Content */}
      {beps === undefined ? (
        <div className="flex flex-col gap-1">
          <Skeleton className="h-24 w-full" />
          <Skeleton className="h-24 w-full" />
          <Skeleton className="h-24 w-full" />
        </div>
      ) : viewMode === "kanban" ? (
        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 xl:grid-cols-6 gap-4">
          {KANBAN_COLUMNS.map((column) => {
            const columnBeps = getBepsByStatus(column.status);
            return (
              <div key={column.status} className="flex flex-col min-h-[200px]">
                <div className="flex items-center gap-2 mb-3 pb-2 border-b">
                  <div className={`w-3 h-3 rounded-full ${column.color}`} />
                  <h3 className="font-medium text-sm">{column.label}</h3>
                  <span className="text-xs text-muted-foreground ml-auto">
                    {columnBeps.length}
                  </span>
                </div>
                <div className="flex flex-col gap-2 flex-1">
                  {columnBeps.length > 0 ? (
                    columnBeps.map((bep) => (
                      <BepKanbanCard
                        key={bep._id}
                        number={bep.number}
                        title={bep.title}
                        status={bep.status}
                        shepherdNames={bep.shepherdNames}
                        commentCount={bep.commentCount}
                        openIssueCount={bep.openIssueCount}
                        updatedAt={bep.updatedAt}
                      />
                    ))
                  ) : (
                    <div className="flex-1 flex items-center justify-center text-xs text-muted-foreground bg-muted/30 rounded-md">
                      No BEPs
                    </div>
                  )}
                </div>
              </div>
            );
          })}
        </div>
      ) : filteredBeps && filteredBeps.length > 0 ? (
        <div className="flex flex-col gap-1">
          {filteredBeps.map((bep) => (
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
          ))}
        </div>
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
  );
}
