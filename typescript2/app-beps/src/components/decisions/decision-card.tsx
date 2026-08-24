"use client";

import { useState } from "react";
import { useMutation } from "convex/react";
import { api } from "../../../convex/_generated/api";
import { Id, Doc } from "../../../convex/_generated/dataModel";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { Label } from "@/components/ui/label";
import { Badge } from "@/components/ui/badge";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import {
  Gavel,
  Users,
  Calendar,
  MoreHorizontal,
  Pencil,
  Trash2,
  MessageSquare,
  ExternalLink,
} from "lucide-react";
import { VersionBadge } from "@/components/comments/version-badge";

type SourceComment = Doc<"comments"> & { authorName: string; versionNumber?: number | null };

interface DecisionCardProps {
  decision: {
    _id: Id<"decisions">;
    title: string;
    description: string;
    rationale?: string;
    participantNames: string[];
    decidedAt: number;
    sourceComments?: SourceComment[];
  };
  currentVersionNumber?: number | null;
  onNavigateToComment?: (
    commentId: Id<"comments">,
    pageId?: Id<"bepPages"> | null,
    versionId?: Id<"bepVersions"> | null
  ) => void;
}

export function DecisionCard({ decision, currentVersionNumber, onNavigateToComment }: DecisionCardProps) {
  const [showEditDialog, setShowEditDialog] = useState(false);
  const [editTitle, setEditTitle] = useState(decision.title);
  const [editDescription, setEditDescription] = useState(decision.description);
  const [editRationale, setEditRationale] = useState(decision.rationale || "");
  const [isSubmitting, setIsSubmitting] = useState(false);

  const updateDecision = useMutation(api.decisions.update);
  const deleteDecision = useMutation(api.decisions.remove);

  const formatDate = (timestamp: number) => {
    return new Date(timestamp).toLocaleDateString("en-US", {
      month: "short",
      day: "numeric",
      year: "numeric",
    });
  };

  const handleEdit = async () => {
    if (!editTitle.trim() || !editDescription.trim()) return;
    setIsSubmitting(true);
    try {
      await updateDecision({
        id: decision._id,
        title: editTitle.trim(),
        description: editDescription.trim(),
        rationale: editRationale.trim() || undefined,
      });
      setShowEditDialog(false);
    } catch (error) {
      console.error("Failed to update decision:", error);
    } finally {
      setIsSubmitting(false);
    }
  };

  const handleDelete = async () => {
    if (confirm("Are you sure you want to delete this decision?")) {
      try {
        await deleteDecision({ id: decision._id });
      } catch (error) {
        console.error("Failed to delete decision:", error);
      }
    }
  };

  const handleCommentClick = (comment: SourceComment) => {
    if (onNavigateToComment) {
      onNavigateToComment(comment._id, comment.pageId, comment.versionId);
    }
  };

  return (
    <div
      data-decision-id={decision._id}
      className="p-4 border rounded-lg transition-all duration-300 bg-green-50/50 dark:bg-green-950/20 border-green-200 dark:border-green-900"
    >
      <div className="flex items-start gap-3">
        <div className="p-2 bg-green-100 dark:bg-green-900 rounded-lg">
          <Gavel className="h-4 w-4 text-green-700 dark:text-green-300" />
        </div>
        <div className="flex-1 min-w-0">
          <div className="flex items-center justify-between gap-2 mb-1">
            <div className="flex items-center gap-2">
              <h4 className="font-medium">{decision.title}</h4>
              <Badge variant="decision" className="text-xs">
                Decision
              </Badge>
            </div>
            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <Button variant="ghost" size="sm" className="h-8 w-8 p-0">
                  <MoreHorizontal className="h-4 w-4" />
                </Button>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="end">
                <Dialog open={showEditDialog} onOpenChange={setShowEditDialog}>
                  <DialogTrigger asChild>
                    <DropdownMenuItem
                      onSelect={(e) => {
                        e.preventDefault();
                        setEditTitle(decision.title);
                        setEditDescription(decision.description);
                        setEditRationale(decision.rationale || "");
                      }}
                    >
                      <Pencil className="h-4 w-4 mr-2" />
                      Edit
                    </DropdownMenuItem>
                  </DialogTrigger>
                  <DialogContent className="sm:max-w-lg">
                    <DialogHeader>
                      <DialogTitle>Edit Decision</DialogTitle>
                    </DialogHeader>
                    <div className="space-y-4">
                      <div className="space-y-2">
                        <Label htmlFor="edit-title">Title</Label>
                        <Input
                          id="edit-title"
                          value={editTitle}
                          onChange={(e) => setEditTitle(e.target.value)}
                          disabled={isSubmitting}
                        />
                      </div>
                      <div className="space-y-2">
                        <Label htmlFor="edit-description">Description</Label>
                        <Textarea
                          id="edit-description"
                          value={editDescription}
                          onChange={(e) => setEditDescription(e.target.value)}
                          disabled={isSubmitting}
                          rows={6}
                          className="min-h-[150px] resize-y"
                        />
                      </div>
                      <div className="space-y-2">
                        <Label htmlFor="edit-rationale">Rationale (optional)</Label>
                        <Textarea
                          id="edit-rationale"
                          value={editRationale}
                          onChange={(e) => setEditRationale(e.target.value)}
                          disabled={isSubmitting}
                          rows={4}
                          className="min-h-[100px] resize-y"
                        />
                      </div>
                      <div className="flex justify-end gap-2">
                        <Button
                          variant="outline"
                          onClick={() => setShowEditDialog(false)}
                          disabled={isSubmitting}
                        >
                          Cancel
                        </Button>
                        <Button
                          onClick={handleEdit}
                          disabled={!editTitle.trim() || !editDescription.trim() || isSubmitting}
                        >
                          {isSubmitting ? "Saving..." : "Save"}
                        </Button>
                      </div>
                    </div>
                  </DialogContent>
                </Dialog>
                <DropdownMenuItem
                  onClick={handleDelete}
                  className="text-destructive"
                >
                  <Trash2 className="h-4 w-4 mr-2" />
                  Delete
                </DropdownMenuItem>
              </DropdownMenuContent>
            </DropdownMenu>
          </div>
          <p className="text-sm text-muted-foreground mb-2 whitespace-pre-wrap">
            {decision.description}
          </p>
          {decision.rationale && (
            <p className="text-sm italic text-muted-foreground mb-2 whitespace-pre-wrap">
              Rationale: {decision.rationale}
            </p>
          )}

          {/* Source Comments */}
          {decision.sourceComments && decision.sourceComments.length > 0 && (
            <div className="mt-3 pt-3 border-t border-dashed border-green-300 dark:border-green-800">
              <div className="flex items-center gap-1 text-xs text-muted-foreground mb-2">
                <MessageSquare className="h-3 w-3" />
                Related Comments ({decision.sourceComments.length})
              </div>
              <div className="space-y-2">
                {decision.sourceComments.map((comment) => (
                  <button
                    key={comment._id}
                    onClick={() => handleCommentClick(comment)}
                    className="w-full text-left p-2 rounded border border-green-200 dark:border-green-800 bg-background/50 hover:bg-muted/50 transition-colors group"
                  >
                    <div className="flex items-start justify-between gap-2">
                      <div className="flex-1 min-w-0">
                        <div className="flex items-center gap-2 mb-1">
                          <p className="text-xs text-muted-foreground">
                            {comment.authorName} · {formatDate(comment.createdAt)}
                          </p>
                          <VersionBadge
                            versionNumber={comment.versionNumber ?? null}
                            currentVersionNumber={currentVersionNumber}
                          />
                        </div>
                        <p className="text-sm text-foreground/80 line-clamp-2 whitespace-pre-wrap">
                          {comment.content}
                        </p>
                      </div>
                      <ExternalLink className="h-3 w-3 text-muted-foreground opacity-0 group-hover:opacity-100 transition-opacity flex-shrink-0 mt-1" />
                    </div>
                  </button>
                ))}
              </div>
            </div>
          )}

          <div className="flex items-center gap-4 text-xs text-muted-foreground mt-2">
            <span className="flex items-center gap-1">
              <Users className="h-3 w-3" />
              {decision.participantNames.join(", ")}
            </span>
            <span className="flex items-center gap-1">
              <Calendar className="h-3 w-3" />
              {formatDate(decision.decidedAt)}
            </span>
          </div>
        </div>
      </div>
    </div>
  );
}
