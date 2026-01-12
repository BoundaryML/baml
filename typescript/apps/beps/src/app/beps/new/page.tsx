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
import { ArrowLeft, Plus, X, FileText, ChevronDown, ChevronUp } from "lucide-react";

interface PageDraft {
  slug: string;
  title: string;
  content: string;
}

function generateSlug(text: string): string {
  return text
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-|-$/g, "");
}

export default function NewBepPage() {
  const router = useRouter();
  const { user, userId, isLoading: userLoading } = useUser();

  const [title, setTitle] = useState("");
  const [content, setContent] = useState("");
  const [pages, setPages] = useState<PageDraft[]>([]);
  const [isSubmitting, setIsSubmitting] = useState(false);
  
  // New page form state
  const [showAddPage, setShowAddPage] = useState(false);
  const [newPageTitle, setNewPageTitle] = useState("");
  const [newPageSlug, setNewPageSlug] = useState("");
  const [newPageContent, setNewPageContent] = useState("");
  
  // Track which pages are expanded for editing
  const [expandedPages, setExpandedPages] = useState<Set<number>>(new Set());

  const nextNumber = useQuery(api.beps.getNextNumber, {});
  const createBep = useMutation(api.beps.create);

  useEffect(() => {
    if (!userLoading && !userId) {
      router.push("/login");
    }
  }, [userLoading, userId, router]);

  const existingSlugs = pages.map((p) => p.slug);
  const isNewSlugTaken = existingSlugs.includes(newPageSlug);
  const isNewPageValid = newPageTitle.trim() && newPageSlug.trim() && !isNewSlugTaken;

  const handleNewPageTitleChange = (value: string) => {
    setNewPageTitle(value);
    setNewPageSlug(generateSlug(value));
  };

  const handleAddPage = () => {
    if (!isNewPageValid) return;
    
    setPages([
      ...pages,
      {
        slug: newPageSlug,
        title: newPageTitle.trim(),
        content: newPageContent.trim(),
      },
    ]);
    
    // Reset form
    setNewPageTitle("");
    setNewPageSlug("");
    setNewPageContent("");
    setShowAddPage(false);
  };

  const handleRemovePage = (index: number) => {
    setPages(pages.filter((_, i) => i !== index));
    setExpandedPages((prev) => {
      const next = new Set(prev);
      next.delete(index);
      return next;
    });
  };

  const togglePageExpanded = (index: number) => {
    setExpandedPages((prev) => {
      const next = new Set(prev);
      if (next.has(index)) {
        next.delete(index);
      } else {
        next.add(index);
      }
      return next;
    });
  };

  const updatePageContent = (index: number, newContent: string) => {
    setPages(pages.map((p, i) => (i === index ? { ...p, content: newContent } : p)));
  };

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
        pages: pages.length > 0 ? pages : undefined,
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
                  Supports Markdown formatting. You can add additional pages after creation.
                </p>
              </div>

              {/* Additional Pages Section */}
              <div className="space-y-4">
                <div className="flex items-center justify-between">
                  <Label>Additional Pages</Label>
                  <span className="text-xs text-muted-foreground">
                    {pages.length} page{pages.length !== 1 ? "s" : ""}
                  </span>
                </div>

                {/* List of added pages */}
                {pages.length > 0 && (
                  <div className="space-y-2">
                    {pages.map((page, index) => (
                      <div
                        key={index}
                        className="border rounded-lg overflow-hidden"
                      >
                        <div className="flex items-center justify-between px-3 py-2 bg-muted/50">
                          <button
                            type="button"
                            onClick={() => togglePageExpanded(index)}
                            className="flex items-center gap-2 text-sm font-medium hover:text-foreground flex-1 text-left"
                          >
                            <FileText className="h-4 w-4" />
                            <span>{page.title}</span>
                            <span className="text-muted-foreground font-mono text-xs">
                              /{page.slug}
                            </span>
                            {expandedPages.has(index) ? (
                              <ChevronUp className="h-4 w-4 ml-auto" />
                            ) : (
                              <ChevronDown className="h-4 w-4 ml-auto" />
                            )}
                          </button>
                          <Button
                            type="button"
                            variant="ghost"
                            size="sm"
                            onClick={() => handleRemovePage(index)}
                            disabled={isSubmitting}
                            className="ml-2 h-7 w-7 p-0 text-muted-foreground hover:text-destructive"
                          >
                            <X className="h-4 w-4" />
                          </Button>
                        </div>
                        {expandedPages.has(index) && (
                          <div className="p-3 border-t">
                            <Textarea
                              value={page.content}
                              onChange={(e) => updatePageContent(index, e.target.value)}
                              disabled={isSubmitting}
                              rows={8}
                              className="font-mono text-sm"
                              placeholder="Page content (Markdown supported)..."
                            />
                          </div>
                        )}
                      </div>
                    ))}
                  </div>
                )}

                {/* Add page form */}
                {showAddPage ? (
                  <div className="border rounded-lg p-4 space-y-4 bg-muted/30">
                    <div className="space-y-2">
                      <Label htmlFor="new-page-title">Page Title</Label>
                      <Input
                        id="new-page-title"
                        value={newPageTitle}
                        onChange={(e) => handleNewPageTitleChange(e.target.value)}
                        placeholder="e.g., Background, Implementation Details"
                        disabled={isSubmitting}
                        autoFocus
                      />
                    </div>

                    <div className="space-y-2">
                      <Label htmlFor="new-page-slug">URL Slug</Label>
                      <Input
                        id="new-page-slug"
                        value={newPageSlug}
                        onChange={(e) => setNewPageSlug(generateSlug(e.target.value))}
                        placeholder="e.g., background"
                        disabled={isSubmitting}
                      />
                      {isNewSlugTaken && (
                        <p className="text-sm text-destructive">
                          This slug is already in use
                        </p>
                      )}
                    </div>

                    <div className="space-y-2">
                      <Label htmlFor="new-page-content">Content (optional)</Label>
                      <Textarea
                        id="new-page-content"
                        value={newPageContent}
                        onChange={(e) => setNewPageContent(e.target.value)}
                        placeholder="Write your page content here (Markdown supported)..."
                        disabled={isSubmitting}
                        rows={6}
                        className="font-mono"
                      />
                    </div>

                    <div className="flex gap-2">
                      <Button
                        type="button"
                        onClick={handleAddPage}
                        disabled={!isNewPageValid || isSubmitting}
                        size="sm"
                      >
                        Add Page
                      </Button>
                      <Button
                        type="button"
                        variant="ghost"
                        onClick={() => {
                          setShowAddPage(false);
                          setNewPageTitle("");
                          setNewPageSlug("");
                          setNewPageContent("");
                        }}
                        disabled={isSubmitting}
                        size="sm"
                      >
                        Cancel
                      </Button>
                    </div>
                  </div>
                ) : (
                  <Button
                    type="button"
                    variant="outline"
                    onClick={() => setShowAddPage(true)}
                    disabled={isSubmitting}
                    className="w-full"
                  >
                    <Plus className="h-4 w-4 mr-2" />
                    Add Page
                  </Button>
                )}
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
