"use client";

import { useState } from "react";
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
import { MessageSquare, AlertCircle, HelpCircle } from "lucide-react";

type CommentType =
  | "discussion"
  | "concern"
  | "question";

const COMMENT_TYPES: {
  value: CommentType;
  label: string;
  icon: typeof MessageSquare;
  description: string;
}[] = [
  {
    value: "discussion",
    label: "Discussion",
    icon: MessageSquare,
    description: "General feedback",
  },
  {
    value: "concern",
    label: "Concern",
    icon: AlertCircle,
    description: "Blocking issue",
  },
  {
    value: "question",
    label: "Question",
    icon: HelpCircle,
    description: "Needs clarification",
  },
];

interface CommentFormProps {
  bepId: Id<"beps">;
  versionId: Id<"bepVersions">;
  pageId?: Id<"bepPages">;
  parentId?: Id<"comments">;
  onSuccess?: () => void;
  onCancel?: () => void;
  compact?: boolean;
}

export function CommentForm({
  bepId,
  versionId,
  pageId,
  parentId,
  onSuccess,
  onCancel,
  compact = false,
}: CommentFormProps) {
  const { userId } = useUser();
  const [content, setContent] = useState("");
  const [type, setType] = useState<CommentType>("discussion");
  const [isSubmitting, setIsSubmitting] = useState(false);

  const addComment = useMutation(api.comments.add);

  const submitComment = async () => {
    if (!content.trim() || !userId || isSubmitting) return;

    setIsSubmitting(true);
    try {
      await addComment({
        bepId,
        versionId,
        pageId,
        parentId,
        authorId: userId,
        type,
        content: content.trim(),
      });
      setContent("");
      setType("discussion");
      onSuccess?.();
    } catch (error) {
      console.error("Failed to add comment:", error);
    } finally {
      setIsSubmitting(false);
    }
  };

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    await submitComment();
  };

  const handleKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    // Submit on Cmd+Enter (Mac) or Ctrl+Enter (Windows/Linux)
    if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
      e.preventDefault();
      submitComment();
    }
  };

  const selectedType = COMMENT_TYPES.find((t) => t.value === type);

  return (
    <form onSubmit={handleSubmit} className="space-y-3">
      <div className="flex gap-2">
        <Select
          value={type}
          onValueChange={(v) => setType(v as CommentType)}
          disabled={isSubmitting}
        >
          <SelectTrigger className={compact ? "w-32" : "w-40"}>
            <SelectValue>
              {selectedType && (
                <span className="flex items-center gap-2">
                  <selectedType.icon className="h-4 w-4" />
                  {selectedType.label}
                </span>
              )}
            </SelectValue>
          </SelectTrigger>
          <SelectContent>
            {COMMENT_TYPES.map((commentType) => (
              <SelectItem key={commentType.value} value={commentType.value}>
                <div className="flex items-center gap-2">
                  <commentType.icon className="h-4 w-4" />
                  <span>{commentType.label}</span>
                  <span className="text-xs text-muted-foreground">
                    - {commentType.description}
                  </span>
                </div>
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </div>

      <Textarea
        value={content}
        onChange={(e) => setContent(e.target.value)}
        onKeyDown={handleKeyDown}
        placeholder={
          parentId
            ? "Write a reply... (Markdown supported)"
            : "Add a comment... (Markdown supported)"
        }
        rows={compact ? 2 : 4}
        disabled={isSubmitting}
        className="resize-none"
      />

      <div className="flex items-center justify-end gap-2">
        {onCancel && (
          <Button
            type="button"
            variant="ghost"
            size={compact ? "sm" : "default"}
            onClick={onCancel}
            disabled={isSubmitting}
          >
            Cancel
          </Button>
        )}
        <Button
          type="submit"
          size={compact ? "sm" : "default"}
          disabled={!content.trim() || isSubmitting}
          title="Submit (⌘+Enter)"
        >
          {isSubmitting ? "Posting..." : parentId ? "Reply" : "Comment"}
        </Button>
      </div>
    </form>
  );
}
