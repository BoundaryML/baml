"use client";

import { useState, useRef, useEffect, useCallback } from "react";
import { useQuery, useMutation } from "convex/react";
import { api } from "../../../convex/_generated/api";
import { Id } from "../../../convex/_generated/dataModel";
import { useUser } from "@/components/providers/user-provider";
import { Button } from "@/components/ui/button";
import { Textarea } from "@/components/ui/textarea";
import { Skeleton } from "@/components/ui/skeleton";
import { BepContent } from "@/components/bep/bep-content";
import { MDXEditorComponent, MDXEditorHandle } from "@/components/editor/mdx";
import { DecisionForm } from "@/components/decisions/decision-form";
import { IssueForm } from "@/components/issues/issue-form";
import { CommentLinkedBadges } from "./comment-linked-badges";
import { BepLinkContext } from "@/lib/bep-link-resolver";
import { 
  MessageSquare, 
  ChevronDown, 
  ChevronUp,
  Check, 
  RotateCcw,
  ThumbsUp,
  ThumbsDown,
  Heart,
  Reply,
  AlertCircle,
  HelpCircle,
  MoreHorizontal,
  Trash2,
  Gavel,
} from "lucide-react";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { cn } from "@/lib/utils";

interface CommentThreadProps {
  bepId: Id<"beps">;
  versionId: Id<"bepVersions">;
  pageId?: Id<"bepPages">;
  viewingVersionId?: Id<"bepVersions">;
  readOnly?: boolean;
  linkContext?: BepLinkContext;
  onNavigateToIssue?: (issueId: string) => void;
  onNavigateToDecision?: (decisionId: string) => void;
}

interface Comment {
  _id: Id<"comments">;
  bepId: Id<"beps">;
  authorId: Id<"users">;
  authorName: string;
  parentId?: Id<"comments">;
  type: string;
  content: string;
  reactions?: {
    thumbsUp?: Id<"users">[];
    thumbsDown?: Id<"users">[];
    heart?: Id<"users">[];
    thinking?: Id<"users">[];
  };
  resolved: boolean;
  resolvedByName?: string;
  createdAt: number;
}

interface LinkedItem {
  _id: string;
  title: string;
  resolved?: boolean;
}

function Avatar({ name, size = "md" }: { name: string; size?: "sm" | "md" }) {
  const initial = name[0]?.toUpperCase() || "?";
  const colors = [
    'bg-blue-500', 'bg-green-500', 'bg-purple-500', 'bg-pink-500', 
    'bg-indigo-500', 'bg-teal-500', 'bg-orange-500', 'bg-cyan-500'
  ];
  const colorIndex = name.split('').reduce((acc, char) => acc + char.charCodeAt(0), 0) % colors.length;
  const sizeClass = size === "sm" ? "w-7 h-7 text-xs" : "w-9 h-9 text-sm";
  
  return (
    <div className={cn("rounded-full flex items-center justify-center text-white font-medium shrink-0", colors[colorIndex], sizeClass)}>
      {initial}
    </div>
  );
}

function TypeBadge({ type }: { type: string }) {
  if (type === "concern") {
    return (
      <span className="inline-flex items-center gap-1 text-xs text-amber-600 bg-amber-50 px-1.5 py-0.5 rounded">
        <AlertCircle className="h-3 w-3" /> Concern
      </span>
    );
  }
  if (type === "question") {
    return (
      <span className="inline-flex items-center gap-1 text-xs text-blue-600 bg-blue-50 px-1.5 py-0.5 rounded">
        <HelpCircle className="h-3 w-3" /> Question
      </span>
    );
  }
  return null;
}

function formatTime(timestamp: number) {
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
  return date.toLocaleDateString("en-US", { month: "short", day: "numeric" });
}

function SingleComment({ 
  comment, 
  versionId,
  depth = 0,
  readOnly,
  linkedItems,
  linkContext,
  onNavigateToIssue,
  onNavigateToDecision,
}: { 
  comment: Comment; 
  versionId: Id<"bepVersions">;
  depth?: number;
  readOnly?: boolean;
  linkedItems?: { issues: LinkedItem[]; decisions: LinkedItem[] };
  linkContext?: BepLinkContext;
  onNavigateToIssue?: (issueId: string) => void;
  onNavigateToDecision?: (decisionId: string) => void;
}) {
  const { userId, user } = useUser();
  const toggleReaction = useMutation(api.comments.toggleReaction);
  const resolveComment = useMutation(api.comments.resolve);
  const unresolveComment = useMutation(api.comments.unresolve);
  const deleteComment = useMutation(api.comments.remove);

  const replies = useQuery(api.comments.getReplies, { parentId: comment._id });
  const [showReplies, setShowReplies] = useState(true);
  const [showReplyForm, setShowReplyForm] = useState(false);
  const [replyContent, setReplyContent] = useState("");
  const replyEditorRef = useRef<MDXEditorHandle>(null);
  const addComment = useMutation(api.comments.add);

  const getReactionCount = (emoji: "thumbsUp" | "thumbsDown" | "heart" | "thinking") => 
    comment.reactions?.[emoji]?.length ?? 0;
  const hasReacted = (emoji: "thumbsUp" | "thumbsDown" | "heart" | "thinking") => 
    userId ? comment.reactions?.[emoji]?.includes(userId) ?? false : false;
  const isAuthor = userId === comment.authorId;

  const handleReaction = async (emoji: "thumbsUp" | "thumbsDown" | "heart" | "thinking") => {
    if (!userId || readOnly) return;
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
    if (confirm("Delete this comment?")) {
      await deleteComment({ id: comment._id });
    }
  };

  const replyContainerRef = useRef<HTMLDivElement>(null);

  const handleSubmitReply = useCallback(async () => {
    const content = replyEditorRef.current?.getMarkdown() || replyContent;
    if (!userId || !content.trim()) return;
    try {
      await addComment({
        bepId: comment.bepId,
        versionId,
        parentId: comment._id,
        authorId: userId,
        type: "discussion",
        content: content.trim(),
      });
      setReplyContent("");
      replyEditorRef.current?.setMarkdown("");
      setShowReplyForm(false);
    } catch (error) {
      console.error("Failed to add reply:", error);
    }
  }, [userId, replyContent, addComment, comment.bepId, comment._id, versionId]);

  useEffect(() => {
    const container = replyContainerRef.current;
    if (!container) return;
    
    const handleKeyDown = (e: globalThis.KeyboardEvent) => {
      if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
        e.preventDefault();
        handleSubmitReply();
      }
    };
    
    container.addEventListener("keydown", handleKeyDown);
    return () => container.removeEventListener("keydown", handleKeyDown);
  }, [handleSubmitReply]);

  const replyCount = replies?.length ?? 0;

  return (
    <div className={cn("group", comment.resolved && "opacity-50")}>
      <div className={cn("flex gap-3", depth > 0 && "ml-12")}>
        <Avatar name={comment.authorName} size={depth > 0 ? "sm" : "md"} />
        <div className="flex-1 min-w-0">
          {/* Header */}
          <div className="flex items-center gap-2 flex-wrap mb-1">
            <span className="font-medium text-sm">{comment.authorName}</span>
            <TypeBadge type={comment.type} />
            <span className="text-xs text-muted-foreground">{formatTime(comment.createdAt)}</span>
            {comment.resolved && (
              <span className="text-xs text-green-600 flex items-center gap-1">
                <Check className="h-3 w-3" /> Resolved
              </span>
            )}
          </div>

          {/* Content - rendered as markdown */}
          <div className="prose prose-sm dark:prose-invert max-w-none" data-comment-id={comment._id}>
            <BepContent content={comment.content} linkContext={linkContext} />
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
              className={cn("h-7 px-2", hasReacted("thumbsUp") && "text-blue-500")}
              onClick={() => handleReaction("thumbsUp")}
              disabled={readOnly}
            >
              <ThumbsUp className="h-3.5 w-3.5 mr-1" />
              {getReactionCount("thumbsUp") > 0 && getReactionCount("thumbsUp")}
            </Button>
            <Button
              variant="ghost"
              size="sm"
              className={cn("h-7 px-2", hasReacted("thumbsDown") && "text-blue-500")}
              onClick={() => handleReaction("thumbsDown")}
              disabled={readOnly}
            >
              <ThumbsDown className="h-3.5 w-3.5 mr-1" />
              {getReactionCount("thumbsDown") > 0 && getReactionCount("thumbsDown")}
            </Button>
            <Button
              variant="ghost"
              size="sm"
              className={cn("h-7 px-2", hasReacted("heart") && "text-red-500")}
              onClick={() => handleReaction("heart")}
              disabled={readOnly}
            >
              <Heart className="h-3.5 w-3.5 mr-1" />
              {getReactionCount("heart") > 0 && getReactionCount("heart")}
            </Button>

            {depth < 2 && !readOnly && (
              <Button
                variant="ghost"
                size="sm"
                className="h-7 px-2"
                onClick={() => setShowReplyForm(!showReplyForm)}
              >
                <Reply className="h-3.5 w-3.5 mr-1" />
                Reply
              </Button>
            )}

            {!readOnly && (
              <DropdownMenu>
                <DropdownMenuTrigger asChild>
                  <Button variant="ghost" size="sm" className="h-7 px-2 opacity-0 group-hover:opacity-100">
                    <MoreHorizontal className="h-4 w-4" />
                  </Button>
                </DropdownMenuTrigger>
                <DropdownMenuContent align="start">
                  {!comment.resolved ? (
                    <DropdownMenuItem onClick={handleResolve}>
                      <Check className="h-4 w-4 mr-2" /> Mark resolved
                    </DropdownMenuItem>
                  ) : (
                    <DropdownMenuItem onClick={handleUnresolve}>
                      <RotateCcw className="h-4 w-4 mr-2" /> Reopen
                    </DropdownMenuItem>
                  )}
                  {isAuthor && (
                    <DropdownMenuItem onClick={handleDelete} className="text-destructive">
                      <Trash2 className="h-4 w-4 mr-2" /> Delete
                    </DropdownMenuItem>
                  )}
                  <DropdownMenuSeparator />
                  <DecisionForm
                    bepId={comment.bepId}
                    sourceCommentId={comment._id}
                    trigger={
                      <DropdownMenuItem onSelect={(e) => e.preventDefault()}>
                        <Gavel className="h-4 w-4 mr-2" /> Mark as Decision
                      </DropdownMenuItem>
                    }
                  />
                  <IssueForm
                    bepId={comment.bepId}
                    sourceCommentId={comment._id}
                    trigger={
                      <DropdownMenuItem onSelect={(e) => e.preventDefault()}>
                        <AlertCircle className="h-4 w-4 mr-2" /> Create Issue
                      </DropdownMenuItem>
                    }
                  />
                </DropdownMenuContent>
              </DropdownMenu>
            )}
          </div>

          {/* Reply form with MDX editor */}
          {showReplyForm && !readOnly && (
            <div className="mt-3 flex gap-2">
              <Avatar name={user?.name ?? "You"} size="sm" />
              <div className="flex-1 space-y-2">
                <div ref={replyContainerRef} className="border rounded-lg overflow-hidden">
                  <MDXEditorComponent
                    ref={replyEditorRef}
                    initialContent=""
                    editable={true}
                    onChange={setReplyContent}
                    placeholder="Write a reply..."
                    showToolbar={true}
                  />
                </div>
                <div className="flex justify-end gap-2">
                  <Button size="sm" variant="ghost" onClick={() => { setShowReplyForm(false); setReplyContent(""); }}>
                    Cancel
                  </Button>
                  <Button size="sm" onClick={handleSubmitReply} disabled={!replyContent.trim()}>
                    Reply
                  </Button>
                </div>
              </div>
            </div>
          )}
        </div>
      </div>

      {/* Replies */}
      {replyCount > 0 && (
        <div className="mt-3">
          {!showReplies ? (
            <button
              className="ml-12 text-xs text-muted-foreground hover:text-foreground flex items-center gap-1"
              onClick={() => setShowReplies(true)}
            >
              <ChevronDown className="h-3 w-3" />
              Show {replyCount} {replyCount === 1 ? 'reply' : 'replies'}
            </button>
          ) : (
            <div className="space-y-4">
              {replyCount > 2 && (
                <button
                  className="ml-12 text-xs text-muted-foreground hover:text-foreground flex items-center gap-1"
                  onClick={() => setShowReplies(false)}
                >
                  <ChevronUp className="h-3 w-3" />
                  Hide replies
                </button>
              )}
              {replies?.map((reply) => (
                <SingleComment
                  key={reply._id}
                  comment={reply as Comment}
                  versionId={versionId}
                  depth={depth + 1}
                  readOnly={readOnly}
                  linkContext={linkContext}
                />
              ))}
            </div>
          )}
        </div>
      )}
    </div>
  );
}

export function CommentThread({
  bepId,
  versionId,
  pageId,
  viewingVersionId,
  readOnly = false,
  linkContext,
  onNavigateToIssue,
  onNavigateToDecision,
}: CommentThreadProps) {
  const { userId, user } = useUser();
  const [showResolved, setShowResolved] = useState(false);
  const [newCommentContent, setNewCommentContent] = useState("");
  const [commentType, setCommentType] = useState<"discussion" | "concern" | "question">("discussion");
  const [isSubmitting, setIsSubmitting] = useState(false);
  const editorRef = useRef<MDXEditorHandle>(null);

  const comments = useQuery(api.comments.byBepPage, {
    bepId,
    pageId,
    versionId: viewingVersionId ?? versionId,
  });

  const linkedItemsBatch = useQuery(api.comments.getLinkedItemsBatch, { bepId });
  const addComment = useMutation(api.comments.add);

  const handleSubmitComment = useCallback(async () => {
    const content = editorRef.current?.getMarkdown() || newCommentContent;
    if (!userId || !content.trim() || isSubmitting) return;
    
    setIsSubmitting(true);
    try {
      await addComment({
        bepId,
        versionId,
        pageId,
        authorId: userId,
        type: commentType,
        content: content.trim(),
      });
      setNewCommentContent("");
      editorRef.current?.setMarkdown("");
      setCommentType("discussion");
    } catch (error) {
      console.error("Failed to add comment:", error);
    } finally {
      setIsSubmitting(false);
    }
  }, [userId, newCommentContent, isSubmitting, addComment, bepId, versionId, pageId, commentType]);

  const editorContainerRef = useRef<HTMLDivElement>(null);
  
  useEffect(() => {
    const container = editorContainerRef.current;
    if (!container) return;
    
    const handleKeyDown = (e: globalThis.KeyboardEvent) => {
      if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
        e.preventDefault();
        handleSubmitComment();
      }
    };
    
    container.addEventListener("keydown", handleKeyDown);
    return () => container.removeEventListener("keydown", handleKeyDown);
  }, [handleSubmitComment]);

  if (comments === undefined) {
    return (
      <div className="space-y-4">
        <Skeleton className="h-8 w-48" />
        <Skeleton className="h-20 w-full" />
        <Skeleton className="h-20 w-full" />
      </div>
    );
  }

  // Filter to only top-level, non-inline comments
  const topLevelComments = comments.filter((c) => !c.parentId && !c.anchor);
  const unresolvedComments = topLevelComments.filter((c) => !c.resolved);
  const resolvedComments = topLevelComments.filter((c) => c.resolved);
  
  const displayedComments = showResolved 
    ? [...unresolvedComments, ...resolvedComments]
    : unresolvedComments;

  displayedComments.sort((a, b) => b.createdAt - a.createdAt);

  return (
    <div className="space-y-6">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-2">
          <MessageSquare className="h-5 w-5 text-muted-foreground" />
          <h3 className="font-semibold text-lg">
            Discussion
            <span className="text-muted-foreground font-normal text-base ml-2">
              {unresolvedComments.length}
              {resolvedComments.length > 0 && !showResolved && (
                <span className="text-sm"> · {resolvedComments.length} resolved</span>
              )}
            </span>
          </h3>
        </div>

        {resolvedComments.length > 0 && (
          <Button
            variant="ghost"
            size="sm"
            onClick={() => setShowResolved(!showResolved)}
            className="text-xs"
          >
            {showResolved ? "Hide" : "Show"} resolved ({resolvedComments.length})
          </Button>
        )}
      </div>

      {/* New comment form with MDX editor */}
      {!readOnly && (
        <div className="flex gap-3">
          <Avatar name={user?.name ?? "You"} />
          <div className="flex-1 space-y-3">
            <div className="flex gap-2">
              <Select
                value={commentType}
                onValueChange={(v) => setCommentType(v as typeof commentType)}
                disabled={isSubmitting}
              >
                <SelectTrigger className="w-40">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="discussion">
                    <span className="flex items-center gap-2">
                      <MessageSquare className="h-4 w-4" /> Discussion
                    </span>
                  </SelectItem>
                  <SelectItem value="concern">
                    <span className="flex items-center gap-2">
                      <AlertCircle className="h-4 w-4" /> Concern
                    </span>
                  </SelectItem>
                  <SelectItem value="question">
                    <span className="flex items-center gap-2">
                      <HelpCircle className="h-4 w-4" /> Question
                    </span>
                  </SelectItem>
                </SelectContent>
              </Select>
            </div>
            <div ref={editorContainerRef} className="border rounded-lg overflow-hidden">
              <MDXEditorComponent
                ref={editorRef}
                initialContent=""
                editable={!isSubmitting}
                onChange={setNewCommentContent}
                placeholder="Share your thoughts on this proposal..."
                showToolbar={true}
              />
            </div>
            <div className="flex justify-end">
              <Button 
                onClick={handleSubmitComment} 
                disabled={!newCommentContent.trim() || isSubmitting}
              >
                {isSubmitting ? "Posting..." : "Comment"}
              </Button>
            </div>
          </div>
        </div>
      )}

      {readOnly && (
        <div className="text-sm text-muted-foreground bg-muted/30 rounded-lg p-3 text-center">
          Comments are read-only when viewing historical versions.
        </div>
      )}

      {/* Comments list */}
      {displayedComments.length > 0 ? (
        <div className="space-y-6">
          {displayedComments.map((comment) => (
            <SingleComment
              key={comment._id}
              comment={comment as Comment}
              versionId={versionId}
              readOnly={readOnly}
              linkContext={linkContext}
              linkedItems={linkedItemsBatch?.[comment._id]}
              onNavigateToIssue={onNavigateToIssue}
              onNavigateToDecision={onNavigateToDecision}
            />
          ))}
        </div>
      ) : (
        <div className="text-center py-8 text-muted-foreground text-sm">
          No comments yet. Start the discussion!
        </div>
      )}
    </div>
  );
}
