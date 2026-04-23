"use client";

import { useState } from "react";
import { useMutation, useQuery } from "convex/react";
import { api } from "../../../convex/_generated/api";
import { Id } from "../../../convex/_generated/dataModel";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogFooter,
} from "@/components/ui/dialog";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Label } from "@/components/ui/label";
import { ScrollArea } from "@/components/ui/scroll-area";

type BepStatus =
  | "draft"
  | "proposed"
  | "pending"
  | "accepted"
  | "implemented"
  | "rejected"
  | "superseded";

const STATUS_OPTIONS: { value: BepStatus; label: string; description: string }[] = [
  { value: "draft", label: "Draft", description: "Work in progress" },
  { value: "proposed", label: "Proposed", description: "Ready for review" },
  { value: "pending", label: "Pending", description: "Under consideration" },
  { value: "accepted", label: "Accepted", description: "Approved for implementation" },
  { value: "implemented", label: "Implemented", description: "Fully implemented" },
  { value: "rejected", label: "Rejected", description: "Not accepted" },
  { value: "superseded", label: "Superseded", description: "Replaced by another BEP" },
];

interface BepStatusSelectProps {
  bepId: Id<"beps">;
  currentStatus: BepStatus;
  canEdit?: boolean;
  existingImplementers?: Id<"users">[];
}

export function BepStatusSelect({
  bepId,
  currentStatus,
  canEdit = true,
  existingImplementers,
}: BepStatusSelectProps) {
  const [isUpdating, setIsUpdating] = useState(false);
  const [showImplementersDialog, setShowImplementersDialog] = useState(false);
  const [selectedImplementers, setSelectedImplementers] = useState<Id<"users">[]>(
    existingImplementers ?? []
  );
  const updateStatus = useMutation(api.beps.updateStatus);
  const users = useQuery(api.users.list);

  const handleStatusChange = async (newStatus: BepStatus) => {
    if (newStatus === currentStatus) return;

    if (newStatus === "implemented") {
      setSelectedImplementers(existingImplementers ?? []);
      setShowImplementersDialog(true);
      return;
    }

    setIsUpdating(true);
    try {
      await updateStatus({ id: bepId, status: newStatus });
    } catch (error) {
      console.error("Failed to update status:", error);
    } finally {
      setIsUpdating(false);
    }
  };

  const handleConfirmImplemented = async () => {
    setIsUpdating(true);
    try {
      await updateStatus({
        id: bepId,
        status: "implemented",
        implementedBy: selectedImplementers.length > 0 ? selectedImplementers : undefined,
      });
      setShowImplementersDialog(false);
    } catch (error) {
      console.error("Failed to update status:", error);
    } finally {
      setIsUpdating(false);
    }
  };

  const toggleImplementer = (userId: Id<"users">) => {
    setSelectedImplementers((prev) =>
      prev.includes(userId)
        ? prev.filter((id) => id !== userId)
        : [...prev, userId]
    );
  };

  if (!canEdit) {
    return <Badge variant={currentStatus}>{currentStatus}</Badge>;
  }

  return (
    <>
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

      <Dialog open={showImplementersDialog} onOpenChange={setShowImplementersDialog}>
        <DialogContent className="sm:max-w-md">
          <DialogHeader>
            <DialogTitle>Mark as Implemented</DialogTitle>
            <DialogDescription>
              Optionally select who implemented this BEP. You can skip this step.
            </DialogDescription>
          </DialogHeader>

          <div className="py-4">
            <Label className="text-sm font-medium mb-3 block">
              Implemented by (optional)
            </Label>
            <ScrollArea className="h-60 rounded-md border p-3">
              <div className="space-y-2">
                {users?.map((user) => (
                  <div key={user._id} className="flex items-center space-x-2">
                    <Checkbox
                      id={user._id}
                      checked={selectedImplementers.includes(user._id)}
                      onCheckedChange={() => toggleImplementer(user._id)}
                    />
                    <label
                      htmlFor={user._id}
                      className="flex items-center gap-2 text-sm font-medium leading-none peer-disabled:cursor-not-allowed peer-disabled:opacity-70 cursor-pointer"
                    >
                      {user.avatarUrl && (
                        <img
                          src={user.avatarUrl}
                          alt={user.name}
                          className="h-5 w-5 rounded-full"
                        />
                      )}
                      {user.name}
                    </label>
                  </div>
                ))}
              </div>
            </ScrollArea>
            {selectedImplementers.length > 0 && (
              <p className="text-xs text-muted-foreground mt-2">
                {selectedImplementers.length} selected
              </p>
            )}
          </div>

          <DialogFooter className="gap-2 sm:gap-0">
            <Button
              variant="outline"
              onClick={() => setShowImplementersDialog(false)}
              disabled={isUpdating}
            >
              Cancel
            </Button>
            <Button onClick={handleConfirmImplemented} disabled={isUpdating}>
              {isUpdating ? "Saving..." : "Mark as Implemented"}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  );
}
