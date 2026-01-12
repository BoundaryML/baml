"use client";

import { useEffect, useState, useRef, useCallback } from "react";
import { useParams, useRouter } from "next/navigation";
import Link from "next/link";
import { useQuery, useMutation } from "convex/react";
import { api } from "../../../../convex/_generated/api";
import { Id } from "../../../../convex/_generated/dataModel";
import { useUser } from "@/components/providers/user-provider";
// BepContentWithComments removed - now using TiptapEditor for both read and edit modes
import { BepNav } from "@/components/bep/bep-nav";
import { BepStatusSelect } from "@/components/bep/bep-status-select";
import { BepVersionSelect } from "@/components/bep/bep-version-select";
import { BepExportDialog } from "@/components/bep/bep-export-dialog";
import { BepImportDialog } from "@/components/bep/bep-import-dialog";
import { BepEditProvider, useEditContext } from "@/components/bep/bep-edit-context";
import { BepSubmitModal } from "@/components/bep/bep-submit-modal";
import { BepAddPageModal } from "@/components/bep/bep-add-page-modal";
import { BepPresence } from "@/components/bep/bep-presence";
import { LexicalEditor, LexicalEditorHandle, LexicalToolbar } from "@/components/editor/lexical";
import { CommentThread } from "@/components/comments/comment-thread";
import { BlockCommentSidebar } from "@/components/comments/block-comment-sidebar";
import { DecisionList } from "@/components/decisions/decision-list";
import { IssueList } from "@/components/issues/issue-list";
import { AIAssistantPanel } from "@/components/ai-assistant/ai-assistant-panel";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { ArrowLeft, Edit, History, Pencil } from "lucide-react";

// Get Convex site URL for streaming endpoint
const CONVEX_SITE_URL = process.env.NEXT_PUBLIC_CONVEX_URL?.replace(
  ".convex.cloud",
  ".convex.site"
) ?? "";

// Special section ID for main content
const MAIN_CONTENT_ID = "_main";

// Get section from hash, defaulting to main content
const getSectionFromHash = () => {
  if (typeof window === "undefined") return MAIN_CONTENT_ID;
  const hash = window.location.hash.slice(1); // Remove the #
  return hash || MAIN_CONTENT_ID;
};

// Inner component that uses the edit context
function BepDetailPageInner() {
  const params = useParams();
  const router = useRouter();
  const { user, userId, isLoading: userLoading } = useUser();
  const [activeSection, setActiveSection] = useState(MAIN_CONTENT_ID);
  // null = viewing current (latest) version
  const [viewingVersionId, setViewingVersionId] = useState<Id<"bepVersions"> | null>(null);

  // Edit mode state
  const { isEditMode, setEditMode, trackChange, trackNewPage, hasChanges, openedAt, changes } = useEditContext();
  const [showSubmitModal, setShowSubmitModal] = useState(false);
  const [showAddPageModal, setShowAddPageModal] = useState(false);
  const [hasConflict, setHasConflict] = useState(false);
  const [conflictVersion, setConflictVersion] = useState<number | undefined>();
  const [, setIsSubmitting] = useState(false); // TODO: Use for loading state in submit modal
  const editorRef = useRef<LexicalEditorHandle>(null);
  // Track new pages added during edit mode (temporary until saved)
  const [newPages, setNewPages] = useState<Array<{ tempId: string; slug: string; title: string }>>([]);


  const bepNumber = parseInt(params.number as string, 10);
  const bep = useQuery(api.beps.getByNumber, { number: bepNumber });
  const updateBep = useMutation(api.beps.update);

  // Determine the current (latest) version
  const latestVersion = bep?.versions?.length
    ? [...bep.versions].sort((a, b) => b.version - a.version)[0]
    : null;
  const currentVersionId = latestVersion?._id ?? null;

  // Get the version being viewed (for historical viewing)
  const viewingVersion = viewingVersionId
    ? bep?.versions?.find((v) => v._id === viewingVersionId)
    : null;

  // Are we viewing a historical (non-current) version?
  const isViewingHistorical = viewingVersionId !== null;

  // Query comment counts by page (only when BEP is loaded)
  // Always filter by version - use currentVersionId when viewing latest
  const commentCounts = useQuery(
    api.comments.countsByPage,
    bep
      ? {
          bepId: bep._id,
          versionId: viewingVersionId ?? currentVersionId ?? undefined,
        }
      : "skip"
  );

  // Query comments for the current page (to get inline comments)
  // Always filter by version - use currentVersionId when viewing latest
  const pageComments = useQuery(
    api.comments.byBepPage,
    bep
      ? {
          bepId: bep._id,
          pageId: activeSection === MAIN_CONTENT_ID ? undefined : bep.pages.find((p) => p.slug === activeSection)?._id,
          versionId: viewingVersionId ?? currentVersionId ?? undefined,
        }
      : "skip"
  );

  // Initialize from hash on mount and listen for hash changes
  useEffect(() => {
    // Set initial section from hash
    const initialSection = getSectionFromHash();
    if (initialSection !== activeSection) {
      setActiveSection(initialSection);
    }

    // Listen for hash changes (back/forward navigation and link clicks)
    const handleHashChange = () => {
      const newSection = getSectionFromHash();
      setActiveSection(newSection);
    };

    window.addEventListener("hashchange", handleHashChange);
    return () => window.removeEventListener("hashchange", handleHashChange);
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  // Update hash when section changes (from nav clicks)
  // Note: Change tracking happens via onChange callback, not here
  const handleSectionChange = (section: string) => {
    setActiveSection(section);
    const newHash = section === MAIN_CONTENT_ID ? "" : `#${section}`;
    window.history.replaceState(null, "", newHash || window.location.pathname);
  };

  // Navigate to a comment's page and scroll to it
  const handleNavigateToComment = (
    commentId: Id<"comments">,
    pageId?: Id<"bepPages"> | null,
    versionId?: Id<"bepVersions"> | null
  ) => {
    // If the comment is from a different version, switch to that version first
    if (versionId && versionId !== currentVersionId && versionId !== viewingVersionId) {
      const targetVersion = bep?.versions?.find((v) => v._id === versionId);
      const versionNum = targetVersion?.version ?? "unknown";

      if (confirm(`This comment is from Version ${versionNum}. Switch to view it?`)) {
        setViewingVersionId(versionId);
      } else {
        return;
      }
    }

    // Find the page slug from the pageId
    const targetSection = pageId
      ? bep?.pages.find((p) => p._id === pageId)?.slug ?? MAIN_CONTENT_ID
      : MAIN_CONTENT_ID;

    handleSectionChange(targetSection);

    // Scroll to and highlight the comment element
    setTimeout(() => {
      const commentElement = document.querySelector(`[data-comment-id="${commentId}"]`);
      if (commentElement) {
        commentElement.scrollIntoView({ behavior: "smooth", block: "center" });
        commentElement.classList.add("ring-2", "ring-primary", "ring-offset-2");
        setTimeout(() => {
          commentElement.classList.remove("ring-2", "ring-primary", "ring-offset-2");
        }, 2000);
      }
    }, 100);
  };

  // Navigate to Issues section
  const handleNavigateToIssue = (issueId: string) => {
    handleSectionChange("issues");
    // After navigating, scroll to and highlight the issue
    setTimeout(() => {
      const issueElement = document.querySelector(`[data-issue-id="${issueId}"]`);
      if (issueElement) {
        issueElement.scrollIntoView({ behavior: "smooth", block: "center" });
        issueElement.classList.add("ring-2", "ring-primary", "ring-offset-2");
        setTimeout(() => {
          issueElement.classList.remove("ring-2", "ring-primary", "ring-offset-2");
        }, 2000);
      }
    }, 100);
  };

  // Navigate to Decisions section
  const handleNavigateToDecision = (decisionId: string) => {
    handleSectionChange("decisions");
    // After navigating, scroll to and highlight the decision
    setTimeout(() => {
      const decisionElement = document.querySelector(`[data-decision-id="${decisionId}"]`);
      if (decisionElement) {
        decisionElement.scrollIntoView({ behavior: "smooth", block: "center" });
        decisionElement.classList.add("ring-2", "ring-primary", "ring-offset-2");
        setTimeout(() => {
          decisionElement.classList.remove("ring-2", "ring-primary", "ring-offset-2");
        }, 2000);
      }
    }, 100);
  };

  useEffect(() => {
    if (!userLoading && !userId) {
      router.push("/login");
    }
  }, [userLoading, userId, router]);

  // Get the page key for a section (used for change tracking)
  const getPageKey = useCallback((section: string) => {
    if (section === MAIN_CONTENT_ID) return "main";
    const page = bep?.pages.find((p) => p.slug === section);
    return page ? String(page._id) : section;
  }, [bep]);

  // Get original content for a section (from server)
  const getOriginalContent = useCallback((section: string) => {
    if (isViewingHistorical && viewingVersion) {
      if (section === MAIN_CONTENT_ID) {
        return viewingVersion.content || "";
      }
      const page = viewingVersion.pagesSnapshot?.find((p) => p.slug === section);
      return page?.content || "";
    }

    if (!bep) return "";
    if (section === MAIN_CONTENT_ID) {
      return bep.content || "";
    }
    const page = bep.pages.find((p) => p.slug === section);
    return page?.content || "";
  }, [isViewingHistorical, viewingVersion, bep]);

  // Get content for the current section - check cached changes first
  const getCurrentContent = useCallback(() => {
    // Check if this is a new page
    const newPage = newPages.find(p => p.slug === activeSection);
    if (newPage) {
      const cachedChange = changes.get(newPage.tempId);
      return cachedChange?.currentContent ?? "";
    }

    const pageKey = getPageKey(activeSection);

    // If in edit mode and we have cached changes for this page, use them
    if (isEditMode) {
      const cachedChange = changes.get(pageKey);
      if (cachedChange && cachedChange.status === "modified") {
        return cachedChange.currentContent;
      }
    }

    return getOriginalContent(activeSection);
  }, [activeSection, isEditMode, getPageKey, getOriginalContent, changes, newPages]);

  // Handle edit mode toggle
  const handleEditModeToggle = () => {
    if (isEditMode) {
      // In edit mode - show the submit modal for Submit/Keep Editing/Discard options
      handleSubmitClick();
    } else {
      // Entering edit mode
      setEditMode(true);
    }
  };

  // Handle discard changes
  const handleDiscardChanges = () => {
    setEditMode(false);
    setNewPages([]); // Clear new pages when exiting edit mode
    setShowSubmitModal(false);
  };

  // Handle adding a new page
  const handleAddPage = (title: string, slug: string) => {
    const tempId = `new-${slug}`;
    trackNewPage(tempId, title, slug, "");
    setNewPages(prev => [...prev, { tempId, slug, title }]);
    // Navigate to the new page
    handleSectionChange(slug);
  };

  // Handle content change from editor
  const handleContentChange = (newContent: string) => {
    if (!bep) return;

    // Check if this is a new page
    const newPage = newPages.find(p => p.slug === activeSection);
    if (newPage) {
      // Update the new page content in the changes map
      trackNewPage(newPage.tempId, newPage.title, newPage.slug, newContent);
      return;
    }

    const pageTitle = activeSection === MAIN_CONTENT_ID
      ? "README"
      : bep.pages.find(p => p.slug === activeSection)?.title ?? activeSection;

    const pageId = activeSection === MAIN_CONTENT_ID
      ? "main" as const
      : bep.pages.find(p => p.slug === activeSection)?._id;

    // Use the original content for this specific page, not the state variable
    const original = getOriginalContent(activeSection);

    if (pageId) {
      trackChange(pageId, pageTitle, original, newContent);
    }
  };

  // Handle submit click - check for conflicts first
  const handleSubmitClick = async () => {
    if (!bep || !latestVersion) return;

    // Check if document was updated since we entered edit mode
    if (latestVersion.createdAt > openedAt) {
      setHasConflict(true);
      setConflictVersion(latestVersion.version);
    } else {
      setHasConflict(false);
      setConflictVersion(undefined);
    }
    setShowSubmitModal(true);
  };

  // Handle confirm submit
  const handleConfirmSubmit = async (editNote: string) => {
    if (!bep || !userId) return;

    setIsSubmitting(true);
    try {
      // First, capture current editor content (user may have edited current page without switching)
      const currentEditorContent = editorRef.current?.getMarkdown();
      if (currentEditorContent !== undefined) {
        handleContentChange(currentEditorContent);
      }

      // Build update payload from all tracked changes
      let mainContent: string | undefined;
      const pageUpdates: Array<{
        _id?: Id<"bepPages">;
        slug: string;
        title: string;
        content: string;
        order: number;
      }> = [];

      // Process all tracked changes
      for (const [key, change] of changes.entries()) {
        if (change.status === "deleted") {
          // Deleted pages are handled by not including them in pageUpdates
          continue;
        }

        if (key === "main" || change.pageId === "main") {
          // Check if this is main content or a new page
          if (change.status === "new") {
            // This is a new page - find it in newPages
            const newPage = newPages.find(p => p.tempId === key);
            if (newPage) {
              const maxOrder = Math.max(0, ...bep.pages.map(p => p.order), ...pageUpdates.map(p => p.order));
              pageUpdates.push({
                slug: newPage.slug,
                title: newPage.title,
                content: change.currentContent,
                order: maxOrder + 1,
              });
            }
          } else {
            mainContent = change.currentContent;
          }
        } else {
          // Find the original page to get its metadata
          const originalPage = bep.pages.find(p => String(p._id) === key);
          if (originalPage) {
            pageUpdates.push({
              _id: originalPage._id,
              slug: originalPage.slug,
              title: originalPage.title,
              content: change.currentContent,
              order: originalPage.order,
            });
          }
        }
      }

      // Include unchanged pages (so they don't get deleted)
      for (const page of bep.pages) {
        const hasChange = changes.has(String(page._id));
        const isDeleted = hasChange && changes.get(String(page._id))?.status === "deleted";
        if (!hasChange && !isDeleted) {
          pageUpdates.push({
            _id: page._id,
            slug: page.slug,
            title: page.title,
            content: page.content,
            order: page.order,
          });
        }
      }

      // Sort pages by order
      pageUpdates.sort((a, b) => a.order - b.order);

      await updateBep({
        id: bep._id,
        content: mainContent,
        pages: pageUpdates.length > 0 ? pageUpdates : undefined,
        userId,
        editNote: editNote || undefined,
      });

      setShowSubmitModal(false);
      setEditMode(false);
      setNewPages([]); // Clear new pages on successful save
    } catch (error) {
      console.error("Failed to save changes:", error);
    } finally {
      setIsSubmitting(false);
    }
  };

  if (userLoading || bep === undefined) {
    return (
      <div className="min-h-screen bg-background">
        <header className="border-b">
          <div className="max-w-6xl mx-auto px-4 py-4">
            <Skeleton className="h-8 w-32" />
          </div>
        </header>
        <main className="max-w-6xl mx-auto px-4 py-8">
          <Skeleton className="h-12 w-3/4 mb-4" />
          <Skeleton className="h-6 w-48 mb-8" />
          <div className="grid grid-cols-4 gap-8">
            <div className="col-span-1">
              <Skeleton className="h-64 w-full" />
            </div>
            <div className="col-span-3">
              <Skeleton className="h-96 w-full" />
            </div>
          </div>
        </main>
      </div>
    );
  }

  if (!user) {
    return null;
  }

  if (!bep) {
    return (
      <div className="min-h-screen bg-background">
        <header className="border-b">
          <div className="max-w-6xl mx-auto px-4 py-4">
            <Link
              href="/"
              className="flex items-center gap-2 text-muted-foreground hover:text-foreground"
            >
              <ArrowLeft className="h-4 w-4" />
              Back
            </Link>
          </div>
        </header>
        <main className="max-w-6xl mx-auto px-4 py-8">
          <div className="text-center py-12">
            <h2 className="text-xl font-semibold mb-2">BEP not found</h2>
            <p className="text-muted-foreground">
              BEP-{String(bepNumber).padStart(3, "0")} does not exist.
            </p>
          </div>
        </main>
      </div>
    );
  }

  // Build sections array based on whether we're viewing historical or current
  const allSections = isViewingHistorical
    ? [
        // When viewing historical, show main content and pages from snapshot
        {
          id: MAIN_CONTENT_ID,
          title: "README",
          hasContent: !!viewingVersion?.content,
        },
        ...(viewingVersion?.pagesSnapshot ?? []).map((p) => ({
          id: p.slug,
          title: p.title,
          hasContent: !!p.content,
        })),
      ]
    : [
        // Current version: show live data + new pages
        {
          id: MAIN_CONTENT_ID,
          title: "README",
          hasContent: !!bep.content,
        },
        ...bep.pages.map((p) => ({
          id: p.slug,
          title: p.title,
          hasContent: !!p.content,
        })),
        // Add new pages (only visible in edit mode)
        ...newPages.map((p) => ({
          id: p.slug,
          title: p.title,
          hasContent: true, // New pages always show in nav
        })),
      ];

  // Build page statuses from changes map for BepNav
  const pageStatuses: Record<string, "modified" | "new" | "deleted"> = {};
  for (const [key, change] of changes.entries()) {
    // Map the change key to the section id
    if (key === "main") {
      pageStatuses[MAIN_CONTENT_ID] = change.status;
    } else if (change.status === "new") {
      // For new pages, find by tempId
      const newPage = newPages.find(p => p.tempId === key);
      if (newPage) {
        pageStatuses[newPage.slug] = "new";
      }
    } else {
      // For existing pages, find by page ID
      const page = bep.pages.find(p => String(p._id) === key);
      if (page) {
        pageStatuses[page.slug] = change.status;
      }
    }
  }

  // Get page ID for comments (undefined = main content)
  const getPageId = (): Id<"bepPages"> | undefined => {
    if (activeSection === MAIN_CONTENT_ID) {
      return undefined;
    }
    const page = bep.pages.find((p) => p.slug === activeSection);
    return page?._id;
  };

  const currentPageId = getPageId();

  const openIssueCount = bep.issues.filter((i) => !i.resolved).length;

  // Check if we're on a content page (not issues/decisions/ai-assistant)
  const isContentPage = activeSection === MAIN_CONTENT_ID ||
    bep.pages.some(p => p.slug === activeSection) ||
    newPages.some(p => p.slug === activeSection) ||
    (isViewingHistorical && viewingVersion?.pagesSnapshot?.some(p => p.slug === activeSection));

  return (
    <div className="min-h-screen bg-background">
      <header className="border-b">
        <div className="max-w-6xl mx-auto px-4 py-4 flex items-center justify-between">
          <Link
            href="/"
            className="flex items-center gap-2 text-muted-foreground hover:text-foreground"
          >
            <ArrowLeft className="h-4 w-4" />
            Back
          </Link>
          <div className="flex items-center gap-3">
            {userId && <BepPresence bepId={bep._id} userId={userId} />}
            <BepExportDialog
              bepId={bep._id}
              bepNumber={bep.number}
            />
            <BepImportDialog
              bepId={bep._id}
              bepNumber={bep.number}
            />
            <BepVersionSelect
              versions={bep.versions}
              currentVersionId={viewingVersionId}
              onVersionChange={setViewingVersionId}
            />
            {!isViewingHistorical && (
              <Button
                variant={isEditMode ? "default" : "outline"}
                size="sm"
                onClick={handleEditModeToggle}
              >
                {isEditMode ? (
                  <>
                    <Pencil className="h-4 w-4 mr-2 animate-pulse" />
                    Editing...
                  </>
                ) : (
                  <>
                    <Edit className="h-4 w-4 mr-2" />
                    Edit
                  </>
                )}
              </Button>
            )}
          </div>
        </div>
      </header>

      <main className="max-w-6xl mx-auto px-4 py-8">
        {/* Historical version banner */}
        {isViewingHistorical && viewingVersion && (
          <Alert className="mb-6 border-amber-500 bg-amber-50 dark:bg-amber-950/30">
            <History className="h-4 w-4 text-amber-600" />
            <AlertDescription className="text-amber-800 dark:text-amber-200">
              Viewing <strong>Version {viewingVersion.version}</strong>{" "}
              (historical) - Created{" "}
              {new Date(viewingVersion.createdAt).toLocaleDateString()}. Comments
              are read-only. Issues, Decisions, and AI Assistant are hidden.
            </AlertDescription>
          </Alert>
        )}

        {/* Title and metadata */}
        <div className="mb-8">
          <div className="flex items-start justify-between gap-4 mb-2">
            <h1 className="text-3xl font-bold">
              <span className="text-muted-foreground font-mono">
                BEP-{String(bep.number).padStart(3, "0")}
              </span>{" "}
              {isViewingHistorical && viewingVersion
                ? viewingVersion.title
                : bep.title}
            </h1>
            {!isViewingHistorical && (
              <BepStatusSelect bepId={bep._id} currentStatus={bep.status} />
            )}
          </div>
          <p className="text-muted-foreground">
            Shepherds: {bep.shepherdNames.join(", ") || "None assigned"}
          </p>
        </div>

        {/* Content grid */}
        <div className="grid grid-cols-1 lg:grid-cols-4 gap-8">
          {/* Navigation sidebar */}
          <aside className="lg:col-span-1">
            <div className="sticky top-8">
              <BepNav
                sections={allSections}
                activeSection={activeSection}
                onSectionClick={handleSectionChange}
                commentCounts={commentCounts ?? {}}
                openIssueCount={isViewingHistorical ? 0 : openIssueCount}
                decisionCount={isViewingHistorical ? 0 : bep.decisions.length}
                hideMetaSections={isViewingHistorical}
                isEditMode={isEditMode}
                pageStatuses={pageStatuses}
                onAddPage={() => setShowAddPageModal(true)}
              />

            </div>
          </aside>

          {/* Main content */}
          <div className="lg:col-span-3">
            {!isViewingHistorical && activeSection === "issues" ? (
              <IssueList
                bepId={bep._id}
                currentVersionNumber={latestVersion?.version ?? null}
                onNavigateToComment={handleNavigateToComment}
              />
            ) : !isViewingHistorical && activeSection === "decisions" ? (
              <DecisionList
                bepId={bep._id}
                currentVersionNumber={latestVersion?.version ?? null}
                onNavigateToComment={handleNavigateToComment}
              />
            ) : !isViewingHistorical && activeSection === "ai-assistant" ? (
              <AIAssistantPanel
                bepId={bep._id}
                versions={bep.versions}
                currentVersionId={currentVersionId}
                convexSiteUrl={CONVEX_SITE_URL}
              />
            ) : (
              <div>
                {/* Lexical editor - editable in edit mode, read-only otherwise */}
                {isContentPage && (
                  <div className={isEditMode ? "border rounded-lg overflow-hidden" : "relative pr-12"}>
                    {isEditMode && (
                      <LexicalToolbar editor={editorRef.current?.getEditor() ?? null} />
                    )}
                    <div className={isEditMode ? "p-4 edit-mode-active" : ""}>
                      <LexicalEditor
                        ref={editorRef}
                        initialContent={getCurrentContent()}
                        editable={isEditMode}
                        onChange={isEditMode ? handleContentChange : undefined}
                        placeholder="Start writing your proposal..."
                      />
                    </div>
                    {/* Block comment sidebar - only in read mode */}
                    {!isEditMode && currentVersionId && (
                      <BlockCommentSidebar
                        editor={editorRef.current?.getEditor() ?? null}
                        bepId={bep._id}
                        versionId={currentVersionId}
                        pageId={currentPageId}
                        comments={(pageComments ?? []).filter(c => c.anchor).map(c => ({
                          _id: c._id,
                          anchor: c.anchor as { nodeId: string; nodeType: string; nodeText: string },
                          authorName: c.authorName,
                          content: c.content,
                          type: c.type,
                          createdAt: c.createdAt,
                          resolved: c.resolved,
                        }))}
                        readOnly={isViewingHistorical}
                      />
                    )}
                  </div>
                )}

                {/* Comments section - hide in edit mode */}
                {!isEditMode && currentVersionId && (
                  <div className="mt-8 pt-8 border-t">
                    <CommentThread
                      bepId={bep._id}
                      versionId={currentVersionId}
                      pageId={currentPageId}
                      viewingVersionId={viewingVersionId ?? undefined}
                      readOnly={isViewingHistorical}
                      onNavigateToIssue={handleNavigateToIssue}
                      onNavigateToDecision={handleNavigateToDecision}
                    />
                  </div>
                )}
              </div>
            )}
          </div>
        </div>
      </main>

      {/* Submit modal */}
      <BepSubmitModal
        open={showSubmitModal}
        onClose={() => setShowSubmitModal(false)}
        onSubmit={handleConfirmSubmit}
        onDiscard={handleDiscardChanges}
        hasConflict={hasConflict}
        conflictVersion={conflictVersion}
      />

      {/* Add page modal */}
      <BepAddPageModal
        open={showAddPageModal}
        onClose={() => setShowAddPageModal(false)}
        onAdd={handleAddPage}
        existingSlugs={[...bep.pages.map(p => p.slug), ...newPages.map(p => p.slug)]}
      />
    </div>
  );
}

// Wrapper component that provides the edit context
export default function BepDetailPage() {
  return (
    <BepEditProvider>
      <BepDetailPageInner />
    </BepEditProvider>
  );
}
