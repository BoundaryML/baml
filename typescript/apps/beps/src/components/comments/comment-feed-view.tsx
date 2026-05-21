"use client";

import { useState, useRef, useEffect, useCallback, useMemo } from "react";
import { useQuery, useMutation } from "convex/react";
import { api } from "../../../convex/_generated/api";
import { Id } from "../../../convex/_generated/dataModel";
import { useUser } from "@/components/providers/user-provider";
import { BepContent } from "@/components/bep/bep-content";
import { MDXEditorComponent, MDXEditorHandle } from "@/components/editor/mdx";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
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
  FileText,
  Clock,
  Filter,
  Quote,
} from "lucide-react";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { cn } from "@/lib/utils";

interface CommentFeedViewProps {
  bepId: Id<"beps">;
  versionId?: Id<"bepVersions">;
  currentVersionNumber?: number | null;
  linkContext?: BepLinkContext;
  onNavigateToPage?: (pageSlug: string | null) => void;
  bepContent?: string;
  bepPages?: Array<{ slug: string; title: string; content: string }>;
}

interface FeedComment {
  _id: Id<"comments">;
  bepId: Id<"beps">;
  versionId?: Id<"bepVersions">;
  pageId?: Id<"bepPages">;
  authorId: Id<"users">;
  authorName: string;
  authorAvatarUrl?: string;
  parentId?: Id<"comments">;
  rootCommentId?: Id<"comments">; // The root of the thread for proper reply handling
  type: string;
  content: string;
  anchor?: {
    nodeId: string;
    nodeType: string;
    nodeText: string;
  };
  reactions?: {
    thumbsUp?: Id<"users">[];
    thumbsDown?: Id<"users">[];
    heart?: Id<"users">[];
    thinking?: Id<"users">[];
  };
  resolved: boolean;
  resolvedByName?: string;
  createdAt: number;
  pageName?: string;
  pageSlug?: string;
  versionNumber?: number;
  parentAuthorName?: string;
}

function extractMarkdownContext(
  content: string | undefined,
  anchorText: string,
  contextLines: number = 3
): string | null {
  if (!content || !anchorText) return null;

  const lines = content.split("\n");
  const anchorTextLower = anchorText.toLowerCase().trim();

  // Find the line that contains the anchor text
  let matchLineIndex = -1;
  for (let i = 0; i < lines.length; i++) {
    if (lines[i].toLowerCase().includes(anchorTextLower.slice(0, 50))) {
      matchLineIndex = i;
      break;
    }
  }

  if (matchLineIndex === -1) {
    // Try to find a partial match
    const words = anchorTextLower.split(/\s+/).filter(w => w.length > 3);
    for (let i = 0; i < lines.length; i++) {
      const lineLower = lines[i].toLowerCase();
      const matchCount = words.filter(w => lineLower.includes(w)).length;
      if (matchCount >= Math.min(3, words.length)) {
        matchLineIndex = i;
        break;
      }
    }
  }

  if (matchLineIndex === -1) return null;

  // Get surrounding lines for context
  const startLine = Math.max(0, matchLineIndex - contextLines);
  const endLine = Math.min(lines.length, matchLineIndex + contextLines + 1);

  const contextLines2 = lines.slice(startLine, endLine);
  return contextLines2.join("\n");
}

function Avatar({ name, size = "md" }: { name: string; size?: "sm" | "md" }) {
  const initial = name[0]?.toUpperCase() || "?";
  const colors = [
    "bg-blue-500",
    "bg-green-500",
    "bg-purple-500",
    "bg-pink-500",
    "bg-indigo-500",
    "bg-teal-500",
    "bg-orange-500",
    "bg-cyan-500",
  ];
  const colorIndex =
    name.split("").reduce((acc, char) => acc + char.charCodeAt(0), 0) %
    colors.length;
  const sizeClass = size === "sm" ? "w-7 h-7 text-xs" : "w-9 h-9 text-sm";

  return (
    <div
      className={cn(
        "rounded-full flex items-center justify-center text-white font-medium shrink-0",
        colors[colorIndex],
        sizeClass
      )}
    >
      {initial}
    </div>
  );
}

function TypeBadge({ type }: { type: string }) {
  if (type === "concern") {
    return (
      <span className="inline-flex items-center gap-1 text-xs text-amber-600 bg-amber-50 dark:bg-amber-950 dark:text-amber-400 px-1.5 py-0.5 rounded">
        <AlertCircle className="h-3 w-3" /> Concern
      </span>
    );
  }
  if (type === "question") {
    return (
      <span className="inline-flex items-center gap-1 text-xs text-blue-600 bg-blue-50 dark:bg-blue-950 dark:text-blue-400 px-1.5 py-0.5 rounded">
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

function getQuotedText(content: string): string | null {
  const lines = content.split("\n");
  const quoteLines: string[] = [];

  for (const line of lines) {
    if (line.startsWith("> ")) {
      quoteLines.push(line.slice(2));
    }
  }

  return quoteLines.length > 0 ? quoteLines.join(" ") : null;
}

function stripQuoteLines(content: string): string {
  const lines = content.split("\n");
  const textLines: string[] = [];
  for (const line of lines) {
    if (!line.startsWith("> ") && line !== ">") {
      textLines.push(line);
    }
  }
  while (textLines.length > 0 && !textLines[0].trim()) textLines.shift();
  while (textLines.length > 0 && !textLines[textLines.length - 1].trim())
    textLines.pop();
  return textLines.join("\n");
}

function FeedCommentCard({
  comment,
  versionId,
  readOnly,
  linkContext,
  onNavigateToPage,
  getContentForPage,
}: {
  comment: FeedComment;
  versionId?: Id<"bepVersions">;
  readOnly?: boolean;
  linkContext?: BepLinkContext;
  onNavigateToPage?: (pageSlug: string | null) => void;
  getContentForPage?: (pageSlug: string | null) => string | undefined;
}) {
  const { userId, user } = useUser();
  const toggleReaction = useMutation(api.comments.toggleReaction);
  const resolveComment = useMutation(api.comments.resolve);
  const unresolveComment = useMutation(api.comments.unresolve);
  const deleteComment = useMutation(api.comments.remove);
  const addComment = useMutation(api.comments.add);

  const [showReplyForm, setShowReplyForm] = useState(false);
  const [replyContent, setReplyContent] = useState("");
  const replyEditorRef = useRef<MDXEditorHandle>(null);
  const replyContainerRef = useRef<HTMLDivElement>(null);

  const getReactionCount = (
    emoji: "thumbsUp" | "thumbsDown" | "heart" | "thinking"
  ) => comment.reactions?.[emoji]?.length ?? 0;
  const hasReacted = (
    emoji: "thumbsUp" | "thumbsDown" | "heart" | "thinking"
  ) => (userId ? comment.reactions?.[emoji]?.includes(userId) ?? false : false);
  const isAuthor = userId === comment.authorId;

  const handleReaction = async (
    emoji: "thumbsUp" | "thumbsDown" | "heart" | "thinking"
  ) => {
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

  const handleSubmitReply = useCallback(async () => {
    const content = replyEditorRef.current?.getMarkdown() || replyContent;
    if (!userId || !content.trim() || !versionId) return;
    try {
      // Reply to the root comment of the thread, not to nested replies
      const replyToId = comment.rootCommentId ?? comment._id;
      
      await addComment({
        bepId: comment.bepId,
        versionId,
        pageId: comment.pageId,
        parentId: replyToId,
        authorId: userId,
        type: "discussion",
        content: content.trim(),
        // Pass anchor so reply shows in sidebar view (sidebar filters by anchor)
        anchor: comment.anchor,
      });
      setReplyContent("");
      replyEditorRef.current?.setMarkdown("");
      setShowReplyForm(false);
    } catch (error) {
      console.error("Failed to add reply:", error);
    }
  }, [userId, replyContent, addComment, comment, versionId]);

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

  const quotedText = getQuotedText(comment.content);
  const isReply = !!comment.parentId;

  return (
    <div
      className={cn(
        "group bg-card border rounded-lg p-4",
        comment.resolved && "opacity-60"
      )}
      data-comment-id={comment._id}
    >
      {/* Location badge - shows which page/section this comment is on */}
      <div className="flex items-center gap-2 text-xs text-muted-foreground mb-3 flex-wrap">
        <div className="flex items-center gap-1">
          <Clock className="h-3 w-3" />
          {formatTime(comment.createdAt)}
        </div>
        {comment.pageName && (
          <button
            className="flex items-center gap-1 hover:text-foreground transition-colors"
            onClick={() => onNavigateToPage?.(comment.pageSlug ?? null)}
          >
            <FileText className="h-3 w-3" />
            <span>{comment.pageName}</span>
          </button>
        )}
        {!comment.pageName && (
          <button
            className="flex items-center gap-1 hover:text-foreground transition-colors"
            onClick={() => onNavigateToPage?.(null)}
          >
            <FileText className="h-3 w-3" />
            <span>README</span>
          </button>
        )}
        {comment.versionNumber && (
          <span className="text-muted-foreground">v{comment.versionNumber}</span>
        )}
        {isReply && comment.parentAuthorName && (
          <span className="flex items-center gap-1">
            <Reply className="h-3 w-3" />
            Reply to {comment.parentAuthorName}
          </span>
        )}
      </div>

      {/* Anchor context - the markdown segment being referenced */}
      {comment.anchor && (
        <div className="mb-3 p-3 bg-muted/50 rounded-md border-l-4 border-amber-400">
          <div className="flex items-center gap-1.5 text-xs text-muted-foreground mb-1">
            <Quote className="h-3 w-3" />
            Referenced section
          </div>
          {(() => {
            const pageContent = getContentForPage?.(comment.pageSlug ?? null);
            const contextMarkdown = extractMarkdownContext(
              pageContent,
              comment.anchor.nodeText
            );
            if (contextMarkdown) {
              return (
                <div className="prose prose-sm dark:prose-invert max-w-none opacity-80">
                  <BepContent content={contextMarkdown} linkContext={linkContext} />
                </div>
              );
            }
            return (
              <p className="text-sm text-muted-foreground italic line-clamp-3">
                {comment.anchor.nodeText}
              </p>
            );
          })()}
        </div>
      )}

      {/* Main comment content */}
      <div className="flex gap-3">
        <Avatar name={comment.authorName} />
        <div className="flex-1 min-w-0">
          {/* Header */}
          <div className="flex items-center gap-2 flex-wrap mb-1">
            <span className="font-medium text-sm">{comment.authorName}</span>
            <TypeBadge type={comment.type} />
            {comment.resolved && (
              <span className="text-xs text-green-600 flex items-center gap-1">
                <Check className="h-3 w-3" /> Resolved
              </span>
            )}
          </div>

          {/* Quoted text (from inline comments) */}
          {quotedText && (
            <div className="text-xs text-muted-foreground/70 italic border-l-2 border-amber-400 pl-2 my-2 line-clamp-2">
              &ldquo;{quotedText}&rdquo;
            </div>
          )}

          {/* Content */}
          <div className="prose prose-sm dark:prose-invert max-w-none">
            <BepContent
              content={stripQuoteLines(comment.content)}
              linkContext={linkContext}
            />
          </div>

          {/* Actions */}
          <div className="flex items-center gap-1 mt-3">
            <Button
              variant="ghost"
              size="sm"
              className={cn(
                "h-7 px-2",
                hasReacted("thumbsUp") && "text-blue-500"
              )}
              onClick={() => handleReaction("thumbsUp")}
              disabled={readOnly}
            >
              <ThumbsUp className="h-3.5 w-3.5 mr-1" />
              {getReactionCount("thumbsUp") > 0 && getReactionCount("thumbsUp")}
            </Button>
            <Button
              variant="ghost"
              size="sm"
              className={cn(
                "h-7 px-2",
                hasReacted("thumbsDown") && "text-blue-500"
              )}
              onClick={() => handleReaction("thumbsDown")}
              disabled={readOnly}
            >
              <ThumbsDown className="h-3.5 w-3.5 mr-1" />
              {getReactionCount("thumbsDown") > 0 &&
                getReactionCount("thumbsDown")}
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

            {!readOnly && (
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
                  <Button
                    variant="ghost"
                    size="sm"
                    className="h-7 px-2 opacity-0 group-hover:opacity-100"
                  >
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
                    <>
                      <DropdownMenuSeparator />
                      <DropdownMenuItem
                        onClick={handleDelete}
                        className="text-destructive"
                      >
                        <Trash2 className="h-4 w-4 mr-2" /> Delete
                      </DropdownMenuItem>
                    </>
                  )}
                </DropdownMenuContent>
              </DropdownMenu>
            )}
          </div>

          {/* Reply form */}
          {showReplyForm && !readOnly && (
            <div className="mt-4 flex gap-2">
              <Avatar name={user?.name ?? "You"} size="sm" />
              <div className="flex-1 space-y-2">
                <div
                  ref={replyContainerRef}
                  className="border rounded-lg overflow-hidden"
                >
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
                  <Button
                    size="sm"
                    variant="ghost"
                    onClick={() => {
                      setShowReplyForm(false);
                      setReplyContent("");
                    }}
                  >
                    Cancel
                  </Button>
                  <Button
                    size="sm"
                    onClick={handleSubmitReply}
                    disabled={!replyContent.trim()}
                  >
                    Reply
                  </Button>
                </div>
              </div>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

function FeedThreadCard({
  rootComment,
  replies,
  latestActivity,
  versionId,
  linkContext,
  onNavigateToPage,
  getContentForPage,
}: {
  rootComment: FeedComment;
  replies: FeedComment[];
  latestActivity: number;
  versionId?: Id<"bepVersions">;
  linkContext?: BepLinkContext;
  onNavigateToPage?: (pageSlug: string | null) => void;
  getContentForPage?: (pageSlug: string | null) => string | undefined;
}) {
  const { userId, user } = useUser();
  const addComment = useMutation(api.comments.add);
  const resolveComment = useMutation(api.comments.resolve);
  const unresolveComment = useMutation(api.comments.unresolve);

  const [showReplyForm, setShowReplyForm] = useState(false);
  const [replyContent, setReplyContent] = useState("");
  const [expanded, setExpanded] = useState(true);
  const replyEditorRef = useRef<MDXEditorHandle>(null);
  const replyContainerRef = useRef<HTMLDivElement>(null);

  const handleSubmitReply = useCallback(async () => {
    const content = replyEditorRef.current?.getMarkdown() || replyContent;
    if (!userId || !content.trim() || !versionId) return;
    try {
      await addComment({
        bepId: rootComment.bepId,
        versionId,
        pageId: rootComment.pageId,
        parentId: rootComment._id,
        authorId: userId,
        type: "discussion",
        content: content.trim(),
        // Pass anchor so reply shows in sidebar view (sidebar filters by anchor)
        anchor: rootComment.anchor,
      });
      setReplyContent("");
      replyEditorRef.current?.setMarkdown("");
      setShowReplyForm(false);
    } catch (error) {
      console.error("Failed to add reply:", error);
    }
  }, [userId, replyContent, addComment, rootComment, versionId]);

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

  const handleResolve = async () => {
    if (!userId) return;
    await resolveComment({ commentId: rootComment._id, userId });
  };

  const handleUnresolve = async () => {
    await unresolveComment({ commentId: rootComment._id });
  };

  const quotedText = getQuotedText(rootComment.content);
  const hasNewActivity = replies.length > 0 && replies[replies.length - 1].createdAt === latestActivity;

  return (
    <div
      className={cn(
        "group bg-card border rounded-lg",
        rootComment.resolved && "opacity-60"
      )}
      data-comment-id={rootComment._id}
    >
      {/* Thread header with location and activity info */}
      <div className="flex items-center gap-2 text-xs text-muted-foreground px-4 py-2 bg-muted/30 border-b flex-wrap">
        <div className="flex items-center gap-1">
          <Clock className="h-3 w-3" />
          {formatTime(latestActivity)}
          {hasNewActivity && (
            <span className="text-primary font-medium ml-1">new reply</span>
          )}
        </div>
        {rootComment.pageName ? (
          <button
            className="flex items-center gap-1 hover:text-foreground transition-colors"
            onClick={() => onNavigateToPage?.(rootComment.pageSlug ?? null)}
          >
            <FileText className="h-3 w-3" />
            <span>{rootComment.pageName}</span>
          </button>
        ) : (
          <button
            className="flex items-center gap-1 hover:text-foreground transition-colors"
            onClick={() => onNavigateToPage?.(null)}
          >
            <FileText className="h-3 w-3" />
            <span>README</span>
          </button>
        )}
        {rootComment.versionNumber && (
          <span>v{rootComment.versionNumber}</span>
        )}
        {replies.length > 0 && (
          <span className="ml-auto flex items-center gap-1">
            <MessageSquare className="h-3 w-3" />
            {replies.length} {replies.length === 1 ? "reply" : "replies"}
          </span>
        )}
      </div>

      {/* Anchor context - the markdown segment being referenced */}
      {rootComment.anchor && (
        <div className="mx-4 mt-3 p-3 bg-muted/50 rounded-md border-l-4 border-amber-400">
          <div className="flex items-center gap-1.5 text-xs text-muted-foreground mb-1">
            <Quote className="h-3 w-3" />
            Referenced section
          </div>
          {(() => {
            const pageContent = getContentForPage?.(rootComment.pageSlug ?? null);
            const contextMarkdown = extractMarkdownContext(
              pageContent,
              rootComment.anchor.nodeText
            );
            if (contextMarkdown) {
              return (
                <div className="prose prose-sm dark:prose-invert max-w-none opacity-80">
                  <BepContent content={contextMarkdown} linkContext={linkContext} />
                </div>
              );
            }
            return (
              <p className="text-sm text-muted-foreground italic line-clamp-3">
                {rootComment.anchor.nodeText}
              </p>
            );
          })()}
        </div>
      )}

      {/* Root comment */}
      <div className="p-4">
        <div className="flex gap-3">
          <Avatar name={rootComment.authorName} />
          <div className="flex-1 min-w-0">
            <div className="flex items-center gap-2 flex-wrap mb-1">
              <span className="font-medium text-sm">{rootComment.authorName}</span>
              <TypeBadge type={rootComment.type} />
              <span className="text-xs text-muted-foreground">
                {formatTime(rootComment.createdAt)}
              </span>
              {rootComment.resolved && (
                <span className="text-xs text-green-600 flex items-center gap-1">
                  <Check className="h-3 w-3" /> Resolved
                </span>
              )}
            </div>

            {quotedText && (
              <div className="text-xs text-muted-foreground/70 italic border-l-2 border-amber-400 pl-2 my-2 line-clamp-2">
                &ldquo;{quotedText}&rdquo;
              </div>
            )}

            <div className="prose prose-sm dark:prose-invert max-w-none">
              <BepContent
                content={stripQuoteLines(rootComment.content)}
                linkContext={linkContext}
              />
            </div>
          </div>
        </div>
      </div>

      {/* Replies */}
      {replies.length > 0 && (
        <div className="border-t">
          <button
            className="w-full px-4 py-2 text-xs text-muted-foreground hover:bg-muted/30 flex items-center gap-2"
            onClick={() => setExpanded(!expanded)}
          >
            {expanded ? (
              <ChevronUp className="h-3 w-3" />
            ) : (
              <ChevronDown className="h-3 w-3" />
            )}
            {expanded ? "Hide" : "Show"} {replies.length} {replies.length === 1 ? "reply" : "replies"}
          </button>

          {expanded && (
            <div className="px-4 pb-4 space-y-3">
              {replies.map((reply) => (
                <div key={reply._id} className="flex gap-2 pl-6 border-l-2 border-muted">
                  <Avatar name={reply.authorName} size="sm" />
                  <div className="flex-1 min-w-0">
                    <div className="flex items-center gap-2 flex-wrap mb-1">
                      <span className="font-medium text-sm">{reply.authorName}</span>
                      <span className="text-xs text-muted-foreground">
                        {formatTime(reply.createdAt)}
                      </span>
                    </div>
                    <div className="prose prose-sm dark:prose-invert max-w-none text-sm">
                      <BepContent
                        content={stripQuoteLines(reply.content)}
                        linkContext={linkContext}
                      />
                    </div>
                  </div>
                </div>
              ))}
            </div>
          )}
        </div>
      )}

      {/* Actions */}
      <div className="border-t px-4 py-2 bg-muted/20 flex items-center gap-2">
        <Button
          variant="ghost"
          size="sm"
          className="h-7 px-2 text-xs"
          onClick={() => setShowReplyForm(!showReplyForm)}
        >
          <Reply className="h-3.5 w-3.5 mr-1" />
          Reply
        </Button>

        {!rootComment.resolved ? (
          <Button
            variant="ghost"
            size="sm"
            className="h-7 px-2 text-xs ml-auto"
            onClick={handleResolve}
          >
            <Check className="h-3.5 w-3.5 mr-1" />
            Resolve
          </Button>
        ) : (
          <Button
            variant="ghost"
            size="sm"
            className="h-7 px-2 text-xs ml-auto"
            onClick={handleUnresolve}
          >
            <RotateCcw className="h-3.5 w-3.5 mr-1" />
            Reopen
          </Button>
        )}
      </div>

      {/* Reply form */}
      {showReplyForm && (
        <div className="border-t p-4 bg-muted/10">
          <div className="flex gap-2">
            <Avatar name={user?.name ?? "You"} size="sm" />
            <div className="flex-1 space-y-2">
              <div
                ref={replyContainerRef}
                className="border rounded-lg overflow-hidden"
              >
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
                <Button
                  size="sm"
                  variant="ghost"
                  onClick={() => {
                    setShowReplyForm(false);
                    setReplyContent("");
                  }}
                >
                  Cancel
                </Button>
                <Button
                  size="sm"
                  onClick={handleSubmitReply}
                  disabled={!replyContent.trim()}
                >
                  Reply
                </Button>
              </div>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

export function CommentFeedView({
  bepId,
  versionId,
  currentVersionNumber,
  linkContext,
  onNavigateToPage,
  bepContent,
  bepPages,
}: CommentFeedViewProps) {
  const [showResolved, setShowResolved] = useState(false);

  const comments = useQuery(api.comments.allByBepNewestFirst, {
    bepId,
    versionId,
    includeResolved: showResolved,
  });

  const getContentForPage = useCallback(
    (pageSlug: string | null): string | undefined => {
      if (!pageSlug) {
        return bepContent;
      }
      return bepPages?.find((p) => p.slug === pageSlug)?.content;
    },
    [bepContent, bepPages]
  );

  // Group comments into threads and sort by latest activity
  const threads = useMemo(() => {
    if (!comments) return [];

    // Separate root comments and replies
    const rootComments = comments.filter((c) => !c.parentId);
    const replies = comments.filter((c) => c.parentId);

    // Build thread map: rootId -> { root, replies, latestActivity }
    const threadMap = new Map<
      string,
      {
        root: FeedComment;
        replies: FeedComment[];
        latestActivity: number;
      }
    >();

    // Add root comments
    for (const root of rootComments) {
      threadMap.set(root._id as string, {
        root: root as FeedComment,
        replies: [],
        latestActivity: root.createdAt,
      });
    }

    // Add replies to their threads
    for (const reply of replies) {
      const rootId = (reply.rootCommentId ?? reply.parentId) as string;
      const thread = threadMap.get(rootId);
      if (thread) {
        thread.replies.push(reply as FeedComment);
        // Update latest activity if this reply is newer
        if (reply.createdAt > thread.latestActivity) {
          thread.latestActivity = reply.createdAt;
        }
      }
    }

    // Sort replies within each thread by time (oldest first for reading order)
    for (const thread of threadMap.values()) {
      thread.replies.sort((a, b) => a.createdAt - b.createdAt);
    }

    // Convert to array and sort threads by latest activity (newest first)
    const threadArray = Array.from(threadMap.values());
    threadArray.sort((a, b) => b.latestActivity - a.latestActivity);

    return threadArray;
  }, [comments]);

  if (comments === undefined) {
    return (
      <div className="space-y-4">
        <Skeleton className="h-8 w-48" />
        <Skeleton className="h-32 w-full" />
        <Skeleton className="h-32 w-full" />
        <Skeleton className="h-32 w-full" />
      </div>
    );
  }

  // Count threads, not individual comments
  const resolvedThreads = threads.filter((t) => t.root.resolved).length;
  const unresolvedThreads = threads.filter((t) => !t.root.resolved).length;

  return (
    <div className="space-y-4">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-2">
          <MessageSquare className="h-5 w-5 text-muted-foreground" />
          <h3 className="font-semibold text-lg">
            All Comments
            <span className="text-muted-foreground font-normal text-base ml-2">
              {showResolved ? threads.length : unresolvedThreads} threads
              {!showResolved && resolvedThreads > 0 && (
                <span className="text-sm"> · {resolvedThreads} resolved</span>
              )}
            </span>
          </h3>
        </div>

        <div className="flex items-center gap-2">
          <Button
            variant="outline"
            size="sm"
            onClick={() => setShowResolved(!showResolved)}
            className="text-xs"
          >
            <Filter className="h-3.5 w-3.5 mr-1.5" />
            {showResolved ? "Hide resolved" : "Show resolved"}
          </Button>
        </div>
      </div>

      {/* Live feed indicator */}
      <div className="flex items-center gap-2 text-xs text-muted-foreground bg-muted/30 rounded-lg px-3 py-2">
        <div className="w-2 h-2 bg-green-500 rounded-full animate-pulse" />
        Live feed - threads with newest activity first
      </div>

      {/* Threads list */}
      {threads.length > 0 ? (
        <div className="space-y-4">
          {threads.map((thread) => (
            <FeedThreadCard
              key={thread.root._id}
              rootComment={thread.root}
              replies={thread.replies}
              latestActivity={thread.latestActivity}
              versionId={versionId}
              linkContext={linkContext}
              onNavigateToPage={onNavigateToPage}
              getContentForPage={getContentForPage}
            />
          ))}
        </div>
      ) : (
        <div className="text-center py-12 text-muted-foreground">
          <MessageSquare className="h-12 w-12 mx-auto mb-4 opacity-30" />
          <p className="text-lg font-medium mb-1">No comments yet</p>
          <p className="text-sm">
            Comments will appear here as they are added to any section of this BEP.
          </p>
        </div>
      )}
    </div>
  );
}
