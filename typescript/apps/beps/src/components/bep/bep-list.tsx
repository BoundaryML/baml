"use client";

import { useState, useCallback } from "react";
import { useQuery, useMutation } from "convex/react";
import {
  DndContext,
  DragEndEvent,
  DragOverEvent,
  DragOverlay,
  DragStartEvent,
  PointerSensor,
  useSensor,
  useSensors,
  closestCorners,
  useDroppable,
} from "@dnd-kit/core";
import {
  SortableContext,
  verticalListSortingStrategy,
} from "@dnd-kit/sortable";
import { api } from "../../../convex/_generated/api";
import { Id } from "../../../convex/_generated/dataModel";
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

interface KanbanColumnProps {
  status: BepStatus;
  label: string;
  color: string;
  beps: Array<{
    _id: Id<"beps">;
    number: number;
    title: string;
    status: BepStatus;
    shepherdNames: string[];
    commentCount: number;
    openIssueCount: number;
    updatedAt: number;
  }>;
  isOver?: boolean;
}

function KanbanColumn({ status, label, color, beps, isOver }: KanbanColumnProps) {
  const { setNodeRef } = useDroppable({
    id: status,
    data: {
      type: "column",
      status,
    },
  });

  return (
    <div
      ref={setNodeRef}
      className={`flex flex-col min-h-[200px] transition-colors ${
        isOver ? "bg-accent/30 rounded-lg" : ""
      }`}
    >
      <div className="flex items-center gap-2 mb-3 pb-2 border-b">
        <div className={`w-3 h-3 rounded-full ${color}`} />
        <h3 className="font-medium text-sm">{label}</h3>
        <span className="text-xs text-muted-foreground ml-auto">
          {beps.length}
        </span>
      </div>
      <SortableContext
        items={beps.map((bep) => bep._id)}
        strategy={verticalListSortingStrategy}
      >
        <div className="flex flex-col gap-2 flex-1 min-h-[100px]">
          {beps.length > 0 ? (
            beps.map((bep) => (
              <BepKanbanCard
                key={bep._id}
                id={bep._id}
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
            <div className="flex-1 flex items-center justify-center text-xs text-muted-foreground bg-muted/30 rounded-md min-h-[100px]">
              {isOver ? "Drop here" : "No BEPs"}
            </div>
          )}
        </div>
      </SortableContext>
    </div>
  );
}

export function BepList() {
  const [statusFilter, setStatusFilter] = useState<BepStatus | "all">("all");
  const [searchQuery, setSearchQuery] = useState("");
  const [showOldestFirst, setShowOldestFirst] = useState(false);
  const [viewMode, setViewMode] = useState<ViewMode>("kanban");
  const [activeId, setActiveId] = useState<Id<"beps"> | null>(null);
  const [overColumn, setOverColumn] = useState<BepStatus | null>(null);

  const beps = useQuery(api.beps.list, {
    status: viewMode === "kanban" ? undefined : statusFilter === "all" ? undefined : statusFilter,
  });

  const updateStatus = useMutation(api.beps.updateStatus);

  const sensors = useSensors(
    useSensor(PointerSensor, {
      activationConstraint: {
        distance: 8,
      },
    })
  );

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

  const getBepsByStatus = useCallback(
    (status: BepStatus) => {
      return filteredBeps?.filter((bep) => bep.status === status) || [];
    },
    [filteredBeps]
  );

  const activeBep = activeId
    ? filteredBeps?.find((bep) => bep._id === activeId)
    : null;

  const handleDragStart = useCallback((event: DragStartEvent) => {
    const { active } = event;
    setActiveId(active.id as Id<"beps">);
  }, []);

  const handleDragOver = useCallback((event: DragOverEvent) => {
    const { over } = event;
    if (!over) {
      setOverColumn(null);
      return;
    }

    const overData = over.data.current;
    if (overData?.type === "column") {
      setOverColumn(overData.status as BepStatus);
    } else if (overData?.type === "bep") {
      const overBep = filteredBeps?.find((bep) => bep._id === over.id);
      if (overBep) {
        setOverColumn(overBep.status);
      }
    }
  }, [filteredBeps]);

  const handleDragEnd = useCallback(
    async (event: DragEndEvent) => {
      const { active, over } = event;
      setActiveId(null);
      setOverColumn(null);

      if (!over) return;

      const activeBepId = active.id as Id<"beps">;
      const activeBepData = filteredBeps?.find((bep) => bep._id === activeBepId);
      if (!activeBepData) return;

      let targetStatus: BepStatus | null = null;

      const overData = over.data.current;
      if (overData?.type === "column") {
        targetStatus = overData.status as BepStatus;
      } else if (overData?.type === "bep") {
        const overBep = filteredBeps?.find((bep) => bep._id === over.id);
        if (overBep) {
          targetStatus = overBep.status;
        }
      }

      if (targetStatus && targetStatus !== activeBepData.status) {
        try {
          await updateStatus({
            id: activeBepId,
            status: targetStatus,
          });
        } catch (error) {
          console.error("Failed to update BEP status:", error);
        }
      }
    },
    [filteredBeps, updateStatus]
  );

  const handleDragCancel = useCallback(() => {
    setActiveId(null);
    setOverColumn(null);
  }, []);

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
        <DndContext
          sensors={sensors}
          collisionDetection={closestCorners}
          onDragStart={handleDragStart}
          onDragOver={handleDragOver}
          onDragEnd={handleDragEnd}
          onDragCancel={handleDragCancel}
        >
          <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 xl:grid-cols-6 gap-4">
            {KANBAN_COLUMNS.map((column) => (
              <KanbanColumn
                key={column.status}
                status={column.status}
                label={column.label}
                color={column.color}
                beps={getBepsByStatus(column.status)}
                isOver={overColumn === column.status}
              />
            ))}
          </div>
          <DragOverlay>
            {activeBep ? (
              <div className="opacity-90">
                <BepKanbanCard
                  id={activeBep._id}
                  number={activeBep.number}
                  title={activeBep.title}
                  status={activeBep.status}
                  shepherdNames={activeBep.shepherdNames}
                  commentCount={activeBep.commentCount}
                  openIssueCount={activeBep.openIssueCount}
                  updatedAt={activeBep.updatedAt}
                  isDragging
                />
              </div>
            ) : null}
          </DragOverlay>
        </DndContext>
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
