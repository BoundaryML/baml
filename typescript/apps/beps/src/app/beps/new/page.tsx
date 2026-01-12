"use client";

import { useState, useEffect } from "react";
import { useRouter } from "next/navigation";
import Link from "next/link";
import { useMutation, useQuery } from "convex/react";
import { api } from "../../../../convex/_generated/api";
import { useUser } from "@/components/providers/user-provider";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { Label } from "@/components/ui/label";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { ArrowLeft } from "lucide-react";

export default function NewBepPage() {
  const router = useRouter();
  const { user, userId, isLoading: userLoading } = useUser();

  const [title, setTitle] = useState("");
  const [content, setContent] = useState("");
  const [isSubmitting, setIsSubmitting] = useState(false);

  const nextNumber = useQuery(api.beps.getNextNumber, {});
  const createBep = useMutation(api.beps.create);

  useEffect(() => {
    if (!userLoading && !userId) {
      router.push("/login");
    }
  }, [userLoading, userId, router]);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!title.trim() || !userId || nextNumber === undefined) return;

    setIsSubmitting(true);
    try {
      await createBep({
        number: nextNumber,
        title: title.trim(),
        shepherds: [userId],
        content: content.trim() || undefined,
        userId,
      });
      router.push(`/beps/${nextNumber}`);
    } catch (error) {
      console.error("Failed to create BEP:", error);
      setIsSubmitting(false);
    }
  };

  if (userLoading || !user) {
    return null;
  }

  return (
    <div className="min-h-screen bg-background">
      <header className="border-b">
        <div className="max-w-4xl mx-auto px-4 py-4">
          <Link
            href="/"
            className="flex items-center gap-2 text-muted-foreground hover:text-foreground"
          >
            <ArrowLeft className="h-4 w-4" />
            Back
          </Link>
        </div>
      </header>

      <main className="max-w-4xl mx-auto px-4 py-8">
        <Card>
          <CardHeader>
            <CardTitle>
              Create New BEP
              {nextNumber !== undefined && (
                <span className="text-muted-foreground font-mono ml-2">
                  (BEP-{String(nextNumber).padStart(3, "0")})
                </span>
              )}
            </CardTitle>
          </CardHeader>
          <CardContent>
            <form onSubmit={handleSubmit} className="space-y-6">
              <div className="space-y-2">
                <Label htmlFor="title">Title *</Label>
                <Input
                  id="title"
                  placeholder="e.g., Exception Handling"
                  value={title}
                  onChange={(e) => setTitle(e.target.value)}
                  disabled={isSubmitting}
                  required
                />
              </div>

              <div className="space-y-2">
                <Label htmlFor="content">Content</Label>
                <Textarea
                  id="content"
                  placeholder={`Write your proposal here...

Suggested structure:
## Summary
A brief TL;DR of the proposal...

## Motivation
Why is this change needed? What problem does it solve?

## Proposal
Describe the proposed solution in detail...

## Alternatives
What other approaches were considered and why were they rejected?`}
                  value={content}
                  onChange={(e) => setContent(e.target.value)}
                  disabled={isSubmitting}
                  rows={20}
                  className="font-mono"
                />
                <p className="text-xs text-muted-foreground">
                  Supports Markdown formatting. You can add additional pages
                  after creation.
                </p>
              </div>

              <div className="flex gap-4">
                <Button type="submit" disabled={!title.trim() || isSubmitting}>
                  {isSubmitting ? "Creating..." : "Create BEP"}
                </Button>
                <Link href="/">
                  <Button type="button" variant="outline">
                    Cancel
                  </Button>
                </Link>
              </div>
            </form>
          </CardContent>
        </Card>
      </main>
    </div>
  );
}
