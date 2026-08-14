import { Badge } from "@/components/ui/badge";

type BepStatus =
  | "draft"
  | "proposed"
  | "pending"
  | "accepted"
  | "implemented"
  | "rejected"
  | "superseded";

const STATUS_DISPLAY_LABELS: Record<BepStatus, string> = {
  draft: "Slop",
  proposed: "Proposed",
  pending: "Pending",
  accepted: "Accepted",
  implemented: "Implemented",
  rejected: "Rejected",
  superseded: "Superseded",
};

interface BepStatusBadgeProps {
  status: BepStatus;
}

export function BepStatusBadge({ status }: BepStatusBadgeProps) {
  return (
    <Badge variant={status}>
      {STATUS_DISPLAY_LABELS[status]}
    </Badge>
  );
}
