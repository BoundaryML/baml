import { Badge } from "@/components/ui/badge";

type BepStatus =
  | "draft"
  | "proposed"
  | "accepted"
  | "implemented"
  | "rejected"
  | "superseded";

interface BepStatusBadgeProps {
  status: BepStatus;
}

export function BepStatusBadge({ status }: BepStatusBadgeProps) {
  return (
    <Badge variant={status} className="capitalize">
      {status}
    </Badge>
  );
}
