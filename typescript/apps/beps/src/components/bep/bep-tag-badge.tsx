"use client";

import { Id } from "../../../convex/_generated/dataModel";

interface Tag {
  _id: Id<"tags">;
  name: string;
  color: string;
}

interface BepTagBadgeProps {
  tag: Tag;
  size?: "sm" | "md";
  onClick?: () => void;
  removable?: boolean;
  onRemove?: () => void;
}

export function BepTagBadge({
  tag,
  size = "md",
  onClick,
  removable,
  onRemove,
}: BepTagBadgeProps) {
  const sizeClasses = size === "sm" 
    ? "px-1.5 py-0.5 text-[10px]" 
    : "px-2 py-0.5 text-xs";

  return (
    <span
      className={`inline-flex items-center gap-1 rounded-full font-medium text-white ${tag.color} ${sizeClasses} ${
        onClick ? "cursor-pointer hover:opacity-80" : ""
      }`}
      onClick={onClick}
    >
      {tag.name}
      {removable && onRemove && (
        <button
          type="button"
          onClick={(e) => {
            e.stopPropagation();
            onRemove();
          }}
          className="ml-0.5 hover:bg-white/20 rounded-full p-0.5"
        >
          <svg className="h-3 w-3" viewBox="0 0 12 12" fill="none" xmlns="http://www.w3.org/2000/svg">
            <path d="M3 3L9 9M9 3L3 9" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" />
          </svg>
        </button>
      )}
    </span>
  );
}

interface BepTagListProps {
  tags: Tag[];
  size?: "sm" | "md";
  maxDisplay?: number;
  onTagClick?: (tag: Tag) => void;
}

export function BepTagList({
  tags,
  size = "md",
  maxDisplay,
  onTagClick,
}: BepTagListProps) {
  if (!tags || tags.length === 0) return null;

  const displayTags = maxDisplay ? tags.slice(0, maxDisplay) : tags;
  const remainingCount = maxDisplay ? Math.max(0, tags.length - maxDisplay) : 0;

  return (
    <div className="flex flex-wrap gap-1">
      {displayTags.map((tag) => (
        <BepTagBadge
          key={tag._id}
          tag={tag}
          size={size}
          onClick={onTagClick ? () => onTagClick(tag) : undefined}
        />
      ))}
      {remainingCount > 0 && (
        <span className={`inline-flex items-center rounded-full bg-muted text-muted-foreground ${
          size === "sm" ? "px-1.5 py-0.5 text-[10px]" : "px-2 py-0.5 text-xs"
        }`}>
          +{remainingCount}
        </span>
      )}
    </div>
  );
}
