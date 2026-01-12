"use client";

import { useState } from "react";
import { useMutation, useQuery } from "convex/react";
import { api } from "../../../convex/_generated/api";
import { Id } from "../../../convex/_generated/dataModel";
import { useUser } from "@/components/providers/user-provider";
import { CommentTypeBadge } from "./comment-type-badge";
import { CommentForm } from "./comment-form";
import { BepContent } from "@/components/bep/bep-content";
import { Button } from "@/components/ui/button";
import {
  ThumbsUp,
  ThumbsDown,
  Heart,
  HelpCircle,
  Reply,
  Check,
  RotateCcw,
  MoreHorizontal,
  Trash2,
  AlertCircle,
  Gavel,
} from "lucide-react";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { DecisionForm } from "@/components/decisions/decision-form";
import { IssueForm } from "@/components/issues/issue-form";
import { CommentLinkedBadges } from "./comment-linked-badges";

type CommentType =
  | "discussion"
  | "concern"
  | "question";

interface Comment {
  _id: Id<"comments">;
  bepId: Id<"beps">;
  versionId?: Id<"bepVersions">;
  authorId: Id<"users">;
  authorName: string;
  authorAvatarUrl?: string;
  parentId?: Id<"comments">;
  type: CommentType;
  content: string;
  reactions?: {
    thumbsUp?: Id<"users">[];
    thumbsDown?: Id<"users">[];
    heart?: Id<"users">[];
    thinking?: Id<"users">[];
  };
  resolved: boolean;
  resolvedBy?: Id<"users">;
  resolvedByName?: string;
  resolvedAt?: number;
  createdAt: number;
  updatedAt: number;
}

interface LinkedItem {
  _id: string;
  title: string;
  resolved?: boolean;
}

interface CommentItemProps {
  comment: Comment;
  versionId: Id<"bepVersions">;
  depth?: number;
  readOnly?: boolean;
  linkedItems?: {
    issues: LinkedItem[];
    decisions: LinkedItem[];
  };
  onNavigateToIssue?: (issueId: string) => void;
  onNavigateToDecision?: (decisionId: string) => void;
}

export function CommentItem({
  comment,
  versionId,
  depth = 0,
  readOnly = false,
  linkedItems,
  onNavigateToIssue,
  onNavigateToDecision,
}: CommentItemProps) {
  const { userId } = useUser();
  const [showReplyForm, setShowReplyForm] = useState(false);

  const replies = useQuery(api.comments.getReplies, { parentId: comment._id });
  const toggleReaction = useMutation(api.comments.toggleReaction);
  const resolveComment = useMutation(api.comments.resolve);
  const unresolveComment = useMutation(api.comments.unresolve);
  const deleteComment = useMutation(api.comments.remove);

  const formatTime = (timestamp: number) => {
    const date = new Date(timestamp);
    const now = new Date();
    const diffMs = now.getTime() - date.getTime();
    const diffMins = Math.floor(diffMs / 60000);
    const diffHours = Math.floor(diffMs / 3600000);
    const diffDays = Math.floor(diffMs / 86400000);

    if (diffMins < 1) return "just now";
    if (diffMins < 60) return `${diffMins}m ago`;
    if (diffHours < 24) return `${diffHours}h ago`;
    if (diffDays < 7) return `${diffDays}d ago`;
    return date.toLocaleDateString();
  };

  const handleReaction = async (
    emoji: "thumbsUp" | "thumbsDown" | "heart" | "thinking"
  ) => {
    if (!userId) return;
    await toggleReaction({ commentId: comment._id, userId, emoji });
  };

  const handleResolve = async () => {
    if (!userId) return;
    await resolveComment({ commentId: comment._id, userId });
  };

  const handleUnresolve = async () => {
    await unresolveComment({ commentId: comment._id });
  };

  const handleDelete = async () => {
    if (confirm("Are you sure you want to delete this comment?")) {
      await deleteComment({ id: comment._id });
    }
  };

  const getReactionCount = (emoji: keyof NonNullable<Comment["reactions"]>) => {
    return comment.reactions?.[emoji]?.length ?? 0;
  };

  const hasReacted = (emoji: keyof NonNullable<Comment["reactions"]>) => {
    return userId ? comment.reactions?.[emoji]?.includes(userId) ?? false : false;
  };

  const isAuthor = userId === comment.authorId;
  const maxDepth = 3;

  return (
    <div
      data-comment-id={comment._id}
      className={`${depth > 0 ? "ml-6 pl-4 border-l-2 border-muted" : ""} ${
        comment.resolved ? "opacity-60" : ""
      } transition-all duration-300`}
    >
      <div className="py-3">
        {/* Header */}
        <div className="flex items-center justify-between mb-2">
          <div className="flex items-center gap-2">
            <div className="w-8 h-8 rounded-full bg-primary/10 flex items-center justify-center text-sm font-medium">
              {comment.authorName.charAt(0).toUpperCase()}
            </div>
            <span className="font-medium">{comment.authorName}</span>
            <CommentTypeBadge type={comment.type} />
            <span className="text-xs text-muted-foreground">
              {formatTime(comment.createdAt)}
            </span>
            {comment.resolved && (
              <span className="text-xs text-green-600 flex items-center gap-1">
                <Check className="h-3 w-3" />
                Resolved
                {comment.resolvedByName && ` by ${comment.resolvedByName}`}
              </span>
            )}
          </div>

          {!readOnly && (
            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <Button variant="ghost" size="sm" className="h-8 w-8 p-0">
                  <MoreHorizontal className="h-4 w-4" />
                </Button>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="end">
                {!comment.resolved ? (
                  <DropdownMenuItem onClick={handleResolve}>
                    <Check className="h-4 w-4 mr-2" />
                    Mark resolved
                  </DropdownMenuItem>
                ) : (
                  <DropdownMenuItem onClick={handleUnresolve}>
                    <RotateCcw className="h-4 w-4 mr-2" />
                    Reopen
                  </DropdownMenuItem>
                )}
                {isAuthor && (
                  <DropdownMenuItem
                    onClick={handleDelete}
                    className="text-destructive"
                  >
                    <Trash2 className="h-4 w-4 mr-2" />
                    Delete
                  </DropdownMenuItem>
                )}
                <DropdownMenuSeparator />
                <DecisionForm
                  bepId={comment.bepId}
                  sourceCommentId={comment._id}
                  trigger={
                    <DropdownMenuItem onSelect={(e) => e.preventDefault()}>
                      <Gavel className="h-4 w-4 mr-2" />
                      Mark as Decision
                    </DropdownMenuItem>
                  }
                />
                <IssueForm
                  bepId={comment.bepId}
                  sourceCommentId={comment._id}
                  trigger={
                    <DropdownMenuItem onSelect={(e) => e.preventDefault()}>
                      <AlertCircle className="h-4 w-4 mr-2" />
                      Create Issue
                    </DropdownMenuItem>
                  }
                />
              </DropdownMenuContent>
            </DropdownMenu>
          )}
        </div>

        {/* Content */}
        <div className="prose-sm">
          <BepContent content={comment.content} />
        </div>

        {/* Linked Issues/Decisions */}
        {linkedItems && (linkedItems.issues.length > 0 || linkedItems.decisions.length > 0) && (
          <div className="mt-2">
            <CommentLinkedBadges
              issues={linkedItems.issues}
              decisions={linkedItems.decisions}
              onNavigateToIssue={onNavigateToIssue}
              onNavigateToDecision={onNavigateToDecision}
            />
          </div>
        )}

        {/* Actions */}
        <div className="flex items-center gap-1 mt-2">
          <Button
            variant="ghost"
            size="sm"
            className={`h-7 px-2 ${hasReacted("thumbsUp") ? "text-primary" : ""}`}
            onClick={() => handleReaction("thumbsUp")}
            disabled={readOnly}
          >
            <ThumbsUp className="h-3 w-3 mr-1" />
            {getReactionCount("thumbsUp") > 0 && getReactionCount("thumbsUp")}
          </Button>
          <Button
            variant="ghost"
            size="sm"
            className={`h-7 px-2 ${hasReacted("thumbsDown") ? "text-primary" : ""}`}
            onClick={() => handleReaction("thumbsDown")}
            disabled={readOnly}
          >
            <ThumbsDown className="h-3 w-3 mr-1" />
            {getReactionCount("thumbsDown") > 0 && getReactionCount("thumbsDown")}
          </Button>
          <Button
            variant="ghost"
            size="sm"
            className={`h-7 px-2 ${hasReacted("heart") ? "text-red-500" : ""}`}
            onClick={() => handleReaction("heart")}
            disabled={readOnly}
          >
            <Heart className="h-3 w-3 mr-1" />
            {getReactionCount("heart") > 0 && getReactionCount("heart")}
          </Button>
          <Button
            variant="ghost"
            size="sm"
            className={`h-7 px-2 ${hasReacted("thinking") ? "text-primary" : ""}`}
            onClick={() => handleReaction("thinking")}
            disabled={readOnly}
          >
            <HelpCircle className="h-3 w-3 mr-1" />
            {getReactionCount("thinking") > 0 && getReactionCount("thinking")}
          </Button>

          {depth < maxDepth && !readOnly && (
            <Button
              variant="ghost"
              size="sm"
              className="h-7 px-2"
              onClick={() => setShowReplyForm(!showReplyForm)}
            >
              <Reply className="h-3 w-3 mr-1" />
              Reply
            </Button>
          )}
        </div>

        {/* Reply form */}
        {showReplyForm && !readOnly && (
          <div className="mt-3">
            <CommentForm
              bepId={comment.bepId}
              versionId={versionId}
              parentId={comment._id}
              onSuccess={() => setShowReplyForm(false)}
              onCancel={() => setShowReplyForm(false)}
              compact
            />
          </div>
        )}
      </div>

      {/* Replies */}
      {replies && replies.length > 0 && (
        <div className="mt-1">
          {replies.map((reply) => (
            <CommentItem
              key={reply._id}
              comment={reply as Comment}
              versionId={versionId}
              depth={depth + 1}
              readOnly={readOnly}
            />
          ))}
        </div>
      )}
    </div>
  );
}
