"use client";

import { useEffect, useMemo, useRef, useState, useCallback } from "react";
import {
  useParams,
  usePathname,
  useRouter,
  useSearchParams,
  useSelectedLayoutSegments,
} from "next/navigation";
import Link from "next/link";
import { useQuery, useMutation } from "convex/react";
import { api } from "../../../convex/_generated/api";
import { Id } from "../../../convex/_generated/dataModel";
import { useUser } from "@/components/providers/user-provider";
import { BepContent } from "@/components/bep/bep-content";
import { BepNav } from "@/components/bep/bep-nav";
import { BepStatusSelect } from "@/components/bep/bep-status-select";
import { BepVersionSelect } from "@/components/bep/bep-version-select";
import { BepExportDialog } from "@/components/bep/bep-export-dialog";
import { BepImportDialog } from "@/components/bep/bep-import-dialog";
import { useEditContext } from "@/components/bep/bep-edit-context";
import { BepSubmitModal } from "@/components/bep/bep-submit-modal";
import { BepAddPageModal } from "@/components/bep/bep-add-page-modal";
import { BepPresence } from "@/components/bep/bep-presence";
import { MDXEditorComponent, MDXEditorHandle } from "@/components/editor/mdx";
import { CommentThread } from "@/components/comments/comment-thread";
import { CommentSidebar } from "@/components/comments/comment-sidebar";
import { DecisionList } from "@/components/decisions/decision-list";
import { IssueList } from "@/components/issues/issue-list";
import { AIAssistantPanel } from "@/components/ai-assistant/ai-assistant-panel";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { ArrowLeft, Edit, History, Pencil } from "lucide-react";
import {
  MAIN_CONTENT_ID,
  RESERVED_PAGE_SLUGS,
  buildBepPath,
  parseBepSegments,
  toNavSectionId,
} from "@/lib/bep-routes";

const CONVEX_SITE_URL = process.env.NEXT_PUBLIC_CONVEX_URL?.replace(
  ".convex.cloud",
  ".convex.site"
) ?? "";

function withQueryParam(path: string, key: string, value: string): string {
  const [base, hash] = path.split("#", 2);
  const [pathname, query] = base.split("?", 2);
  const params = new URLSearchParams(query ?? "");
  params.set(key, value);
  const nextQuery = params.toString();
  return `${pathname}${nextQuery ? `?${nextQuery}` : ""}${hash ? `#${hash}` : ""}`;
}

function highlightElement(target: Element) {
  target.scrollIntoView({ behavior: "smooth", block: "center" });
  target.classList.add("ring-2", "ring-primary", "ring-offset-2");
  window.setTimeout(() => {
    target.classList.remove("ring-2", "ring-primary", "ring-offset-2");
  }, 2000);
}

export function BepRouteShell() {
  const params = useParams();
  const router = useRouter();
  const pathname = usePathname();
  const searchParams = useSearchParams();
  const routeSegments = useSelectedLayoutSegments();
  const { user, userId, isLoading: userLoading } = useUser();
  const routeInfo = useMemo(() => parseBepSegments(routeSegments), [routeSegments]);

  const {
    isEditMode,
    setEditMode,
    trackChange,
    trackNewPage,
    hasChanges,
    openedAt,
    changes,
  } = useEditContext();

  const [showSubmitModal, setShowSubmitModal] = useState(false);
  const [showAddPageModal, setShowAddPageModal] = useState(false);
  const [hasConflict, setHasConflict] = useState(false);
  const [conflictVersion, setConflictVersion] = useState<number | undefined>();
  const [isSubmitting, setIsSubmitting] = useState(false);
  const editorRef = useRef<MDXEditorHandle>(null);
  const newPages = useMemo(
    () =>
      Array.from(changes.entries())
        .filter(([, change]) => change.status === "new" && !!change.slug)
        .map(([tempId, change]) => ({
          tempId,
          slug: change.slug!,
          title: change.title,
        })),
    [changes]
  );

  const rawBepNumber = Array.isArray(params.number) ? params.number[0] : params.number;
  const parsedBepNumber = Number.parseInt(rawBepNumber ?? "", 10);
  const hasValidBepNumber = Number.isFinite(parsedBepNumber);
  const bepNumber = hasValidBepNumber ? parsedBepNumber : 0;
  const bep = useQuery(
    api.beps.getByNumber,
    hasValidBepNumber ? { number: bepNumber } : "skip"
  );
  const updateBep = useMutation(api.beps.update);

  const latestVersion = bep?.versions?.length
    ? [...bep.versions].sort((a, b) => b.version - a.version)[0]
    : null;
  const currentVersionId = latestVersion?._id ?? null;
  const latestVersionNumber = latestVersion?.version ?? null;

  const viewingVersion =
    routeInfo.versionNumber !== null
      ? bep?.versions?.find((v) => v.version === routeInfo.versionNumber) ?? null
      : null;
  const viewingVersionId = viewingVersion?._id ?? null;
  const isViewingHistorical = routeInfo.versionNumber !== null;

  const activeSection = toNavSectionId(routeInfo.section, routeInfo.pageSlug);
  const isContentRoute =
    routeInfo.section === "readme" || routeInfo.section === "page";
  const contentSection =
    routeInfo.section === "page"
      ? routeInfo.pageSlug ?? MAIN_CONTENT_ID
      : MAIN_CONTENT_ID;

  const commentCounts = useQuery(
    api.comments.countsByPage,
    bep
      ? {
          bepId: bep._id,
          versionId: viewingVersionId ?? currentVersionId ?? undefined,
        }
      : "skip"
  );

  const pageComments = useQuery(
    api.comments.byBepPage,
    bep && isContentRoute
      ? {
          bepId: bep._id,
          pageId:
            contentSection === MAIN_CONTENT_ID
              ? undefined
              : bep.pages.find((p) => p.slug === contentSection)?._id,
          versionId: viewingVersionId ?? currentVersionId ?? undefined,
        }
      : "skip"
  );

  useEffect(() => {
    if (!userLoading && !userId) {
      router.push("/login");
    }
  }, [userLoading, userId, router]);

  useEffect(() => {
    if (!hasValidBepNumber) {
      router.replace("/");
      return;
    }
    if (!routeInfo.isValid) {
      router.replace(buildBepPath({ bepNumber, section: "readme" }));
    }
  }, [routeInfo.isValid, hasValidBepNumber, bepNumber, router]);

  useEffect(() => {
    if (!bep || !isViewingHistorical) return;
    if (!viewingVersion) {
      router.replace(buildBepPath({ bepNumber, section: "readme" }));
    }
  }, [bep, isViewingHistorical, viewingVersion, bepNumber, router]);

  useEffect(() => {
    if (!bep || routeInfo.section !== "page" || !routeInfo.pageSlug) return;
    const slug = routeInfo.pageSlug;
    const existsInNewPages = newPages.some((p) => p.slug === slug);
    const existsInCurrentPages = bep.pages.some((p) => p.slug === slug);
    const existsInHistory =
      viewingVersion?.pagesSnapshot?.some((p) => p.slug === slug) ?? false;
    const exists =
      isViewingHistorical ? existsInHistory : existsInCurrentPages || existsInNewPages;

    if (!exists) {
      router.replace(
        buildBepPath({
          bepNumber,
          section: "readme",
          versionNumber:
            isViewingHistorical && latestVersionNumber !== routeInfo.versionNumber
              ? routeInfo.versionNumber
              : null,
        })
      );
    }
  }, [
    bep,
    routeInfo.section,
    routeInfo.pageSlug,
    newPages,
    viewingVersion,
    isViewingHistorical,
    routeInfo.versionNumber,
    latestVersionNumber,
    bepNumber,
    router,
  ]);

  useEffect(() => {
    const focusComment = searchParams.get("focusComment");
    const focusIssue = searchParams.get("focusIssue");
    const focusDecision = searchParams.get("focusDecision");
    const focus =
      (focusComment
        ? {
            key: "focusComment",
            selector: `[data-comment-id="${focusComment}"]`,
          }
        : null) ??
      (focusIssue
        ? {
            key: "focusIssue",
            selector: `[data-issue-id="${focusIssue}"]`,
          }
        : null) ??
      (focusDecision
        ? {
            key: "focusDecision",
            selector: `[data-decision-id="${focusDecision}"]`,
          }
        : null);

    if (!focus) return;

    let cancelled = false;
    let attempts = 0;
    const maxAttempts = 25;
    const intervalMs = 120;

    const tryFocus = () => {
      if (cancelled) return;
      const target = document.querySelector(focus.selector);
      if (target) {
        highlightElement(target);
        const next = new URLSearchParams(searchParams.toString());
        next.delete(focus.key);
        const nextQuery = next.toString();
        router.replace(nextQuery ? `${pathname}?${nextQuery}` : pathname, {
          scroll: false,
        });
        return;
      }
      if (attempts < maxAttempts) {
        attempts += 1;
        window.setTimeout(tryFocus, intervalMs);
      }
    };

    window.setTimeout(tryFocus, intervalMs);
    return () => {
      cancelled = true;
    };
  }, [pathname, router, searchParams]);

  const getPageKey = useCallback(
    (section: string) => {
      if (section === MAIN_CONTENT_ID) return "main";
      const page = bep?.pages.find((p) => p.slug === section);
      return page ? String(page._id) : section;
    },
    [bep]
  );

  const getOriginalContent = useCallback(
    (section: string) => {
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
    },
    [isViewingHistorical, viewingVersion, bep]
  );

  const getCurrentContent = useCallback(
    (section: string) => {
      const newPage = newPages.find((p) => p.slug === section);
      if (newPage) {
        const cachedChange = changes.get(newPage.tempId);
        return cachedChange?.currentContent ?? "";
      }

      const pageKey = getPageKey(section);
      if (isEditMode) {
        const cachedChange = changes.get(pageKey);
        if (cachedChange && cachedChange.status === "modified") {
          return cachedChange.currentContent;
        }
      }
      return getOriginalContent(section);
    },
    [changes, getOriginalContent, getPageKey, isEditMode, newPages]
  );

  const handleContentChange = useCallback(
    (section: string, newContent: string) => {
      if (!bep) return;

      const newPage = newPages.find((p) => p.slug === section);
      if (newPage) {
        trackNewPage(newPage.tempId, newPage.title, newPage.slug, newContent);
        return;
      }

      const pageTitle =
        section === MAIN_CONTENT_ID
          ? "README"
          : bep.pages.find((p) => p.slug === section)?.title ?? section;
      const pageId =
        section === MAIN_CONTENT_ID
          ? ("main" as const)
          : bep.pages.find((p) => p.slug === section)?._id;
      const original = getOriginalContent(section);
      if (pageId) {
        trackChange(pageId, pageTitle, original, newContent);
      }
    },
    [bep, newPages, trackNewPage, getOriginalContent, trackChange]
  );

  const flushEditorBeforeNavigation = useCallback(() => {
    if (!isEditMode || !isContentRoute) return;
    const currentEditorContent = editorRef.current?.getMarkdown();
    if (currentEditorContent !== undefined) {
      handleContentChange(contentSection, currentEditorContent);
    }
  }, [isEditMode, isContentRoute, handleContentChange, contentSection]);

  const navigateToSection = useCallback(
    (
      section: string,
      versionNumber: number | null = isViewingHistorical ? routeInfo.versionNumber : null,
      focus?: { key: string; value: string }
    ) => {
      let path: string;
      if (section === MAIN_CONTENT_ID) {
        path = buildBepPath({
          bepNumber,
          section: "readme",
          versionNumber:
            versionNumber !== null && versionNumber !== latestVersionNumber
              ? versionNumber
              : null,
        });
      } else if (section === "issues") {
        path = buildBepPath({ bepNumber, section: "issues" });
      } else if (section === "decisions") {
        path = buildBepPath({ bepNumber, section: "decisions" });
      } else if (section === "ai") {
        path = buildBepPath({ bepNumber, section: "ai" });
      } else {
        path = buildBepPath({
          bepNumber,
          section: "page",
          pageSlug: section,
          versionNumber:
            versionNumber !== null && versionNumber !== latestVersionNumber
              ? versionNumber
              : null,
        });
      }

      flushEditorBeforeNavigation();
      router.push(focus ? withQueryParam(path, focus.key, focus.value) : path);
    },
    [
      isViewingHistorical,
      routeInfo.versionNumber,
      bepNumber,
      latestVersionNumber,
      flushEditorBeforeNavigation,
      router,
    ]
  );

  const handleSectionChange = (section: string) => {
    navigateToSection(section);
  };

  const handleNavigateToComment = (
    commentId: Id<"comments">,
    pageId?: Id<"bepPages"> | null,
    versionId?: Id<"bepVersions"> | null
  ) => {
    let targetVersionNumber: number | null =
      isViewingHistorical && routeInfo.versionNumber !== latestVersionNumber
        ? routeInfo.versionNumber
        : null;

    if (versionId && versionId !== currentVersionId && versionId !== viewingVersionId) {
      const targetVersion = bep?.versions?.find((v) => v._id === versionId);
      const versionNum = targetVersion?.version ?? null;
      if (!versionNum) return;

      if (confirm(`This comment is from Version ${versionNum}. Switch to view it?`)) {
        targetVersionNumber =
          versionNum === latestVersionNumber ? null : versionNum;
      } else {
        return;
      }
    }

    const targetSection = pageId
      ? bep?.pages.find((p) => p._id === pageId)?.slug ?? MAIN_CONTENT_ID
      : MAIN_CONTENT_ID;

    navigateToSection(targetSection, targetVersionNumber, {
      key: "focusComment",
      value: String(commentId),
    });
  };

  const handleNavigateToIssue = (issueId: string) => {
    navigateToSection("issues", null, { key: "focusIssue", value: issueId });
  };

  const handleNavigateToDecision = (decisionId: string) => {
    navigateToSection("decisions", null, {
      key: "focusDecision",
      value: decisionId,
    });
  };

  const handleVersionRouteChange = (targetVersionNumber: number | null) => {
    const isMetaSection =
      routeInfo.section === "issues" ||
      routeInfo.section === "decisions" ||
      routeInfo.section === "ai";
    if (isMetaSection && targetVersionNumber !== null) {
      navigateToSection(MAIN_CONTENT_ID, targetVersionNumber);
      return;
    }
    if (routeInfo.section === "page" && routeInfo.pageSlug && targetVersionNumber !== null) {
      const targetVersion = bep?.versions.find((v) => v.version === targetVersionNumber);
      const existsInTargetVersion =
        targetVersion?.pagesSnapshot?.some((p) => p.slug === routeInfo.pageSlug) ?? false;
      if (!existsInTargetVersion) {
        navigateToSection(MAIN_CONTENT_ID, targetVersionNumber);
        return;
      }
    }
    navigateToSection(activeSection, targetVersionNumber);
  };

  const handleEditModeToggle = () => {
    if (isEditMode) {
      if (hasChanges) {
        handleSubmitClick();
      } else {
        setEditMode(false);
        setShowSubmitModal(false);
      }
    } else {
      setEditMode(true);
    }
  };

  const handleDiscardChanges = () => {
    setEditMode(false);
    setShowSubmitModal(false);
  };

  const handleAddPage = (title: string, slug: string) => {
    const tempId = `new-${slug}`;
    trackNewPage(tempId, title, slug, "");
    navigateToSection(slug, null);
  };

  const handleSubmitClick = async () => {
    if (!bep || !latestVersion) return;

    if (latestVersion.createdAt > openedAt) {
      setHasConflict(true);
      setConflictVersion(latestVersion.version);
    } else {
      setHasConflict(false);
      setConflictVersion(undefined);
    }
    setShowSubmitModal(true);
  };

  const handleConfirmSubmit = async (
    editNote: string,
    versionMode: "new" | "current"
  ) => {
    if (isSubmitting) return;
    if (!bep || !userId) return;

    setIsSubmitting(true);
    try {
      const currentEditorContent = editorRef.current?.getMarkdown();
      if (currentEditorContent !== undefined) {
        handleContentChange(contentSection, currentEditorContent);
      }

      let mainContent: string | undefined;
      const pageUpdates: Array<{
        _id?: Id<"bepPages">;
        slug: string;
        title: string;
        content: string;
        order: number;
      }> = [];

      for (const [key, change] of changes.entries()) {
        if (change.status === "deleted") continue;

        if (key === "main" || change.pageId === "main") {
          if (change.status === "new") {
            const newPage = newPages.find((p) => p.tempId === key);
            if (newPage) {
              const maxOrder = Math.max(
                0,
                ...bep.pages.map((p) => p.order),
                ...pageUpdates.map((p) => p.order)
              );
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
          const originalPage = bep.pages.find((p) => String(p._id) === key);
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

      for (const page of bep.pages) {
        const hasChange = changes.has(String(page._id));
        const isDeleted =
          hasChange && changes.get(String(page._id))?.status === "deleted";
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

      pageUpdates.sort((a, b) => a.order - b.order);

      await updateBep({
        id: bep._id,
        content: mainContent,
        pages: pageUpdates.length > 0 ? pageUpdates : undefined,
        userId,
        editNote: editNote || undefined,
        versionMode,
      });

      setShowSubmitModal(false);
      setEditMode(false);
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

  if (!hasValidBepNumber) {
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

  const allSections = isViewingHistorical
    ? [
        {
          id: MAIN_CONTENT_ID,
          title: "README",
          // Keep README visible even when the document starts blank.
          hasContent: true,
        },
        ...(viewingVersion?.pagesSnapshot ?? []).map((p) => ({
          id: p.slug,
          title: p.title,
          hasContent: !!p.content,
        })),
      ]
    : [
        {
          id: MAIN_CONTENT_ID,
          title: "README",
          // Keep README visible even when the document starts blank.
          hasContent: true,
        },
        ...bep.pages.map((p) => ({
          id: p.slug,
          title: p.title,
          hasContent: !!p.content,
        })),
        ...newPages.map((p) => ({
          id: p.slug,
          title: p.title,
          hasContent: true,
        })),
      ];

  const pageStatuses: Record<string, "modified" | "new" | "deleted"> = {};
  for (const [key, change] of changes.entries()) {
    if (key === "main") {
      pageStatuses[MAIN_CONTENT_ID] = change.status;
    } else if (change.status === "new") {
      const newPage = newPages.find((p) => p.tempId === key);
      if (newPage) {
        pageStatuses[newPage.slug] = "new";
      }
    } else {
      const page = bep.pages.find((p) => String(p._id) === key);
      if (page) {
        pageStatuses[page.slug] = change.status;
      }
    }
  }

  const currentPageId =
    isContentRoute && contentSection !== MAIN_CONTENT_ID
      ? bep.pages.find((p) => p.slug === contentSection)?._id
      : undefined;
  const hasExistingContent =
    (bep.content ?? "").trim().length > 0 ||
    bep.pages.some((page) => page.content.trim().length > 0);
  const currentContent = getCurrentContent(contentSection);
  const openIssueCount = bep.issues.filter((i) => !i.resolved).length;
  const isContentPage =
    isContentRoute &&
    (contentSection === MAIN_CONTENT_ID ||
      bep.pages.some((p) => p.slug === contentSection) ||
      newPages.some((p) => p.slug === contentSection) ||
      (isViewingHistorical &&
        viewingVersion?.pagesSnapshot?.some((p) => p.slug === contentSection)));

  const effectiveVersionId = viewingVersionId ?? currentVersionId;

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
            <BepExportDialog bepId={bep._id} bepNumber={bep.number} />
            <BepImportDialog bepId={bep._id} bepNumber={bep.number} />
            <BepVersionSelect
              versions={bep.versions}
              currentVersionNumber={
                isViewingHistorical ? routeInfo.versionNumber : null
              }
              onVersionChange={handleVersionRouteChange}
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

      <main className="max-w-5xl mx-auto px-4 py-8 lg:mr-80 lg:ml-8">
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

        <div className="mb-8">
          <div className="flex items-start justify-between gap-4 mb-2">
            <h1 className="text-3xl font-bold">
              <span className="text-muted-foreground font-mono">
                BEP-{String(bep.number).padStart(3, "0")}
              </span>{" "}
              {isViewingHistorical && viewingVersion ? viewingVersion.title : bep.title}
            </h1>
            {!isViewingHistorical && (
              <BepStatusSelect bepId={bep._id} currentStatus={bep.status} />
            )}
          </div>
          <p className="text-muted-foreground">
            Shepherds: {bep.shepherdNames.join(", ") || "None assigned"}
          </p>
        </div>

        <div className="grid grid-cols-1 lg:grid-cols-4 gap-8">
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

          <div className="lg:col-span-3">
            {!isViewingHistorical && routeInfo.section === "issues" ? (
              <IssueList
                bepId={bep._id}
                currentVersionNumber={latestVersionNumber}
                onNavigateToComment={handleNavigateToComment}
              />
            ) : !isViewingHistorical && routeInfo.section === "decisions" ? (
              <DecisionList
                bepId={bep._id}
                currentVersionNumber={latestVersionNumber}
                onNavigateToComment={handleNavigateToComment}
              />
            ) : !isViewingHistorical && routeInfo.section === "ai" ? (
              <AIAssistantPanel
                bepId={bep._id}
                bepNumber={bep.number}
                versions={bep.versions}
                currentVersionId={currentVersionId}
                convexSiteUrl={CONVEX_SITE_URL}
              />
            ) : (
              <div>
                {isContentPage && (
                  <>
                    {isEditMode ? (
                      <div className="border rounded-lg overflow-hidden">
                        <MDXEditorComponent
                          ref={editorRef}
                          initialContent={currentContent}
                          editable={true}
                          onChange={(newContent) =>
                            handleContentChange(contentSection, newContent)
                          }
                          placeholder="Start writing your proposal..."
                          showToolbar={true}
                        />
                      </div>
                    ) : (
                      <div className="relative">
                        <BepContent
                          content={currentContent}
                          linkContext={{
                            bepNumber,
                            isHistorical: isViewingHistorical,
                            versionNumber:
                              isViewingHistorical && routeInfo.versionNumber !== latestVersionNumber
                                ? routeInfo.versionNumber
                                : null,
                          }}
                        />
                        {effectiveVersionId && (
                          <CommentSidebar
                            contentSelector="[data-bep-content]"
                            bepId={bep._id}
                            versionId={effectiveVersionId}
                            pageId={currentPageId}
                            readOnly={isViewingHistorical}
                            comments={(pageComments ?? [])
                              .filter((c) => c.anchor)
                              .map((c) => ({
                                _id: c._id,
                                parentId: c.parentId,
                                anchor: c.anchor as {
                                  nodeId: string;
                                  nodeType: string;
                                  nodeText: string;
                                },
                                authorName: c.authorName,
                                content: c.content,
                                type: c.type,
                                createdAt: c.createdAt,
                                resolved: c.resolved,
                              }))}
                          />
                        )}
                      </div>
                    )}
                  </>
                )}

                {!isEditMode && effectiveVersionId && isContentRoute && (
                  <div className="mt-8 pt-8 border-t">
                    <CommentThread
                      bepId={bep._id}
                      versionId={effectiveVersionId}
                      pageId={currentPageId}
                      viewingVersionId={viewingVersionId ?? undefined}
                      readOnly={isViewingHistorical}
                      linkContext={{
                        bepNumber,
                        isHistorical: isViewingHistorical,
                        versionNumber:
                          isViewingHistorical && routeInfo.versionNumber !== latestVersionNumber
                            ? routeInfo.versionNumber
                            : null,
                      }}
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

      <BepSubmitModal
        open={showSubmitModal}
        onClose={() => setShowSubmitModal(false)}
        onSubmit={handleConfirmSubmit}
        onDiscard={handleDiscardChanges}
        hasConflict={hasConflict}
        conflictVersion={conflictVersion}
        defaultVersionMode={hasExistingContent ? "new" : "current"}
      />

      <BepAddPageModal
        open={showAddPageModal}
        onClose={() => setShowAddPageModal(false)}
        onAdd={handleAddPage}
        existingSlugs={[
          ...bep.pages.map((p) => p.slug),
          ...newPages.map((p) => p.slug),
          ...RESERVED_PAGE_SLUGS,
        ]}
      />
    </div>
  );
}
