"use client";

import { useState } from "react";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
} from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { isReservedPageSlug } from "@/lib/bep-routes";

interface BepAddPageModalProps {
  open: boolean;
  onClose: () => void;
  onAdd: (title: string, slug: string) => void;
  existingSlugs: string[];
}

function generateSlug(text: string): string {
  return text
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-|-$/g, "");
}

export function BepAddPageModal({
  open,
  onClose,
  onAdd,
  existingSlugs,
}: BepAddPageModalProps) {
  const [title, setTitle] = useState("");
  const [slug, setSlug] = useState("");

  const handleTitleChange = (value: string) => {
    setTitle(value);
    setSlug(generateSlug(value));
  };

  const isSlugTaken = existingSlugs.includes(slug);
  const isReservedSlug = isReservedPageSlug(slug);
  const isValid = title.trim() && slug.trim() && !isSlugTaken && !isReservedSlug;

  const handleAdd = () => {
    if (isValid) {
      onAdd(title.trim(), slug);
      setTitle("");
      setSlug("");
      onClose();
    }
  };

  const handleOpenChange = (isOpen: boolean) => {
    if (!isOpen) {
      setTitle("");
      setSlug("");
      onClose();
    }
  };

  return (
    <Dialog open={open} onOpenChange={handleOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Add New Page</DialogTitle>
        </DialogHeader>

        <div className="space-y-4">
          <div className="space-y-2">
            <Label htmlFor="page-title">Page Title</Label>
            <Input
              id="page-title"
              value={title}
              onChange={(e) => handleTitleChange(e.target.value)}
              placeholder="e.g., Background, Implementation Details"
              autoFocus
            />
          </div>

          <div className="space-y-2">
            <Label htmlFor="page-slug">URL Slug</Label>
            <Input
              id="page-slug"
              value={slug}
              onChange={(e) => setSlug(generateSlug(e.target.value))}
              placeholder="e.g., background"
            />
            {isSlugTaken && (
              <p className="text-sm text-destructive">This slug is already in use</p>
            )}
            {!isSlugTaken && isReservedSlug && (
              <p className="text-sm text-destructive">This slug is reserved for app routes</p>
            )}
          </div>
        </div>

        <DialogFooter>
          <Button variant="outline" onClick={onClose}>
            Cancel
          </Button>
          <Button onClick={handleAdd} disabled={!isValid}>
            Add Page
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
