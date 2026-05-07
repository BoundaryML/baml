"use client";

import { useEffect, useState } from "react";
import { useRouter } from "next/navigation";
import { useQuery, useMutation } from "convex/react";
import { api } from "../../../convex/_generated/api";
import { Id } from "../../../convex/_generated/dataModel";
import { useUser } from "@/components/providers/user-provider";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Skeleton } from "@/components/ui/skeleton";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
import { Label } from "@/components/ui/label";
import { Textarea } from "@/components/ui/textarea";
import { ArrowLeft, Tags, Plus, Pencil, Trash2 } from "lucide-react";

const TAG_COLORS = [
  { name: "Gray", value: "bg-gray-500" },
  { name: "Red", value: "bg-red-500" },
  { name: "Orange", value: "bg-orange-500" },
  { name: "Amber", value: "bg-amber-500" },
  { name: "Yellow", value: "bg-yellow-500" },
  { name: "Lime", value: "bg-lime-500" },
  { name: "Green", value: "bg-green-500" },
  { name: "Emerald", value: "bg-emerald-500" },
  { name: "Teal", value: "bg-teal-500" },
  { name: "Cyan", value: "bg-cyan-500" },
  { name: "Sky", value: "bg-sky-500" },
  { name: "Blue", value: "bg-blue-500" },
  { name: "Indigo", value: "bg-indigo-500" },
  { name: "Violet", value: "bg-violet-500" },
  { name: "Purple", value: "bg-purple-500" },
  { name: "Fuchsia", value: "bg-fuchsia-500" },
  { name: "Pink", value: "bg-pink-500" },
  { name: "Rose", value: "bg-rose-500" },
];

interface TagRecord {
  _id: Id<"tags">;
  name: string;
  description?: string;
  color: string;
  bepCount: number;
  createdAt: number;
  updatedAt: number;
}

function TagBadge({ name, color }: { name: string; color: string }) {
  return (
    <span
      className={`inline-flex items-center px-2.5 py-0.5 rounded-full text-xs font-medium text-white ${color}`}
    >
      {name}
    </span>
  );
}

function ColorPicker({
  value,
  onChange,
}: {
  value: string;
  onChange: (color: string) => void;
}) {
  return (
    <div className="grid grid-cols-6 gap-2">
      {TAG_COLORS.map((color) => (
        <button
          key={color.value}
          type="button"
          className={`w-8 h-8 rounded-full ${color.value} ${
            value === color.value
              ? "ring-2 ring-offset-2 ring-primary"
              : "hover:ring-2 hover:ring-offset-2 hover:ring-gray-300"
          }`}
          onClick={() => onChange(color.value)}
          title={color.name}
        />
      ))}
    </div>
  );
}

function CreateTagDialog({
  userId,
  onSuccess,
}: {
  userId: Id<"users">;
  onSuccess?: () => void;
}) {
  const [open, setOpen] = useState(false);
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [color, setColor] = useState("bg-blue-500");
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const createTag = useMutation(api.tags.create);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!name.trim()) {
      setError("Name is required");
      return;
    }

    setIsSubmitting(true);
    setError(null);

    try {
      await createTag({
        name: name.trim(),
        description: description.trim() || undefined,
        color,
        requesterId: userId,
      });
      setOpen(false);
      setName("");
      setDescription("");
      setColor("bg-blue-500");
      onSuccess?.();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to create tag");
    } finally {
      setIsSubmitting(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogTrigger asChild>
        <Button>
          <Plus className="h-4 w-4 mr-2" />
          Create Tag
        </Button>
      </DialogTrigger>
      <DialogContent>
        <form onSubmit={handleSubmit}>
          <DialogHeader>
            <DialogTitle>Create New Tag</DialogTitle>
            <DialogDescription>
              Create a new tag to categorize BEPs
            </DialogDescription>
          </DialogHeader>
          <div className="space-y-4 py-4">
            <div className="space-y-2">
              <Label htmlFor="name">Name</Label>
              <Input
                id="name"
                value={name}
                onChange={(e) => setName(e.target.value)}
                placeholder="e.g., standard-library, core"
              />
            </div>
            <div className="space-y-2">
              <Label htmlFor="description">Description (optional)</Label>
              <Textarea
                id="description"
                value={description}
                onChange={(e) => setDescription(e.target.value)}
                placeholder="Brief description of when to use this tag"
              />
            </div>
            <div className="space-y-2">
              <Label>Color</Label>
              <div className="flex items-center gap-4">
                <TagBadge name={name || "Preview"} color={color} />
              </div>
              <ColorPicker value={color} onChange={setColor} />
            </div>
            {error && (
              <p className="text-sm text-destructive">{error}</p>
            )}
          </div>
          <DialogFooter>
            <Button
              type="button"
              variant="outline"
              onClick={() => setOpen(false)}
            >
              Cancel
            </Button>
            <Button type="submit" disabled={isSubmitting}>
              {isSubmitting ? "Creating..." : "Create Tag"}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}

function EditTagDialog({
  tag,
  userId,
  onSuccess,
}: {
  tag: TagRecord;
  userId: Id<"users">;
  onSuccess?: () => void;
}) {
  const [open, setOpen] = useState(false);
  const [name, setName] = useState(tag.name);
  const [description, setDescription] = useState(tag.description || "");
  const [color, setColor] = useState(tag.color);
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const updateTag = useMutation(api.tags.update);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!name.trim()) {
      setError("Name is required");
      return;
    }

    setIsSubmitting(true);
    setError(null);

    try {
      await updateTag({
        id: tag._id,
        name: name.trim(),
        description: description.trim() || undefined,
        color,
        requesterId: userId,
      });
      setOpen(false);
      onSuccess?.();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to update tag");
    } finally {
      setIsSubmitting(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogTrigger asChild>
        <Button variant="ghost" size="sm">
          <Pencil className="h-4 w-4" />
        </Button>
      </DialogTrigger>
      <DialogContent>
        <form onSubmit={handleSubmit}>
          <DialogHeader>
            <DialogTitle>Edit Tag</DialogTitle>
            <DialogDescription>
              Update tag details
            </DialogDescription>
          </DialogHeader>
          <div className="space-y-4 py-4">
            <div className="space-y-2">
              <Label htmlFor="edit-name">Name</Label>
              <Input
                id="edit-name"
                value={name}
                onChange={(e) => setName(e.target.value)}
              />
            </div>
            <div className="space-y-2">
              <Label htmlFor="edit-description">Description (optional)</Label>
              <Textarea
                id="edit-description"
                value={description}
                onChange={(e) => setDescription(e.target.value)}
              />
            </div>
            <div className="space-y-2">
              <Label>Color</Label>
              <div className="flex items-center gap-4">
                <TagBadge name={name || "Preview"} color={color} />
              </div>
              <ColorPicker value={color} onChange={setColor} />
            </div>
            {error && (
              <p className="text-sm text-destructive">{error}</p>
            )}
          </div>
          <DialogFooter>
            <Button
              type="button"
              variant="outline"
              onClick={() => setOpen(false)}
            >
              Cancel
            </Button>
            <Button type="submit" disabled={isSubmitting}>
              {isSubmitting ? "Saving..." : "Save Changes"}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}

function DeleteTagDialog({
  tag,
  userId,
  onSuccess,
}: {
  tag: TagRecord;
  userId: Id<"users">;
  onSuccess?: () => void;
}) {
  const [open, setOpen] = useState(false);
  const [isDeleting, setIsDeleting] = useState(false);

  const removeTag = useMutation(api.tags.remove);

  const handleDelete = async () => {
    setIsDeleting(true);
    try {
      await removeTag({
        id: tag._id,
        requesterId: userId,
      });
      setOpen(false);
      onSuccess?.();
    } catch (err) {
      console.error("Failed to delete tag:", err);
    } finally {
      setIsDeleting(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogTrigger asChild>
        <Button variant="ghost" size="sm" className="text-destructive hover:text-destructive">
          <Trash2 className="h-4 w-4" />
        </Button>
      </DialogTrigger>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Delete Tag</DialogTitle>
          <DialogDescription>
            Are you sure you want to delete the tag "{tag.name}"?
            {tag.bepCount > 0 && (
              <span className="block mt-2 text-amber-600">
                This tag is currently applied to {tag.bepCount} BEP{tag.bepCount !== 1 ? "s" : ""}.
                Deleting it will remove it from all BEPs.
              </span>
            )}
          </DialogDescription>
        </DialogHeader>
        <DialogFooter>
          <Button
            type="button"
            variant="outline"
            onClick={() => setOpen(false)}
          >
            Cancel
          </Button>
          <Button
            type="button"
            variant="destructive"
            onClick={handleDelete}
            disabled={isDeleting}
          >
            {isDeleting ? "Deleting..." : "Delete Tag"}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

function TagRow({
  tag,
  userId,
}: {
  tag: TagRecord;
  userId: Id<"users">;
}) {
  return (
    <div className="flex items-center justify-between p-4 border rounded-lg bg-card">
      <div className="flex items-center gap-4">
        <TagBadge name={tag.name} color={tag.color} />
        <div>
          {tag.description && (
            <p className="text-sm text-muted-foreground">{tag.description}</p>
          )}
          <p className="text-xs text-muted-foreground">
            Used in {tag.bepCount} BEP{tag.bepCount !== 1 ? "s" : ""}
          </p>
        </div>
      </div>
      <div className="flex items-center gap-2">
        <EditTagDialog tag={tag} userId={userId} />
        <DeleteTagDialog tag={tag} userId={userId} />
      </div>
    </div>
  );
}

export default function TagsPage() {
  const { user, userId, isLoading: userLoading, hasManagementPermissions } = useUser();
  const router = useRouter();

  const tags = useQuery(
    api.tags.listWithCounts,
    userId && hasManagementPermissions ? {} : "skip"
  );

  useEffect(() => {
    if (!userLoading && !userId) {
      router.push("/login");
      return;
    }

    if (!userLoading && userId && !hasManagementPermissions) {
      router.push("/");
    }
  }, [userLoading, userId, hasManagementPermissions, router]);

  if (userLoading || !hasManagementPermissions) {
    return (
      <div className="min-h-screen bg-background p-8">
        <div className="max-w-4xl mx-auto space-y-4">
          <Skeleton className="h-12 w-64" />
          <Skeleton className="h-8 w-96" />
          <div className="space-y-3">
            <Skeleton className="h-20 w-full" />
            <Skeleton className="h-20 w-full" />
            <Skeleton className="h-20 w-full" />
          </div>
        </div>
      </div>
    );
  }

  if (!user || !userId) {
    return null;
  }

  return (
    <div className="min-h-screen bg-background">
      <header className="border-b">
        <div className="max-w-4xl mx-auto px-4 py-4 flex items-center gap-4">
          <Button variant="ghost" size="sm" onClick={() => router.push("/")}>
            <ArrowLeft className="h-4 w-4 mr-2" />
            Back
          </Button>
          <div className="flex items-center gap-2">
            <Tags className="h-5 w-5" />
            <h1 className="text-xl font-bold">Tag Management</h1>
          </div>
        </div>
      </header>

      <main className="max-w-4xl mx-auto px-4 py-8">
        <Card className="mb-8">
          <CardHeader>
            <CardTitle>About Tags</CardTitle>
            <CardDescription>
              Tags help categorize BEPs for easier navigation and filtering
            </CardDescription>
          </CardHeader>
          <CardContent>
            <p className="text-sm text-muted-foreground">
              Create tags like "standard-library", "core", "tooling", "breaking-change", etc.
              to help users find related BEPs quickly. Tags can be applied to any BEP and
              users can filter the BEP list by tags on the home page.
            </p>
          </CardContent>
        </Card>

        <div className="flex items-center justify-between mb-6">
          <h2 className="text-lg font-semibold">All Tags</h2>
          <CreateTagDialog userId={userId} />
        </div>

        {tags === undefined ? (
          <div className="space-y-3">
            <Skeleton className="h-20 w-full" />
            <Skeleton className="h-20 w-full" />
            <Skeleton className="h-20 w-full" />
          </div>
        ) : tags.length === 0 ? (
          <div className="text-center py-12 text-muted-foreground border rounded-lg">
            <Tags className="h-12 w-12 mx-auto mb-4 opacity-50" />
            <p>No tags created yet.</p>
            <p className="text-sm">Create your first tag to start categorizing BEPs.</p>
          </div>
        ) : (
          <div className="space-y-3">
            {tags.map((tag) => (
              <TagRow
                key={tag._id}
                tag={tag as TagRecord}
                userId={userId}
              />
            ))}
          </div>
        )}
      </main>
    </div>
  );
}
