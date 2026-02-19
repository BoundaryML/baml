"use client";

import { useState } from "react";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogFooter,
} from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Badge } from "@/components/ui/badge";
import { AlertTriangle, FileText, FilePlus, Trash2, Send } from "lucide-react";
import { useEditContext } from "./bep-edit-context";
import { cn } from "@/lib/utils";
import type { VersionMode } from "@/lib/types";

interface BepSubmitModalProps {
  open: boolean;
  onClose: () => void;
  onSubmit: (editNote: string, versionMode: VersionMode) => void;
  onDiscard: () => void;
  hasConflict: boolean;
  conflictVersion?: number;
}

export function BepSubmitModal({
  open,
  onClose,
  onSubmit,
  onDiscard,
  hasConflict,
  conflictVersion,
}: BepSubmitModalProps) {
  const { changes, hasChanges } = useEditContext();
  const [editNote, setEditNote] = useState("");
  const [versionMode, setVersionMode] = useState<VersionMode>("new");

  const changesList = Array.from(changes.values());

  const handleSubmit = () => {
    onSubmit(editNote, versionMode);
    setEditNote("");
    setVersionMode("new");
  };

  const handleDiscard = () => {
    if (confirm("Are you sure you want to discard all changes? This cannot be undone.")) {
      onDiscard();
      setEditNote("");
      setVersionMode("new");
    }
  };

  return (
    <Dialog open={open} onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="max-w-lg">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <Send className="h-5 w-5" />
            Ready to Submit?
          </DialogTitle>
          <DialogDescription className="text-left pt-2">
            Choose whether this should be a full new version or a direct update
            to the current one.
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-2">
          <Label id="version-mode-label">How should these edits be applied?</Label>
          <div className="grid gap-2 sm:grid-cols-2" role="group" aria-labelledby="version-mode-label">
            <button
              type="button"
              onClick={() => setVersionMode("new")}
              aria-pressed={versionMode === "new"}
              className={cn(
                "rounded-lg border p-3 text-left transition-colors",
                versionMode === "new"
                  ? "border-primary bg-primary/5"
                  : "hover:bg-muted/50"
              )}
            >
              <p className="font-medium text-sm">Create New Version</p>
              <p className="text-xs text-muted-foreground mt-1">
                Best for major feedback rounds. Starts a fresh comment cycle.
              </p>
            </button>
            <button
              type="button"
              onClick={() => setVersionMode("current")}
              aria-pressed={versionMode === "current"}
              className={cn(
                "rounded-lg border p-3 text-left transition-colors",
                versionMode === "current"
                  ? "border-primary bg-primary/5"
                  : "hover:bg-muted/50"
              )}
            >
              <p className="font-medium text-sm">Apply To Current Version</p>
              <p className="text-xs text-muted-foreground mt-1">
                Best for small corrections. Keeps existing comments visible.
              </p>
            </button>
          </div>
        </div>

        {/* Shepherd message */}
        <div className="bg-muted/50 rounded-lg p-4 text-sm space-y-2">
          {versionMode === "new" ? (
            <p>
              <strong>Comments</strong> from the current version will be preserved in history
              but won&apos;t appear on the new version.
            </p>
          ) : (
            <p>
              <strong>Comments</strong> stay attached to the current version, so active threads
              remain visible after your update.
            </p>
          )}
          <p className="text-muted-foreground">
            <strong>Issues</strong> and <strong>Decisions</strong> carry forward in either mode.
          </p>
        </div>

        {hasConflict && (
          <div className="flex items-start gap-2 p-3 bg-yellow-50 dark:bg-yellow-950/30 border border-yellow-200 dark:border-yellow-800 rounded-lg">
            <AlertTriangle className="h-5 w-5 text-yellow-600 flex-shrink-0 mt-0.5" />
            <div className="text-sm">
              <p className="font-medium text-yellow-800 dark:text-yellow-200">
                This document has been updated
              </p>
              <p className="text-yellow-700 dark:text-yellow-300">
                Someone else made changes (version {conflictVersion}) since you started editing.
                {versionMode === "new"
                  ? " Your changes will create a new version on top of theirs."
                  : " Your changes will be applied to the latest current version."}
              </p>
            </div>
          </div>
        )}

        {/* Changes summary */}
        {hasChanges && (
          <div className="space-y-2">
            <p className="text-sm font-medium">Changes to submit:</p>
            <div className="space-y-1 max-h-32 overflow-y-auto">
              {changesList.map((change, idx) => (
                <div key={idx} className="flex items-center gap-2 text-sm">
                  {change.status === "new" && <FilePlus className="h-4 w-4 text-green-500" />}
                  {change.status === "modified" && <FileText className="h-4 w-4 text-blue-500" />}
                  {change.status === "deleted" && <Trash2 className="h-4 w-4 text-red-500" />}
                  <span>{change.title}</span>
                  <Badge
                    variant="outline"
                    className={`text-xs ${change.status === "new"
                        ? "text-green-600"
                        : change.status === "deleted"
                          ? "text-red-600"
                          : "text-blue-600"
                      }`}
                  >
                    {change.status}
                  </Badge>
                </div>
              ))}
            </div>
          </div>
        )}

        {/* Edit note */}
        <div className="space-y-2">
          <Label htmlFor="edit-note">
            {versionMode === "new" ? "Version note (optional)" : "Change note (optional)"}
          </Label>
          <Input
            id="edit-note"
            value={editNote}
            onChange={(e) => setEditNote(e.target.value)}
            placeholder={
              versionMode === "new"
                ? "Briefly describe what changed in this version..."
                : "Briefly describe this correction..."
            }
          />
        </div>

        <DialogFooter className="flex-col sm:flex-row gap-2">
          <Button
            variant="ghost"
            onClick={handleDiscard}
            className="text-destructive hover:text-destructive hover:bg-destructive/10"
          >
            Discard
          </Button>
          <div className="flex-1" />
          <Button variant="outline" onClick={onClose}>
            Keep Editing
          </Button>
          <Button onClick={handleSubmit} disabled={!hasChanges}>
            {versionMode === "new" ? "Create Version" : "Apply Changes"}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
