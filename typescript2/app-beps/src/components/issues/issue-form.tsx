"use client";

import { useState } from "react";
import { useMutation, useQuery } from "convex/react";
import { api } from "../../../convex/_generated/api";
import { Id } from "../../../convex/_generated/dataModel";
import { useUser } from "@/components/providers/user-provider";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { Label } from "@/components/ui/label";
import { Checkbox } from "@/components/ui/checkbox";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/collapsible";
import { AlertCircle, ChevronDown, MessageSquare } from "lucide-react";

interface IssueFormProps {
  bepId: Id<"beps">;
  sourceCommentId?: Id<"comments">;
  onSuccess?: () => void;
  trigger?: React.ReactNode;
}

export function IssueForm({
  bepId,
  sourceCommentId,
  onSuccess,
  trigger,
}: IssueFormProps) {
  const { userId } = useUser();
  const [open, setOpen] = useState(false);
  const [title, setTitle] = useState("");
  const [description, setDescription] = useState("");
  const [selectedCommentIds, setSelectedCommentIds] = useState<Id<"comments">[]>([]);
  const [showComments, setShowComments] = useState(false);
  const [isSubmitting, setIsSubmitting] = useState(false);

  const comments = useQuery(api.comments.byBep, open ? { bepId } : "skip");
  const createIssue = useMutation(api.issues.create);
  const createFromComment = useMutation(api.issues.createFromComment);
  const attachComment = useMutation(api.issues.attachComment);

  // Filter out the source comment and only show unresolved top-level comments
  const availableComments = comments?.filter(
    (c) => c._id !== sourceCommentId && !c.parentId && !c.resolved
  ) ?? [];

  const toggleComment = (commentId: Id<"comments">) => {
    setSelectedCommentIds((prev) =>
      prev.includes(commentId)
        ? prev.filter((id) => id !== commentId)
        : [...prev, commentId]
    );
  };

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!title.trim() || !userId) return;

    setIsSubmitting(true);
    try {
      let issueId: Id<"openIssues">;

      if (sourceCommentId) {
        issueId = await createFromComment({
          commentId: sourceCommentId,
          title: title.trim(),
          userId,
        });
      } else {
        issueId = await createIssue({
          bepId,
          title: title.trim(),
          description: description.trim() || undefined,
          raisedBy: userId,
        });
      }

      // Attach selected comments
      for (const commentId of selectedCommentIds) {
        await attachComment({ id: issueId, commentId });
      }

      setOpen(false);
      setTitle("");
      setDescription("");
      setSelectedCommentIds([]);
      setShowComments(false);
      onSuccess?.();
    } catch (error) {
      console.error("Failed to create issue:", error);
    } finally {
      setIsSubmitting(false);
    }
  };

  const formatDate = (timestamp: number) => {
    return new Date(timestamp).toLocaleDateString("en-US", {
      month: "short",
      day: "numeric",
    });
  };

  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogTrigger asChild>
        {trigger || (
          <Button variant="outline" size="sm">
            <AlertCircle className="h-4 w-4 mr-2" />
            Raise Issue
          </Button>
        )}
      </DialogTrigger>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>Raise Issue</DialogTitle>
        </DialogHeader>
        <form onSubmit={handleSubmit} className="space-y-4">
          <div className="space-y-2">
            <Label htmlFor="title">Issue Title *</Label>
            <Input
              id="title"
              placeholder="e.g., Unclear error handling requirements"
              value={title}
              onChange={(e) => setTitle(e.target.value)}
              disabled={isSubmitting}
            />
          </div>

          {!sourceCommentId && (
            <div className="space-y-2">
              <Label htmlFor="description">Description (optional)</Label>
              <Textarea
                id="description"
                placeholder="More details about the issue..."
                value={description}
                onChange={(e) => setDescription(e.target.value)}
                disabled={isSubmitting}
                rows={6}
                className="min-h-[150px] resize-y"
              />
            </div>
          )}

          {/* Attach Comments Section */}
          {availableComments.length > 0 && (
            <Collapsible open={showComments} onOpenChange={setShowComments}>
              <CollapsibleTrigger asChild>
                <Button
                  type="button"
                  variant="ghost"
                  size="sm"
                  className="w-full justify-between"
                >
                  <span className="flex items-center gap-2">
                    <MessageSquare className="h-4 w-4" />
                    Attach Related Comments
                    {selectedCommentIds.length > 0 && (
                      <span className="text-xs bg-primary/10 px-2 py-0.5 rounded">
                        {selectedCommentIds.length} selected
                      </span>
                    )}
                  </span>
                  <ChevronDown
                    className={`h-4 w-4 transition-transform ${showComments ? "rotate-180" : ""}`}
                  />
                </Button>
              </CollapsibleTrigger>
              <CollapsibleContent className="space-y-2 pt-2">
                <div className="max-h-[200px] overflow-y-auto space-y-2 border rounded-md p-2">
                  {availableComments.map((comment) => (
                    <label
                      key={comment._id}
                      className="flex items-start gap-2 p-2 rounded hover:bg-muted/50 cursor-pointer"
                    >
                      <Checkbox
                        checked={selectedCommentIds.includes(comment._id)}
                        onCheckedChange={() => toggleComment(comment._id)}
                        disabled={isSubmitting}
                      />
                      <div className="flex-1 min-w-0">
                        <p className="text-xs text-muted-foreground">
                          {comment.authorName} · {formatDate(comment.createdAt)}
                        </p>
                        <p className="text-sm line-clamp-2">{comment.content}</p>
                      </div>
                    </label>
                  ))}
                </div>
              </CollapsibleContent>
            </Collapsible>
          )}

          <div className="flex justify-end gap-2">
            <Button
              type="button"
              variant="outline"
              onClick={() => setOpen(false)}
              disabled={isSubmitting}
            >
              Cancel
            </Button>
            <Button type="submit" disabled={!title.trim() || isSubmitting}>
              {isSubmitting ? "Creating..." : "Raise Issue"}
            </Button>
          </div>
        </form>
      </DialogContent>
    </Dialog>
  );
}
