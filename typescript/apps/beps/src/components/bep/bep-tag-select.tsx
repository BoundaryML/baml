"use client";

import { useState } from "react";
import { useQuery, useMutation } from "convex/react";
import { api } from "../../../convex/_generated/api";
import { Id } from "../../../convex/_generated/dataModel";
import { BepTagBadge, BepTagList } from "./bep-tag-badge";
import { Button } from "@/components/ui/button";
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover";
import { Checkbox } from "@/components/ui/checkbox";
import { Tags, ChevronDown } from "lucide-react";

interface Tag {
  _id: Id<"tags">;
  name: string;
  color: string;
  description?: string;
}

interface BepTagSelectProps {
  bepId: Id<"beps">;
  currentTags: Tag[];
  userId: Id<"users">;
  readOnly?: boolean;
}

export function BepTagSelect({
  bepId,
  currentTags,
  userId,
  readOnly = false,
}: BepTagSelectProps) {
  const [open, setOpen] = useState(false);
  const [isUpdating, setIsUpdating] = useState(false);

  const allTags = useQuery(api.tags.list);
  const setTagsForBep = useMutation(api.tags.setTagsForBep);

  const currentTagIds = new Set(currentTags.map((t) => t._id));

  const handleToggleTag = async (tagId: Id<"tags">) => {
    if (isUpdating) return;
    setIsUpdating(true);

    try {
      const newTagIds = currentTagIds.has(tagId)
        ? currentTags.filter((t) => t._id !== tagId).map((t) => t._id)
        : [...currentTags.map((t) => t._id), tagId];

      await setTagsForBep({
        bepId,
        tagIds: newTagIds,
        userId,
      });
    } catch (err) {
      console.error("Failed to update tags:", err);
    } finally {
      setIsUpdating(false);
    }
  };

  if (readOnly) {
    if (currentTags.length === 0) return null;
    return <BepTagList tags={currentTags} size="sm" />;
  }

  return (
    <div className="flex items-center gap-2">
      <BepTagList tags={currentTags} size="sm" />
      <Popover open={open} onOpenChange={setOpen}>
        <PopoverTrigger asChild>
          <Button variant="ghost" size="sm" className="h-6 px-2 text-xs">
            <Tags className="h-3 w-3 mr-1" />
            {currentTags.length === 0 ? "Add tags" : "Edit"}
            <ChevronDown className="h-3 w-3 ml-1" />
          </Button>
        </PopoverTrigger>
        <PopoverContent className="w-64 p-2" align="start">
          <div className="space-y-1">
            {!allTags ? (
              <p className="text-sm text-muted-foreground p-2">Loading...</p>
            ) : allTags.length === 0 ? (
              <p className="text-sm text-muted-foreground p-2">
                No tags available. Team members can create tags in the tag management page.
              </p>
            ) : (
              allTags.map((tag) => (
                <div
                  key={tag._id}
                  className="flex items-center gap-2 p-2 rounded hover:bg-muted cursor-pointer"
                  onClick={() => handleToggleTag(tag._id)}
                >
                  <Checkbox
                    checked={currentTagIds.has(tag._id)}
                    disabled={isUpdating}
                    className="pointer-events-none"
                  />
                  <BepTagBadge tag={tag} size="sm" />
                  {tag.description && (
                    <span className="text-xs text-muted-foreground truncate flex-1">
                      {tag.description}
                    </span>
                  )}
                </div>
              ))
            )}
          </div>
        </PopoverContent>
      </Popover>
    </div>
  );
}

interface BepTagDisplayProps {
  tags: Tag[];
  size?: "sm" | "md";
  maxDisplay?: number;
}

export function BepTagDisplay({ tags, size = "sm", maxDisplay }: BepTagDisplayProps) {
  if (!tags || tags.length === 0) return null;
  return <BepTagList tags={tags} size={size} maxDisplay={maxDisplay} />;
}
