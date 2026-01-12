import { Badge } from "@/components/ui/badge";

type CommentType =
  | "discussion"
  | "concern"
  | "question";

interface CommentTypeBadgeProps {
  type: CommentType;
}

export function CommentTypeBadge({ type }: CommentTypeBadgeProps) {
  return (
    <Badge variant={type} className="capitalize text-xs">
      {type}
    </Badge>
  );
}
