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
  AlertCircle,
  CheckCircle,
  User,
  Calendar,
  MoreHorizontal,
  RotateCcw,
  Trash2,
  Pencil,
  MessageSquare,
  ExternalLink,
} from "lucide-react";
import { VersionBadge } from "@/components/comments/version-badge";

type RelatedComment = Doc<"comments"> & { authorName: string; versionNumber?: number | null };

interface IssueCardProps {
  issue: {
    _id: Id<"openIssues">;
    title: string;
    description?: string;
    raisedByName: string;
    assignedToName?: string;
    resolved: boolean;
    resolution?: string;
    resolvedAt?: number;
    createdAt: number;
    relatedComments?: RelatedComment[];
  };
  currentVersionNumber?: number | null;
  onNavigateToComment?: (
    commentId: Id<"comments">,
    pageId?: Id<"bepPages"> | null,
    versionId?: Id<"bepVersions"> | null
  ) => void;
}

export function IssueCard({ issue, currentVersionNumber, onNavigateToComment }: IssueCardProps) {
  const [showResolveDialog, setShowResolveDialog] = useState(false);
  const [showEditDialog, setShowEditDialog] = useState(false);
  const [resolution, setResolution] = useState("");
  const [editTitle, setEditTitle] = useState(issue.title);
  const [editDescription, setEditDescription] = useState(issue.description || "");
  const [isSubmitting, setIsSubmitting] = useState(false);

  const resolveIssue = useMutation(api.issues.resolve);
  const reopenIssue = useMutation(api.issues.reopen);
  const deleteIssue = useMutation(api.issues.remove);
  const updateIssue = useMutation(api.issues.update);

  const formatDate = (timestamp: number) => {
    return new Date(timestamp).toLocaleDateString("en-US", {
      month: "short",
      day: "numeric",
      year: "numeric",
    });
  };

  const handleResolve = async () => {
    if (!resolution.trim()) return;
    setIsSubmitting(true);
    try {
      await resolveIssue({ id: issue._id, resolution: resolution.trim() });
      setShowResolveDialog(false);
      setResolution("");
    } catch (error) {
      console.error("Failed to resolve issue:", error);
    } finally {
      setIsSubmitting(false);
    }
  };

  const handleReopen = async () => {
    try {
      await reopenIssue({ id: issue._id });
    } catch (error) {
      console.error("Failed to reopen issue:", error);
    }
  };

  const handleDelete = async () => {
    if (confirm("Are you sure you want to delete this issue?")) {
      try {
        await deleteIssue({ id: issue._id });
      } catch (error) {
        console.error("Failed to delete issue:", error);
      }
    }
  };

  const handleEdit = async () => {
    if (!editTitle.trim()) return;
    setIsSubmitting(true);
    try {
      await updateIssue({
        id: issue._id,
        title: editTitle.trim(),
        description: editDescription.trim() || undefined,
      });
      setShowEditDialog(false);
    } catch (error) {
      console.error("Failed to update issue:", error);
    } finally {
      setIsSubmitting(false);
    }
  };

  const handleCommentClick = (comment: RelatedComment) => {
    if (onNavigateToComment) {
      onNavigateToComment(comment._id, comment.pageId, comment.versionId);
    }
  };

  return (
    <div
      data-issue-id={issue._id}
      className={`p-4 border rounded-lg transition-all duration-300 ${
        issue.resolved
          ? "bg-muted/30 border-muted"
          : "bg-yellow-50/50 dark:bg-yellow-950/20 border-yellow-200 dark:border-yellow-900"
      }`}
    >
      <div className="flex items-start gap-3">
        <div
          className={`p-2 rounded-lg ${
            issue.resolved
              ? "bg-muted"
              : "bg-yellow-100 dark:bg-yellow-900"
          }`}
        >
          {issue.resolved ? (
            <CheckCircle className="h-4 w-4 text-green-600" />
          ) : (
            <AlertCircle className="h-4 w-4 text-yellow-700 dark:text-yellow-300" />
          )}
        </div>
        <div className="flex-1 min-w-0">
          <div className="flex items-center justify-between gap-2 mb-1">
            <div className="flex items-center gap-2">
              <h4
                className={`font-medium ${issue.resolved ? "line-through text-muted-foreground" : ""}`}
              >
                {issue.title}
              </h4>
              {issue.resolved ? (
                <Badge variant="secondary" className="text-xs">
                  Resolved
                </Badge>
              ) : (
                <Badge variant="concern" className="text-xs">
                  Open
                </Badge>
              )}
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
                        setEditTitle(issue.title);
                        setEditDescription(issue.description || "");
                      }}
                    >
                      <Pencil className="h-4 w-4 mr-2" />
                      Edit
                    </DropdownMenuItem>
                  </DialogTrigger>
                  <DialogContent className="sm:max-w-lg">
                    <DialogHeader>
                      <DialogTitle>Edit Issue</DialogTitle>
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
                          disabled={!editTitle.trim() || isSubmitting}
                        >
                          {isSubmitting ? "Saving..." : "Save"}
                        </Button>
                      </div>
                    </div>
                  </DialogContent>
                </Dialog>
                {issue.resolved ? (
                  <DropdownMenuItem onClick={handleReopen}>
                    <RotateCcw className="h-4 w-4 mr-2" />
                    Reopen
                  </DropdownMenuItem>
                ) : (
                  <Dialog
                    open={showResolveDialog}
                    onOpenChange={setShowResolveDialog}
                  >
                    <DialogTrigger asChild>
                      <DropdownMenuItem onSelect={(e) => e.preventDefault()}>
                        <CheckCircle className="h-4 w-4 mr-2" />
                        Resolve
                      </DropdownMenuItem>
                    </DialogTrigger>
                    <DialogContent>
                      <DialogHeader>
                        <DialogTitle>Resolve Issue</DialogTitle>
                      </DialogHeader>
                      <div className="space-y-4">
                        <p className="text-sm text-muted-foreground">
                          How was this issue resolved?
                        </p>
                        <Input
                          placeholder="Resolution description..."
                          value={resolution}
                          onChange={(e) => setResolution(e.target.value)}
                          disabled={isSubmitting}
                        />
                        <div className="flex justify-end gap-2">
                          <Button
                            variant="outline"
                            onClick={() => setShowResolveDialog(false)}
                            disabled={isSubmitting}
                          >
                            Cancel
                          </Button>
                          <Button
                            onClick={handleResolve}
                            disabled={!resolution.trim() || isSubmitting}
                          >
                            {isSubmitting ? "Resolving..." : "Resolve"}
                          </Button>
                        </div>
                      </div>
                    </DialogContent>
                  </Dialog>
                )}
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

          {issue.description && (
            <p className="text-sm text-muted-foreground mb-2 whitespace-pre-wrap">
              {issue.description}
            </p>
          )}

          {issue.resolved && issue.resolution && (
            <p className="text-sm text-green-700 dark:text-green-400 mb-2 whitespace-pre-wrap">
              Resolution: {issue.resolution}
            </p>
          )}

          {/* Related Comments */}
          {issue.relatedComments && issue.relatedComments.length > 0 && (
            <div className="mt-3 pt-3 border-t border-dashed">
              <div className="flex items-center gap-1 text-xs text-muted-foreground mb-2">
                <MessageSquare className="h-3 w-3" />
                Related Comments ({issue.relatedComments.length})
              </div>
              <div className="space-y-2">
                {issue.relatedComments.map((comment) => (
                  <button
                    key={comment._id}
                    onClick={() => handleCommentClick(comment)}
                    className="w-full text-left p-2 rounded border bg-background/50 hover:bg-muted/50 transition-colors group"
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
              <User className="h-3 w-3" />
              Raised by {issue.raisedByName}
            </span>
            {issue.assignedToName && (
              <span className="flex items-center gap-1">
                Assigned to {issue.assignedToName}
              </span>
            )}
            <span className="flex items-center gap-1">
              <Calendar className="h-3 w-3" />
              {formatDate(issue.createdAt)}
            </span>
          </div>
        </div>
      </div>
    </div>
  );
}
