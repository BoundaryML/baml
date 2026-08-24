"use client";

import { useState } from "react";
import { useMutation } from "convex/react";
import { api } from "../../../convex/_generated/api";
import { Id } from "../../../convex/_generated/dataModel";
import { Button } from "@/components/ui/button";
import { Star } from "lucide-react";

interface BepGoodReferenceToggleProps {
  bepId: Id<"beps">;
  isGoodReference: boolean;
  canEdit?: boolean;
}

export function BepGoodReferenceToggle({
  bepId,
  isGoodReference,
  canEdit = true,
}: BepGoodReferenceToggleProps) {
  const [isUpdating, setIsUpdating] = useState(false);
  const toggleGoodReference = useMutation(api.beps.toggleGoodReference);

  const handleToggle = async () => {
    if (!canEdit) return;

    setIsUpdating(true);
    try {
      await toggleGoodReference({ id: bepId });
    } catch (error) {
      console.error("Failed to toggle good reference:", error);
    } finally {
      setIsUpdating(false);
    }
  };

  return (
    <Button
      variant={isGoodReference ? "default" : "outline"}
      size="sm"
      onClick={handleToggle}
      disabled={isUpdating || !canEdit}
      title={isGoodReference ? "Good reference BEP" : "Mark as good reference"}
      className={
        isGoodReference ? "bg-amber-500 hover:bg-amber-600 text-white" : ""
      }
    >
      <Star className={`h-4 w-4 ${isGoodReference ? "fill-current" : ""}`} />
    </Button>
  );
}
