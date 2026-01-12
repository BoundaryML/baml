"use client";

import { useState, useEffect, useRef, useMemo, useCallback } from "react";
import { LexicalEditor } from "lexical";
import { useMutation } from "convex/react";
import { api } from "../../../convex/_generated/api";
import { Id } from "../../../convex/_generated/dataModel";
import { useUser } from "@/components/providers/user-provider";
import { Button } from "@/components/ui/button";
import { Textarea } from "@/components/ui/textarea";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover";
import { MessageSquarePlus } from "lucide-react";
import { cn } from "@/lib/utils";
import { CommentTypeBadge } from "./comment-type-badge";

interface BlockComment {
  _id: Id<"comments">;
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

interface BlockCommentSidebarProps {
  editor: LexicalEditor | null;
  bepId: Id<"beps">;
  versionId: Id<"bepVersions">;
  pageId?: Id<"bepPages">;
  comments: BlockComment[];
  readOnly?: boolean;
}

interface BlockMarker {
  nodeId: string;
  element: Element;
  top: number;
  commentCount: number;
}

// Generate a stable ID for a block element based on its content and type
function generateBlockId(element: Element, index: number): string {
  const tagName = element.tagName.toLowerCase();
  const text = element.textContent?.slice(0, 50) || "";
  // Create a simple hash from tag + index + text prefix
  return `${tagName}-${index}-${text.replace(/\s+/g, "_").slice(0, 20)}`;
}

// Find a block element that matches an anchor (fuzzy matching)
function findBlockByAnchor(
  blocks: Element[],
  anchor: { nodeId: string; nodeType: string; nodeText: string }
): Element | null {
  // First try exact nodeId match
  for (let i = 0; i < blocks.length; i++) {
    if (generateBlockId(blocks[i], i) === anchor.nodeId) {
      return blocks[i];
    }
  }

  // Fallback: match by type and text content
  for (const block of blocks) {
    const tagName = block.tagName.toLowerCase();
    const text = block.textContent || "";

    // Check if tag matches and text is similar
    if (tagName === anchor.nodeType || tagName.startsWith(anchor.nodeType.charAt(0))) {
      if (text.includes(anchor.nodeText.slice(0, 30)) || anchor.nodeText.includes(text.slice(0, 30))) {
        return block;
      }
    }
  }

  return null;
}

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

export function BlockCommentSidebar({
  editor,
  bepId,
  versionId,
  pageId,
  comments,
  readOnly = false,
}: BlockCommentSidebarProps) {
  const { userId } = useUser();
  const [markers, setMarkers] = useState<BlockMarker[]>([]);
  const [activeNodeId, setActiveNodeId] = useState<string | null>(null);
  const [hoveredNodeId, setHoveredNodeId] = useState<string | null>(null);
  const hoverTimeoutRef = useRef<NodeJS.Timeout | null>(null);
  const [newCommentContent, setNewCommentContent] = useState("");
  const [newCommentType, setNewCommentType] = useState<"discussion" | "concern" | "question">("discussion");
  const containerRef = useRef<HTMLDivElement>(null);
  const addComment = useMutation(api.comments.add);

  // Group comments by nodeId
  const commentsByNode = useMemo(() => {
    const byNode: Record<string, BlockComment[]> = {};
    for (const comment of comments) {
      if (comment.anchor?.nodeId) {
        const nodeId = comment.anchor.nodeId;
        if (!byNode[nodeId]) byNode[nodeId] = [];
        byNode[nodeId].push(comment);
      }
    }
    return byNode;
  }, [comments]);

  // Calculate marker positions when editor content changes
  useEffect(() => {
    if (!editor || !containerRef.current) return;

    const updateMarkers = () => {
      const editorElement = editor.getRootElement();
      const containerRect = containerRef.current?.getBoundingClientRect();
      if (!containerRect || !editorElement) return;

      const newMarkers: BlockMarker[] = [];
      const processedNodeIds = new Set<string>();

      // Get all direct block children of the editor root
      const blockElements = Array.from(editorElement.children).filter(
        (el) => el.nodeType === Node.ELEMENT_NODE
      );

      // First pass: create markers for all blocks
      blockElements.forEach((element, index) => {
        const nodeId = generateBlockId(element, index);
        const rect = element.getBoundingClientRect();
        const top = rect.top - containerRect.top;
        const commentCount = commentsByNode[nodeId]?.filter(c => !c.resolved).length ?? 0;

        // Only add marker if there are comments or we're in interactive mode
        if (commentCount > 0 || !readOnly) {
          newMarkers.push({ nodeId, element, top, commentCount });
          processedNodeIds.add(nodeId);
        }
      });

      // Second pass: find blocks for comments with anchors that don't match generated IDs
      for (const [anchorNodeId, blockComments] of Object.entries(commentsByNode)) {
        if (processedNodeIds.has(anchorNodeId)) continue;

        const unresolvedCount = blockComments.filter(c => !c.resolved).length;
        if (unresolvedCount === 0) continue;

        // Try to find the block by anchor data
        const anchor = blockComments[0]?.anchor;
        if (!anchor) continue;

        const matchedBlock = findBlockByAnchor(blockElements, anchor);
        if (matchedBlock) {
          const rect = matchedBlock.getBoundingClientRect();
          const top = rect.top - containerRect.top;

          // Check if we already have a marker for this block
          const existingMarker = newMarkers.find(m => m.element === matchedBlock);
          if (existingMarker) {
            // Add to existing marker's count (comments migrated from old anchor)
            existingMarker.commentCount += unresolvedCount;
          } else {
            newMarkers.push({
              nodeId: anchorNodeId,
              element: matchedBlock,
              top,
              commentCount: unresolvedCount,
            });
          }
        }
      }

      // Sort by position
      newMarkers.sort((a, b) => a.top - b.top);
      setMarkers(newMarkers);
    };

    // Initial calculation
    updateMarkers();

    // Recalculate on editor updates
    const unregister = editor.registerUpdateListener(() => {
      // Small delay to let DOM update
      requestAnimationFrame(updateMarkers);
    });

    // Recalculate on scroll/resize
    const handleScroll = () => updateMarkers();
    window.addEventListener("scroll", handleScroll, true);
    window.addEventListener("resize", handleScroll);

    return () => {
      unregister();
      window.removeEventListener("scroll", handleScroll, true);
      window.removeEventListener("resize", handleScroll);
    };
  }, [editor, commentsByNode, readOnly]);

  // Clear hover with delay (allows time to move to sidebar button)
  const clearHoverDelayed = useCallback(() => {
    hoverTimeoutRef.current = setTimeout(() => {
      setHoveredNodeId(null);
    }, 150);
  }, []);

  // Cancel pending clear when hovering again
  const cancelHoverClear = useCallback(() => {
    if (hoverTimeoutRef.current) {
      clearTimeout(hoverTimeoutRef.current);
      hoverTimeoutRef.current = null;
    }
  }, []);

  // Track which block is being hovered
  useEffect(() => {
    if (!editor) return;

    const editorElement = editor.getRootElement();
    if (!editorElement) return;

    const handleMouseOver = (e: MouseEvent) => {
      cancelHoverClear();
      const target = e.target as HTMLElement;
      let blockElement: HTMLElement | null = target;
      while (blockElement && blockElement.parentElement !== editorElement) {
        blockElement = blockElement.parentElement;
      }

      if (blockElement && blockElement.parentElement === editorElement) {
        const blockElements = Array.from(editorElement.children);
        const index = blockElements.indexOf(blockElement);
        if (index !== -1) {
          setHoveredNodeId(generateBlockId(blockElement, index));
        }
      }
    };

    const handleMouseLeave = () => {
      clearHoverDelayed();
    };

    editorElement.addEventListener("mouseover", handleMouseOver);
    editorElement.addEventListener("mouseleave", handleMouseLeave);

    return () => {
      editorElement.removeEventListener("mouseover", handleMouseOver);
      editorElement.removeEventListener("mouseleave", handleMouseLeave);
      cancelHoverClear();
    };
  }, [editor, cancelHoverClear, clearHoverDelayed]);

  const handleAddComment = async () => {
    if (!userId || !activeNodeId || !newCommentContent.trim()) return;

    // Find the marker to get element info
    const marker = markers.find(m => m.nodeId === activeNodeId);
    const nodeElement = marker?.element;
    const nodeType = nodeElement?.tagName.toLowerCase() ?? "paragraph";
    const nodeText = nodeElement?.textContent?.slice(0, 100) ?? "";

    try {
      await addComment({
        bepId,
        versionId,
        pageId,
        authorId: userId,
        type: newCommentType,
        content: newCommentContent.trim(),
        anchor: {
          nodeId: activeNodeId,
          nodeType,
          nodeText,
        },
      });
      setNewCommentContent("");
      setNewCommentType("discussion");
    } catch (error) {
      console.error("Failed to add comment:", error);
    }
  };

  const activeComments = activeNodeId ? commentsByNode[activeNodeId] ?? [] : [];

  return (
    <div ref={containerRef} className="absolute right-0 top-0 bottom-0 w-10 -mr-12">
      {markers.map((marker) => (
        <Popover
          key={marker.nodeId}
          open={activeNodeId === marker.nodeId}
          onOpenChange={(open) => {
            setActiveNodeId(open ? marker.nodeId : null);
            if (!open) {
              setNewCommentContent("");
            }
          }}
        >
          <PopoverTrigger asChild>
            <button
              onMouseEnter={cancelHoverClear}
              onMouseLeave={clearHoverDelayed}
              className={cn(
                "absolute right-0 w-8 h-8 rounded-full flex items-center justify-center transition-all duration-150",
                "border border-transparent",
                marker.commentCount > 0
                  ? "bg-amber-100 dark:bg-amber-900/30 text-amber-600 dark:text-amber-400 border-amber-200 dark:border-amber-800"
                  : "bg-muted/50 text-muted-foreground/60 hover:bg-muted hover:text-muted-foreground",
                // Hide buttons with no comments unless this block is hovered or popover is open
                marker.commentCount === 0 && hoveredNodeId !== marker.nodeId && activeNodeId !== marker.nodeId
                  ? "opacity-0 pointer-events-none"
                  : "opacity-100"
              )}
              style={{ top: marker.top }}
            >
              {marker.commentCount > 0 ? (
                <span className="text-xs font-semibold">{marker.commentCount}</span>
              ) : (
                <MessageSquarePlus className="h-4 w-4" />
              )}
            </button>
          </PopoverTrigger>
          <PopoverContent side="bottom" align="end" className="w-80">
            <div className="space-y-3">
              {/* Existing comments */}
              {activeComments.length > 0 && (
                <div className="space-y-2 max-h-60 overflow-y-auto">
                  {activeComments.map((comment) => (
                    <div
                      key={comment._id}
                      className={cn(
                        "p-2 rounded text-sm",
                        comment.resolved ? "bg-muted/50 opacity-60" : "bg-muted"
                      )}
                    >
                      <div className="flex items-center gap-2 text-xs mb-1">
                        <span className="font-medium text-foreground">{comment.authorName}</span>
                        <span className="text-muted-foreground">{formatTime(comment.createdAt)}</span>
                        <span className="ml-auto scale-90 origin-right">
                          <CommentTypeBadge type={comment.type as "discussion" | "concern" | "question"} />
                        </span>
                      </div>
                      <p className="whitespace-pre-wrap">{comment.content}</p>
                    </div>
                  ))}
                </div>
              )}

              {/* Add comment form - shown directly without intermediate button */}
              {!readOnly && (
                <div className="space-y-2">
                  <Select
                    value={newCommentType}
                    onValueChange={(v) => setNewCommentType(v as typeof newCommentType)}
                  >
                    <SelectTrigger className="h-8 text-xs">
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      <SelectItem value="discussion">Discussion</SelectItem>
                      <SelectItem value="concern">Concern</SelectItem>
                      <SelectItem value="question">Question</SelectItem>
                    </SelectContent>
                  </Select>
                  <Textarea
                    value={newCommentContent}
                    onChange={(e) => setNewCommentContent(e.target.value)}
                    placeholder="Add a comment..."
                    className="min-h-[60px] text-sm"
                    autoFocus
                  />
                  <div className="flex justify-end gap-2">
                    <Button
                      size="sm"
                      onClick={handleAddComment}
                      disabled={!newCommentContent.trim()}
                    >
                      Comment
                    </Button>
                  </div>
                </div>
              )}

              {activeComments.length === 0 && readOnly && (
                <p className="text-sm text-muted-foreground text-center py-2">
                  No comments on this block
                </p>
              )}
            </div>
          </PopoverContent>
        </Popover>
      ))}
    </div>
  );
}
