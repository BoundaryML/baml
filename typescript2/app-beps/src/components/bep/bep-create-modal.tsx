"use client";

import { useState } from "react";
import { useRouter } from "next/navigation";
import { useMutation, useQuery } from "convex/react";
import { api } from "../../../convex/_generated/api";
import { Id } from "../../../convex/_generated/dataModel";
import { AlertCircle, Loader2, Plus } from "lucide-react";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";

interface BepCreateModalProps {
  userId: Id<"users"> | null;
}

export function BepCreateModal({ userId }: BepCreateModalProps) {
  const router = useRouter();
  const nextNumber = useQuery(api.beps.getNextNumber, {});
  const createBep = useMutation(api.beps.create);

  const [open, setOpen] = useState(false);
  const [title, setTitle] = useState("");
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const canCreate = title.trim().length > 0 && !!userId && nextNumber !== undefined && !isSubmitting;

  const handleOpenChange = (isOpen: boolean) => {
    setOpen(isOpen);
    if (!isOpen) {
      setTitle("");
      setError(null);
      setIsSubmitting(false);
    }
  };

  const handleCreate = async (event: React.FormEvent<HTMLFormElement>) => {
    event.preventDefault();

    if (!userId || nextNumber === undefined || !title.trim()) {
      return;
    }

    setIsSubmitting(true);
    setError(null);

    const bepNumber = nextNumber;

    try {
      const created = await createBep({
        number: bepNumber,
        title: title.trim(),
        shepherds: [userId],
        content: "",
        userId,
      });

      setOpen(false);
      router.push(`/beps/${created.number}`);
    } catch (createError) {
      setError(
        createError instanceof Error
          ? createError.message
          : "Failed to create BEP"
      );
      setIsSubmitting(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={handleOpenChange}>
      <DialogTrigger asChild>
        <Button>
          <Plus className="h-4 w-4 mr-2" />
          New BEP
        </Button>
      </DialogTrigger>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>Create New BEP</DialogTitle>
          <DialogDescription>
            Start with a title. After creation, you can import files or edit the
            README directly.
          </DialogDescription>
        </DialogHeader>

        <form onSubmit={handleCreate} className="space-y-4">
          {error && (
            <Alert variant="destructive">
              <AlertCircle className="h-4 w-4" />
              <AlertDescription>{error}</AlertDescription>
            </Alert>
          )}

          <div className="space-y-2">
            <Label htmlFor="bep-title">BEP Title</Label>
            <Input
              id="bep-title"
              value={title}
              onChange={(e) => setTitle(e.target.value)}
              placeholder="e.g., Exception Handling"
              autoFocus
              disabled={isSubmitting}
              required
            />
          </div>

          {nextNumber !== undefined && (
            <p className="text-xs text-muted-foreground">
              This will create BEP-{String(nextNumber).padStart(3, "0")} as an
              empty draft.
            </p>
          )}

          <DialogFooter>
            <Button
              type="button"
              variant="outline"
              onClick={() => handleOpenChange(false)}
              disabled={isSubmitting}
            >
              Cancel
            </Button>
            <Button type="submit" disabled={!canCreate}>
              {isSubmitting ? (
                <>
                  <Loader2 className="h-4 w-4 mr-2 animate-spin" />
                  Creating...
                </>
              ) : (
                "Create BEP"
              )}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
