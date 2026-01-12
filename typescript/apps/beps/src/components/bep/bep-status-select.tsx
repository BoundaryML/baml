"use client";

import { useState } from "react";
import { useMutation } from "convex/react";
import { api } from "../../../convex/_generated/api";
import { Id } from "../../../convex/_generated/dataModel";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Badge } from "@/components/ui/badge";

type BepStatus =
  | "draft"
  | "proposed"
  | "accepted"
  | "implemented"
  | "rejected"
  | "superseded";

const STATUS_OPTIONS: { value: BepStatus; label: string; description: string }[] = [
  { value: "draft", label: "Draft", description: "Work in progress" },
  { value: "proposed", label: "Proposed", description: "Ready for review" },
  { value: "accepted", label: "Accepted", description: "Approved for implementation" },
  { value: "implemented", label: "Implemented", description: "Fully implemented" },
  { value: "rejected", label: "Rejected", description: "Not accepted" },
  { value: "superseded", label: "Superseded", description: "Replaced by another BEP" },
];

interface BepStatusSelectProps {
  bepId: Id<"beps">;
  currentStatus: BepStatus;
  canEdit?: boolean;
}

export function BepStatusSelect({
  bepId,
  currentStatus,
  canEdit = true,
}: BepStatusSelectProps) {
  const [isUpdating, setIsUpdating] = useState(false);
  const updateStatus = useMutation(api.beps.updateStatus);

  const handleStatusChange = async (newStatus: BepStatus) => {
    if (newStatus === currentStatus) return;

    setIsUpdating(true);
    try {
      await updateStatus({ id: bepId, status: newStatus });
    } catch (error) {
      console.error("Failed to update status:", error);
    } finally {
      setIsUpdating(false);
    }
  };

  if (!canEdit) {
    return <Badge variant={currentStatus}>{currentStatus}</Badge>;
  }

  return (
    <Select
      value={currentStatus}
      onValueChange={(v) => handleStatusChange(v as BepStatus)}
      disabled={isUpdating}
    >
      <SelectTrigger className="w-40">
        <SelectValue>
          <Badge variant={currentStatus} className="capitalize">
            {currentStatus}
          </Badge>
        </SelectValue>
      </SelectTrigger>
      <SelectContent>
        {STATUS_OPTIONS.map((option) => (
          <SelectItem key={option.value} value={option.value}>
            <div className="flex items-center gap-2">
              <Badge variant={option.value} className="capitalize">
                {option.label}
              </Badge>
              <span className="text-xs text-muted-foreground">
                {option.description}
              </span>
            </div>
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  );
}
