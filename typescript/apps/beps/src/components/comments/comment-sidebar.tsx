"use client";

import { useState, useEffect, useRef, useMemo, useCallback } from "react";
import { useMutation } from "convex/react";
import { api } from "../../../convex/_generated/api";
import { Id } from "../../../convex/_generated/dataModel";
import { useUser } from "@/components/providers/user-provider";
import { Button } from "@/components/ui/button";
import { Textarea } from "@/components/ui/textarea";
import { Check, AlertCircle, HelpCircle, ChevronDown, ChevronUp, MessageSquare, X } from "lucide-react";
import { cn } from "@/lib/utils";
import {
  Sheet,
  SheetContent,
  SheetHeader,
  SheetTitle,
  SheetTrigger,
} from "@/components/ui/sheet";

interface Comment {
  _id: Id<"comments">;
  parentId?: Id<"comments">;
  anchor?: {
    nodeId: string;
    nodeType: string;
    nodeText: string;
  };
  authorName: string;
  content: string;
  type: string;
  createdAt: number;
  resolved: boolean;
}

interface CommentSidebarProps {
  contentSelector: string;
  bepId: Id<"beps">;
  versionId: Id<"bepVersions">;
  pageId?: Id<"bepPages">;
  comments: Comment[];
  readOnly?: boolean;
}

interface NewCommentState {
  blockId: string;
  blockElement: Element;
  top: number;
  selectedText: string;
}

interface SelectionPopup {
  x: number;
  y: number;
  text: string;
  blockId: string;
  blockElement: Element;
  blockTop: number;
}

interface CommentThread {
  id: string;
  blockId: string;
  rootComment: Comment;
  replies: Comment[];
  top: number;
  adjustedTop: number;
}

function generateBlockId(element: Element, index: number): string {
  const tagName = element.tagName.toLowerCase();
  const text = element.textContent?.slice(0, 30) || "";
  return `${tagName}-${index}-${text.replace(/\s+/g, "_").slice(0, 15)}`;
}

const formatTime = (timestamp: number) => {
  const date = new Date(timestamp);
  const now = new Date();
  const isToday = date.toDateString() === now.toDateString();
  
  if (isToday) {
    return date.toLocaleTimeString("en-US", { hour: "numeric", minute: "2-digit", hour12: true });
  }
  return date.toLocaleDateString("en-US", { month: "short", day: "numeric" });
};

function TypeIcon({ type }: { type: string }) {
  if (type === "concern") return <AlertCircle className="h-3 w-3 text-amber-500" />;
  if (type === "question") return <HelpCircle className="h-3 w-3 text-blue-500" />;
  return null;
}

function Avatar({ name, size = "sm" }: { name: string; size?: "sm" | "xs" }) {
  const initial = name[0]?.toUpperCase() || "?";
  const colors = [
    'bg-blue-500', 'bg-green-500', 'bg-purple-500', 'bg-pink-500', 
    'bg-indigo-500', 'bg-teal-500', 'bg-orange-500', 'bg-cyan-500'
  ];
  const colorIndex = name.split('').reduce((acc, char) => acc + char.charCodeAt(0), 0) % colors.length;
  const sizeClass = size === "xs" ? "w-5 h-5 text-[10px]" : "w-6 h-6 text-[11px]";
  
  return (
    <div className={cn("rounded-full flex items-center justify-center text-white font-medium shrink-0", colors[colorIndex], sizeClass)}>
      {initial}
    </div>
  );
}

function CommentText({ content }: { content: string }) {
  const lines = content.split('\n');
  const textLines: string[] = [];
  
  for (const line of lines) {
    if (!line.startsWith('> ') && line !== '>' && line.trim()) {
      textLines.push(line);
    }
  }
  
  return <>{textLines.join(' ')}</>;
}

function getQuotedText(content: string): string | null {
  const lines = content.split('\n');
  const quoteLines: string[] = [];
  
  for (const line of lines) {
    if (line.startsWith('> ')) {
      quoteLines.push(line.slice(2));
    }
  }
  
  return quoteLines.length > 0 ? quoteLines.join(' ') : null;
}

function getParticipants(rootComment: Comment, replies: Comment[]): string[] {
  const names = new Set<string>();
  names.add(rootComment.authorName);
  replies.forEach(r => names.add(r.authorName));
  return Array.from(names);
}

const CARD_MIN_HEIGHT = 50;
const CARD_GAP = 6;
const COLLAPSE_THRESHOLD = 2;

export function CommentSidebar({
  contentSelector,
  bepId,
  versionId,
  pageId,
  comments,
  readOnly = false,
}: CommentSidebarProps) {
  const { userId } = useUser();
  const [newComment, setNewComment] = useState<NewCommentState | null>(null);
  const [selectionPopup, setSelectionPopup] = useState<SelectionPopup | null>(null);
  const [activeThread, setActiveThread] = useState<string | null>(null);
  const [expandedThreads, setExpandedThreads] = useState<Set<string>>(new Set());
  const [replyingTo, setReplyingTo] = useState<string | null>(null);
  const [newCommentContent, setNewCommentContent] = useState("");
  const [replyContent, setReplyContent] = useState("");
  const [blockPositions, setBlockPositions] = useState<Map<string, number>>(new Map());
  const [cardHeights, setCardHeights] = useState<Map<string, number>>(new Map());
  const [hoveredThread, setHoveredThread] = useState<string | null>(null);
  const [mobileSheetOpen, setMobileSheetOpen] = useState(false);
  const cardRefs = useRef<Map<string, HTMLDivElement>>(new Map());
  const addComment = useMutation(api.comments.add);
  const resolveComment = useMutation(api.comments.resolve);

  const threads = useMemo(() => {
    const unresolvedComments = comments.filter(c => !c.resolved && c.anchor?.nodeId);
    const topLevel = unresolvedComments.filter(c => !c.parentId);
    
    const repliesByParent = new Map<string, Comment[]>();
    for (const comment of unresolvedComments) {
      if (comment.parentId) {
        const parentIdStr = comment.parentId as string;
        if (!repliesByParent.has(parentIdStr)) repliesByParent.set(parentIdStr, []);
        repliesByParent.get(parentIdStr)!.push(comment);
      }
    }
    
    repliesByParent.forEach((replies) => replies.sort((a, b) => a.createdAt - b.createdAt));
    
    const result: Omit<CommentThread, 'top' | 'adjustedTop'>[] = topLevel.map(rootComment => ({
      id: rootComment._id as string,
      blockId: rootComment.anchor!.nodeId,
      rootComment,
      replies: repliesByParent.get(rootComment._id as string) ?? [],
    }));
    
    result.sort((a, b) => a.rootComment.createdAt - b.rootComment.createdAt);
    return result;
  }, [comments]);

  const blocksWithComments = useMemo(() => new Set(threads.map(t => t.blockId)), [threads]);

  // Highlight block when hovering/clicking on comment
  useEffect(() => {
    const contentElement = document.querySelector(contentSelector);
    if (!contentElement) return;

    contentElement.querySelectorAll('[data-comment-highlight]').forEach(el => {
      el.removeAttribute('data-comment-highlight');
      (el as HTMLElement).style.backgroundColor = '';
      (el as HTMLElement).style.transition = '';
    });

    const threadToHighlight = hoveredThread || activeThread;
    if (!threadToHighlight) return;

    const thread = threads.find(t => t.id === threadToHighlight);
    if (!thread) return;

    Array.from(contentElement.children).forEach((element, index) => {
      const blockId = generateBlockId(element, index);
      if (blockId === thread.blockId) {
        element.setAttribute('data-comment-highlight', 'true');
        (element as HTMLElement).style.transition = 'background-color 0.15s ease';
        (element as HTMLElement).style.backgroundColor = 'rgb(254 249 195 / 0.5)';
      }
    });

    return () => {
      contentElement.querySelectorAll('[data-comment-highlight]').forEach(el => {
        el.removeAttribute('data-comment-highlight');
        (el as HTMLElement).style.backgroundColor = '';
      });
    };
  }, [hoveredThread, activeThread, threads, contentSelector]);

  useEffect(() => {
    const newHeights = new Map<string, number>();
    cardRefs.current.forEach((el, threadId) => {
      if (el) newHeights.set(threadId, el.offsetHeight);
    });
    setCardHeights(newHeights);
  }, [threads, activeThread, replyingTo, expandedThreads]);

  const updateBlockPositions = useCallback(() => {
    const contentElement = document.querySelector(contentSelector);
    if (!contentElement) return;

    const contentRect = contentElement.getBoundingClientRect();
    const newPositions = new Map<string, number>();

    Array.from(contentElement.children).forEach((element, index) => {
      const blockId = generateBlockId(element, index);
      if (blocksWithComments.has(blockId)) {
        const rect = element.getBoundingClientRect();
        newPositions.set(blockId, rect.top - contentRect.top);
      }
    });

    setBlockPositions(newPositions);
  }, [contentSelector, blocksWithComments]);

  useEffect(() => {
    updateBlockPositions();
    const handleUpdate = () => requestAnimationFrame(updateBlockPositions);
    
    window.addEventListener("scroll", handleUpdate, true);
    window.addEventListener("resize", handleUpdate);
    
    const observer = new MutationObserver(handleUpdate);
    const contentElement = document.querySelector(contentSelector);
    if (contentElement) observer.observe(contentElement, { childList: true, subtree: true });

    return () => {
      window.removeEventListener("scroll", handleUpdate, true);
      window.removeEventListener("resize", handleUpdate);
      observer.disconnect();
    };
  }, [updateBlockPositions, contentSelector]);

  // Handle text selection - show popup button
  useEffect(() => {
    if (readOnly) return;

    const findBlockElement = (node: Node | null, contentElement: Element): Element | null => {
      if (!node) return null;
      
      let current: Node | null = node;
      
      // If it's a text node, start with parent
      if (current.nodeType !== Node.ELEMENT_NODE) {
        current = current.parentNode;
      }
      
      // Walk up to find direct child of content element
      while (current && current !== contentElement) {
        if (current.nodeType === Node.ELEMENT_NODE && current.parentNode === contentElement) {
          return current as Element;
        }
        current = current.parentNode;
      }
      
      return null;
    };

    const handleSelectionChange = () => {
      const selection = window.getSelection();
      if (!selection || selection.isCollapsed || selection.rangeCount === 0) {
        return;
      }

      const text = selection.toString().trim();
      if (!text || text.length < 3) {
        setSelectionPopup(null);
        return;
      }

      const contentElement = document.querySelector(contentSelector);
      if (!contentElement) return;

      const range = selection.getRangeAt(0);
      
      // Check if selection is within content area
      if (!contentElement.contains(range.startContainer) && !contentElement.contains(range.endContainer)) {
        setSelectionPopup(null);
        return;
      }

      // Try to find block from: 1) common ancestor, 2) start container, 3) end container
      let blockElement = findBlockElement(range.commonAncestorContainer, contentElement);
      
      if (!blockElement) {
        // Selection spans multiple blocks - use the start block
        blockElement = findBlockElement(range.startContainer, contentElement);
      }
      
      if (!blockElement) {
        // Fallback to end container
        blockElement = findBlockElement(range.endContainer, contentElement);
      }
      
      if (!blockElement) {
        // Last resort: find first block that intersects with selection
        const children = Array.from(contentElement.children);
        for (const child of children) {
          if (range.intersectsNode(child)) {
            blockElement = child;
            break;
          }
        }
      }

      if (!blockElement) {
        setSelectionPopup(null);
        return;
      }

      const blockIndex = Array.from(contentElement.children).indexOf(blockElement);
      const blockId = generateBlockId(blockElement, blockIndex);
      const contentRect = contentElement.getBoundingClientRect();
      const blockRect = blockElement.getBoundingClientRect();
      const rangeRect = range.getBoundingClientRect();

      setSelectionPopup({
        x: rangeRect.right + 8,
        y: rangeRect.top + rangeRect.height / 2,
        text: text.length > 150 ? text.slice(0, 150) + "..." : text,
        blockId,
        blockElement,
        blockTop: blockRect.top - contentRect.top,
      });
    };

    const handleMouseUp = () => {
      // Small delay to let selection complete
      setTimeout(handleSelectionChange, 10);
    };

    const handleMouseDown = (e: MouseEvent) => {
      // Clear popup if clicking outside of it
      const target = e.target as Element;
      if (!target.closest('[data-selection-popup]')) {
        setSelectionPopup(null);
      }
    };

    document.addEventListener("mouseup", handleMouseUp);
    document.addEventListener("mousedown", handleMouseDown);
    
    return () => {
      document.removeEventListener("mouseup", handleMouseUp);
      document.removeEventListener("mousedown", handleMouseDown);
    };
  }, [contentSelector, readOnly]);

  const handleStartComment = () => {
    if (!selectionPopup) return;
    
    setNewComment({
      blockId: selectionPopup.blockId,
      blockElement: selectionPopup.blockElement,
      top: selectionPopup.blockTop,
      selectedText: selectionPopup.text,
    });
    setSelectionPopup(null);
    setActiveThread(null);
    setNewCommentContent("");
    window.getSelection()?.removeAllRanges();
    
    // On mobile, open the sheet to show the comment form
    if (window.innerWidth < 1024) {
      setMobileSheetOpen(true);
    }
  };

  const handleAddComment = async () => {
    if (!userId || !newComment || !newCommentContent.trim()) return;
    
    const finalContent = `> ${newComment.selectedText.split('\n').join(' ')}\n\n${newCommentContent.trim()}`;

    try {
      await addComment({
        bepId, versionId, pageId, authorId: userId, type: "discussion", content: finalContent,
        anchor: { 
          nodeId: newComment.blockId, 
          nodeType: newComment.blockElement.tagName.toLowerCase(), 
          nodeText: newComment.blockElement.textContent?.slice(0, 100) || "" 
        },
      });
      setNewCommentContent("");
      setNewComment(null);
    } catch (error) {
      console.error("Failed to add comment:", error);
    }
  };

  const handleAddReply = async (threadId: string) => {
    if (!userId || !replyContent.trim()) return;
    const thread = threads.find(t => t.id === threadId);
    if (!thread) return;

    try {
      await addComment({
        bepId, versionId, pageId, parentId: thread.rootComment._id, authorId: userId,
        type: "discussion", content: replyContent.trim(), anchor: thread.rootComment.anchor,
      });
      setReplyContent("");
      setReplyingTo(null);
      setExpandedThreads(prev => new Set(prev).add(threadId));
    } catch (error) {
      console.error("Failed to add reply:", error);
    }
  };

  const handleResolve = async (commentId: Id<"comments">) => {
    if (!userId) return;
    try {
      await resolveComment({ commentId, userId });
    } catch (error) {
      console.error("Failed to resolve comment:", error);
    }
  };

  const toggleExpanded = (threadId: string) => {
    setExpandedThreads(prev => {
      const next = new Set(prev);
      if (next.has(threadId)) next.delete(threadId);
      else next.add(threadId);
      return next;
    });
  };

  const positionedThreads = useMemo((): CommentThread[] => {
    const result: CommentThread[] = threads.map(thread => ({
      ...thread,
      top: blockPositions.get(thread.blockId) ?? 0,
      adjustedTop: blockPositions.get(thread.blockId) ?? 0,
    }));

    result.sort((a, b) => a.top - b.top);

    for (let i = 1; i < result.length; i++) {
      const prev = result[i - 1];
      const curr = result[i];
      const prevHeight = cardHeights.get(prev.id) ?? CARD_MIN_HEIGHT;
      const minTop = prev.adjustedTop + prevHeight + CARD_GAP;
      if (curr.adjustedTop < minTop) curr.adjustedTop = minTop;
    }

    return result;
  }, [threads, blockPositions, cardHeights]);

  return (
    <>
      {/* Selection popup - appears next to selected text */}
      {selectionPopup && !newComment && (
        <button
          data-selection-popup
          className="fixed z-50 flex items-center gap-1 px-2 py-1 bg-gray-900 text-white text-xs rounded shadow-lg hover:bg-gray-800 transition-colors"
          style={{ 
            left: selectionPopup.x,
            top: selectionPopup.y,
            transform: 'translateY(-50%)'
          }}
          onClick={handleStartComment}
        >
          <MessageSquare className="h-3 w-3" />
          Comment
        </button>
      )}

      {/* Comment sidebar */}
      <div className="hidden lg:block absolute -right-4 top-0 bottom-0 w-64 translate-x-full">
        {/* Existing comment threads */}
        {positionedThreads.map((thread) => {
          const { id, rootComment, replies, adjustedTop } = thread;
          const isActive = activeThread === id;
          const isReplying = replyingTo === id;
          const isExpanded = expandedThreads.has(id);
          const shouldCollapse = replies.length > COLLAPSE_THRESHOLD && !isExpanded && !isActive;
          const quotedText = getQuotedText(rootComment.content);
          const participants = getParticipants(rootComment, replies);
          const lastMessage = replies.length > 0 ? replies[replies.length - 1] : rootComment;

          return (
            <div
              key={id}
              ref={(el) => { if (el) cardRefs.current.set(id, el); }}
              className={cn(
                "absolute left-0 right-0 bg-white dark:bg-gray-900 rounded border text-[12px] leading-normal cursor-pointer",
                isActive ? "shadow-md ring-1 ring-blue-300 z-10" : "shadow-sm hover:shadow"
              )}
              style={{ top: adjustedTop }}
              onClick={() => setActiveThread(isActive ? null : id)}
              onMouseEnter={() => setHoveredThread(id)}
              onMouseLeave={() => setHoveredThread(null)}
            >
              {/* Root comment */}
              <div className="p-2">
                <div className="flex items-start gap-2">
                  <Avatar name={rootComment.authorName} />
                  <div className="flex-1 min-w-0">
                    <div className="flex items-center gap-1 flex-wrap">
                      <span className="font-semibold text-[12px]">{rootComment.authorName}</span>
                      <TypeIcon type={rootComment.type} />
                      <span className="text-muted-foreground text-[10px] ml-auto">{formatTime(rootComment.createdAt)}</span>
                    </div>
                    {quotedText && (
                      <div className="text-[11px] text-muted-foreground/70 italic border-l-2 border-amber-400 pl-1.5 my-1 line-clamp-1">
                        {quotedText}
                      </div>
                    )}
                    <div className="text-[12px] leading-snug mt-0.5">
                      <CommentText content={rootComment.content} />
                    </div>
                  </div>
                </div>
              </div>

              {/* Collapsed view */}
              {shouldCollapse && (
                <div 
                  className="px-2 pb-2"
                  onClick={(e) => { e.stopPropagation(); toggleExpanded(id); }}
                >
                  <div className="flex items-center gap-1.5 pl-6 py-1.5 bg-gray-50 dark:bg-gray-800/50 rounded text-[11px]">
                    <div className="flex -space-x-1.5">
                      {participants.slice(0, 3).map((name, i) => (
                        <Avatar key={i} name={name} size="xs" />
                      ))}
                    </div>
                    <span className="text-muted-foreground">{replies.length} replies</span>
                    <ChevronDown className="h-3 w-3 text-muted-foreground ml-auto mr-1" />
                  </div>
                  <div className="flex items-start gap-1.5 pl-6 mt-1.5">
                    <Avatar name={lastMessage.authorName} size="xs" />
                    <div className="flex-1 min-w-0">
                      <span className="font-medium text-[11px]">{lastMessage.authorName}</span>
                      <span className="text-muted-foreground text-[10px] ml-1">{formatTime(lastMessage.createdAt)}</span>
                      <div className="text-[11px] text-muted-foreground line-clamp-1">
                        <CommentText content={lastMessage.content} />
                      </div>
                    </div>
                  </div>
                </div>
              )}

              {/* Expanded replies */}
              {!shouldCollapse && replies.length > 0 && (
                <>
                  <div className="border-t border-dashed mx-2" />
                  {replies.map((reply, idx) => {
                    const replyQuote = getQuotedText(reply.content);
                    return (
                      <div key={reply._id} className={cn("px-2 py-1.5", idx === 0 && "pt-2")}>
                        <div className="flex items-start gap-1.5 pl-6">
                          <Avatar name={reply.authorName} size="xs" />
                          <div className="flex-1 min-w-0">
                            <div className="flex items-center gap-1">
                              <span className="font-semibold text-[11px]">{reply.authorName}</span>
                              <span className="text-muted-foreground text-[10px] ml-auto">{formatTime(reply.createdAt)}</span>
                            </div>
                            {replyQuote && (
                              <div className="text-[10px] text-muted-foreground/60 italic border-l border-amber-400 pl-1 line-clamp-1">
                                {replyQuote}
                              </div>
                            )}
                            <div className="text-[11px] leading-snug">
                              <CommentText content={reply.content} />
                            </div>
                          </div>
                        </div>
                      </div>
                    );
                  })}
                  {isExpanded && replies.length > COLLAPSE_THRESHOLD && (
                    <div className="px-2 pb-1">
                      <button
                        className="text-[10px] text-muted-foreground hover:text-foreground pl-6"
                        onClick={(e) => { e.stopPropagation(); toggleExpanded(id); }}
                      >
                        Show less
                      </button>
                    </div>
                  )}
                </>
              )}

              {/* Actions */}
              {isActive && !readOnly && (
                <div className="border-t px-2 py-1.5 bg-gray-50 dark:bg-gray-800/50">
                  {isReplying ? (
                    <div className="space-y-1.5 pl-6">
                      <div className="flex gap-1.5">
                        <Avatar name="You" size="xs" />
                        <Textarea
                          value={replyContent}
                          onChange={(e) => setReplyContent(e.target.value)}
                          placeholder="Reply..."
                          className="flex-1 min-h-[36px] text-[11px] resize-none p-1.5"
                          autoFocus
                          onClick={(e) => e.stopPropagation()}
                        />
                      </div>
                      <div className="flex justify-end gap-1">
                        <Button size="sm" variant="ghost" className="h-5 text-[10px] px-2" onClick={(e) => { e.stopPropagation(); setReplyingTo(null); setReplyContent(""); }}>
                          Cancel
                        </Button>
                        <Button size="sm" className="h-5 text-[10px] px-2" onClick={(e) => { e.stopPropagation(); handleAddReply(id); }} disabled={!replyContent.trim()}>
                          Reply
                        </Button>
                      </div>
                    </div>
                  ) : (
                    <div className="flex items-center gap-2 text-[10px] pl-6">
                      <button className="text-muted-foreground hover:text-foreground font-medium" onClick={(e) => { e.stopPropagation(); setReplyingTo(id); }}>
                        Reply
                      </button>
                      <button className="text-muted-foreground hover:text-foreground flex items-center gap-0.5 ml-auto" onClick={(e) => { e.stopPropagation(); handleResolve(rootComment._id); }}>
                        <Check className="h-3 w-3" /> Resolve
                      </button>
                    </div>
                  )}
                </div>
              )}
            </div>
          );
        })}

        {/* New comment form */}
        {newComment && !readOnly && (
          <div className="absolute left-0 right-0 bg-white dark:bg-gray-900 rounded border shadow-lg p-2 z-20 text-[12px]" style={{ top: newComment.top }}>
            <div className="text-[10px] text-muted-foreground/60 italic border-l-2 border-amber-400 pl-1.5 mb-1.5 line-clamp-2">
              "{newComment.selectedText}"
            </div>
            <div className="flex gap-1.5">
              <Avatar name="You" size="xs" />
              <Textarea
                value={newCommentContent}
                onChange={(e) => setNewCommentContent(e.target.value)}
                placeholder="Add comment..."
                className="flex-1 min-h-[40px] text-[11px] resize-none p-1.5"
                autoFocus
              />
            </div>
            <div className="flex justify-end gap-1 mt-1.5">
              <Button size="sm" variant="ghost" className="h-5 text-[10px] px-2" onClick={() => { setNewComment(null); setNewCommentContent(""); }}>
                Cancel
              </Button>
              <Button size="sm" className="h-5 text-[10px] px-2" onClick={handleAddComment} disabled={!newCommentContent.trim()}>
                Comment
              </Button>
            </div>
          </div>
        )}
      </div>

      {/* Mobile comment button and sheet */}
      <Sheet open={mobileSheetOpen} onOpenChange={(open) => {
        setMobileSheetOpen(open);
        // Clear new comment form when closing
        if (!open && newComment) {
          setNewComment(null);
          setNewCommentContent("");
        }
      }}>
        {/* Only show floating button if there are comments */}
        {threads.length > 0 && (
          <SheetTrigger asChild>
            <button className="lg:hidden fixed bottom-4 right-4 z-40 flex items-center gap-1.5 bg-primary text-primary-foreground px-3 py-2 rounded-full shadow-lg text-sm font-medium">
              <MessageSquare className="h-4 w-4" />
              {threads.length} {threads.length === 1 ? 'comment' : 'comments'}
            </button>
          </SheetTrigger>
        )}
        <SheetContent side="bottom" className="h-[80vh] overflow-hidden flex flex-col">
          <SheetHeader className="shrink-0">
            <SheetTitle className="flex items-center gap-2">
              <MessageSquare className="h-5 w-5" />
              {newComment ? "New Comment" : `Inline Comments (${threads.length})`}
            </SheetTitle>
          </SheetHeader>
          <div className="flex-1 overflow-y-auto mt-4 space-y-4">
            {/* New comment form on mobile */}
            {newComment && !readOnly && (
              <div className="border rounded-lg bg-card p-3 ring-2 ring-primary/20">
                <div className="text-xs text-muted-foreground/70 italic border-l-2 border-amber-400 pl-2 mb-3 line-clamp-3">
                  "{newComment.selectedText}"
                </div>
                <div className="flex gap-2">
                  <Avatar name="You" />
                  <div className="flex-1 space-y-2">
                    <Textarea
                      value={newCommentContent}
                      onChange={(e) => setNewCommentContent(e.target.value)}
                      placeholder="Add your comment..."
                      className="min-h-[80px] text-sm resize-none"
                      autoFocus
                    />
                    <div className="flex justify-end gap-2">
                      <Button 
                        size="sm" 
                        variant="ghost" 
                        onClick={() => { 
                          setNewComment(null); 
                          setNewCommentContent(""); 
                          if (threads.length === 0) setMobileSheetOpen(false);
                        }}
                      >
                        Cancel
                      </Button>
                      <Button 
                        size="sm" 
                        onClick={async () => {
                          await handleAddComment();
                          if (threads.length === 0) {
                            // Keep sheet open to show the new comment
                          }
                        }} 
                        disabled={!newCommentContent.trim()}
                      >
                        Comment
                      </Button>
                    </div>
                  </div>
                </div>
              </div>
            )}
            
            {/* Existing threads */}
              {threads.map((thread) => {
                const { id, rootComment, replies } = thread;
                const isExpanded = expandedThreads.has(id);
                const quotedText = getQuotedText(rootComment.content);
                const shouldCollapse = replies.length > COLLAPSE_THRESHOLD && !isExpanded;

                return (
                  <div key={id} className="border rounded-lg bg-card">
                    {/* Root comment */}
                    <div className="p-3">
                      <div className="flex items-start gap-2">
                        <Avatar name={rootComment.authorName} />
                        <div className="flex-1 min-w-0">
                          <div className="flex items-center gap-1.5 flex-wrap">
                            <span className="font-semibold text-sm">{rootComment.authorName}</span>
                            <TypeIcon type={rootComment.type} />
                            <span className="text-muted-foreground text-xs ml-auto">{formatTime(rootComment.createdAt)}</span>
                          </div>
                          {quotedText && (
                            <div className="text-xs text-muted-foreground/70 italic border-l-2 border-amber-400 pl-2 my-1.5 line-clamp-2">
                              {quotedText}
                            </div>
                          )}
                          <div className="text-sm leading-relaxed mt-1">
                            <CommentText content={rootComment.content} />
                          </div>
                        </div>
                      </div>
                    </div>

                    {/* Replies */}
                    {replies.length > 0 && (
                      <>
                        <div className="border-t border-dashed mx-3" />
                        
                        {shouldCollapse ? (
                          <button
                            className="w-full px-3 py-2 text-sm text-muted-foreground hover:text-foreground flex items-center gap-2"
                            onClick={() => toggleExpanded(id)}
                          >
                            <div className="flex -space-x-1.5">
                              {getParticipants(rootComment, replies).slice(0, 3).map((name, i) => (
                                <Avatar key={i} name={name} size="xs" />
                              ))}
                            </div>
                            {replies.length} replies
                            <ChevronDown className="h-4 w-4 ml-auto" />
                          </button>
                        ) : (
                          <div className="py-2">
                            {replies.map((reply) => {
                              const replyQuote = getQuotedText(reply.content);
                              return (
                                <div key={reply._id} className="px-3 py-2">
                                  <div className="flex items-start gap-2 pl-6">
                                    <Avatar name={reply.authorName} size="xs" />
                                    <div className="flex-1 min-w-0">
                                      <div className="flex items-center gap-1.5">
                                        <span className="font-semibold text-xs">{reply.authorName}</span>
                                        <span className="text-muted-foreground text-xs ml-auto">{formatTime(reply.createdAt)}</span>
                                      </div>
                                      {replyQuote && (
                                        <div className="text-xs text-muted-foreground/60 italic border-l border-amber-400 pl-1.5 my-1 line-clamp-1">
                                          {replyQuote}
                                        </div>
                                      )}
                                      <div className="text-sm leading-relaxed">
                                        <CommentText content={reply.content} />
                                      </div>
                                    </div>
                                  </div>
                                </div>
                              );
                            })}
                            {replies.length > COLLAPSE_THRESHOLD && (
                              <button
                                className="px-3 py-1 text-xs text-muted-foreground hover:text-foreground flex items-center gap-1 pl-9"
                                onClick={() => toggleExpanded(id)}
                              >
                                <ChevronUp className="h-3 w-3" />
                                Show less
                              </button>
                            )}
                          </div>
                        )}
                      </>
                    )}

                    {/* Actions and Reply form */}
                    {!readOnly && (
                      <div className="border-t bg-muted/30">
                        {replyingTo === id ? (
                          <div className="p-3 space-y-2">
                            <div className="flex gap-2">
                              <Avatar name="You" size="xs" />
                              <Textarea
                                value={replyContent}
                                onChange={(e) => setReplyContent(e.target.value)}
                                placeholder="Write a reply..."
                                className="flex-1 min-h-[60px] text-sm resize-none"
                                autoFocus
                              />
                            </div>
                            <div className="flex justify-end gap-2">
                              <Button size="sm" variant="ghost" onClick={() => { setReplyingTo(null); setReplyContent(""); }}>
                                Cancel
                              </Button>
                              <Button size="sm" onClick={() => handleAddReply(id)} disabled={!replyContent.trim()}>
                                Reply
                              </Button>
                            </div>
                          </div>
                        ) : (
                          <div className="px-3 py-2 flex items-center gap-3 text-xs">
                            <button
                              className="text-muted-foreground hover:text-foreground font-medium"
                              onClick={() => setReplyingTo(id)}
                            >
                              Reply
                            </button>
                            <button
                              className="text-muted-foreground hover:text-foreground flex items-center gap-1 ml-auto"
                              onClick={() => handleResolve(rootComment._id)}
                            >
                              <Check className="h-3 w-3" /> Resolve
                            </button>
                          </div>
                        )}
                      </div>
                    )}
                  </div>
                );
              })}
            {/* Empty state */}
            {threads.length === 0 && !newComment && (
              <div className="text-center py-8 text-muted-foreground text-sm">
                No inline comments yet. Select text to add one.
              </div>
            )}
            </div>
          </SheetContent>
        </Sheet>
    </>
  );
}
