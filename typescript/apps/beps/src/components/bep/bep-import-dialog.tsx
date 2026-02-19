"use client";

import { useState, useCallback, useRef } from "react";
import { useMutation, useQuery } from "convex/react";
import { api } from "../../../convex/_generated/api";
import { Id } from "../../../convex/_generated/dataModel";
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
import { Label } from "@/components/ui/label";
import { Input } from "@/components/ui/input";
import { Badge } from "@/components/ui/badge";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { cn } from "@/lib/utils";
import {
  Upload,
  Loader2,
  FileText,
  AlertCircle,
  CheckCircle2,
  Plus,
  RefreshCw,
  Folder,
} from "lucide-react";
import {
  parseImportedReadme,
  parseImportedPage,
  sanitizeSlug,
  hasContent,
} from "@/lib/import-utils";
import type { VersionMode } from "@/lib/types";

interface BepImportDialogProps {
  bepId: Id<"beps">;
  bepNumber: number;
}

interface ParsedFile {
  name: string;
  type: "readme" | "page";
  content: string;
  extractedTitle?: string;
  slug?: string;
  isNew?: boolean; // true if this page doesn't exist yet
  error?: string;
}

export function BepImportDialog({ bepId, bepNumber }: BepImportDialogProps) {
  const [isOpen, setIsOpen] = useState(false);
  const [isImporting, setIsImporting] = useState(false);
  const [editNote, setEditNote] = useState("");
  const [versionMode, setVersionMode] = useState<VersionMode>("new");
  const [parsedFiles, setParsedFiles] = useState<ParsedFile[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [success, setSuccess] = useState<{
    versionNumber: number;
    versionAction: "created" | "updated";
    pagesCreated: number;
    pagesUpdated: number;
  } | null>(null);
  const folderInputRef = useRef<HTMLInputElement>(null);

  const importVersion = useMutation(api.beps.importVersion);

  // Get current BEP data to check which pages exist
  const bepData = useQuery(
    api.beps.getByNumber,
    isOpen ? { number: bepNumber } : "skip"
  );

  const formatBepNumber = (num: number) =>
    `BEP-${String(num).padStart(3, "0")}`;

  const handleFolderSelect = useCallback(
    async (event: React.ChangeEvent<HTMLInputElement>) => {
      const files = event.target.files;
      if (!files || files.length === 0) return;

      // Create set of existing page slugs inside callback to avoid dependency issues
      const existingPageSlugs = new Set(bepData?.pages?.map((p) => p.slug) ?? []);

      setError(null);
      setSuccess(null);
      const parsed: ParsedFile[] = [];

      // Filter files: look for README.md at root and .md files in pages/ folder
      for (const file of Array.from(files)) {
        // Get the relative path within the selected folder
        // webkitRelativePath gives us "FolderName/path/to/file.md"
        const relativePath = file.webkitRelativePath;
        const pathParts = relativePath.split("/");

        // Skip the root folder name, get the path within
        // e.g., "BEP-001/README.md" -> "README.md"
        // e.g., "BEP-001/pages/background.md" -> "pages/background.md"
        const innerPath = pathParts.slice(1).join("/");

        // Skip non-markdown files
        if (!file.name.toLowerCase().endsWith(".md")) {
          continue;
        }

        // Determine file type based on path
        const isReadme = innerPath.toLowerCase() === "readme.md";
        const isPage = innerPath.toLowerCase().startsWith("pages/") &&
          pathParts.length === 3; // Only direct children of pages/

        // Skip files that aren't README or in pages/
        if (!isReadme && !isPage) {
          continue;
        }

        try {
          const text = await file.text();

          // Check if file is empty
          if (!text.trim()) {
            parsed.push({
              name: innerPath,
              type: isReadme ? "readme" : "page",
              content: "",
              error: "File is empty",
            });
            continue;
          }

          if (isReadme) {
            const result = parseImportedReadme(text);
            if (!hasContent(result.content)) {
              parsed.push({
                name: "README.md",
                type: "readme",
                content: "",
                error: "No content after stripping comments",
              });
              continue;
            }
            parsed.push({
              name: "README.md",
              type: "readme",
              content: result.content,
              extractedTitle: result.extractedTitle,
            });
          } else {
            // It's a page file
            const result = parseImportedPage(text, file.name);
            if (!hasContent(result.content)) {
              parsed.push({
                name: `pages/${file.name}`,
                type: "page",
                content: "",
                slug: result.slug,
                error: "No content after stripping comments",
              });
              continue;
            }
            // Sanitize the slug
            const slug = sanitizeSlug(file.name);
            if (!slug) {
              parsed.push({
                name: `pages/${file.name}`,
                type: "page",
                content: result.content,
                error: "Invalid filename for page slug",
              });
              continue;
            }
            parsed.push({
              name: `pages/${file.name}`,
              type: "page",
              content: result.content,
              extractedTitle: result.title,
              slug,
              isNew: !existingPageSlugs.has(slug),
            });
          }
        } catch (err) {
          parsed.push({
            name: innerPath,
            type: isReadme ? "readme" : "page",
            content: "",
            error: `Failed to read file: ${err instanceof Error ? err.message : "Unknown error"}`,
          });
        }
      }

      // Check if we found a README
      if (!parsed.some((f) => f.type === "readme")) {
        setError("No README.md found in the selected folder");
        setParsedFiles([]);
        return;
      }

      setParsedFiles(parsed);

      // Reset file input
      if (folderInputRef.current) {
        folderInputRef.current.value = "";
      }
    },
    [bepData?.pages]
  );

  const handleImport = useCallback(async () => {
    // Validation
    const readme = parsedFiles.find((f) => f.type === "readme" && !f.error);
    if (!readme) {
      setError("A README.md file is required for import");
      return;
    }

    const validPages = parsedFiles.filter(
      (f) => f.type === "page" && !f.error && f.slug
    );

    // Get the current user
    const userId = localStorage.getItem("bep-user-id");
    if (!userId) {
      setError("You must be logged in to import");
      return;
    }

    setIsImporting(true);
    setError(null);

    try {
      const result = await importVersion({
        bepId,
        content: readme.content,
        pages: validPages.map((p) => ({
          slug: p.slug!,
          title: p.extractedTitle || p.slug!,
          content: p.content,
        })),
        editNote: editNote || undefined,
        userId: userId as Id<"users">,
        versionMode,
      });

      setSuccess({
        versionNumber: result.versionNumber,
        versionAction: result.versionAction,
        pagesCreated: result.pagesCreated,
        pagesUpdated: result.pagesUpdated,
      });

      // Clear parsed files
      setParsedFiles([]);
      setEditNote("");
    } catch (err) {
      setError(
        `Import failed: ${err instanceof Error ? err.message : "Unknown error"}`
      );
    } finally {
      setIsImporting(false);
    }
  }, [parsedFiles, editNote, bepId, importVersion, versionMode]);

  const handleClose = () => {
    setIsOpen(false);
    setParsedFiles([]);
    setError(null);
    setSuccess(null);
    setEditNote("");
    setVersionMode("new");
  };

  const readmeFile = parsedFiles.find((f) => f.type === "readme");
  const pageFiles = parsedFiles.filter((f) => f.type === "page");
  const hasValidReadme = readmeFile && !readmeFile.error;
  const validPageCount = pageFiles.filter((p) => !p.error).length;
  const errorCount = parsedFiles.filter((f) => f.error).length;
  const latestVersionNumber = bepData?.versions?.[0]?.version ?? 0;
  const targetVersionNumber =
    versionMode === "new" || latestVersionNumber === 0
      ? latestVersionNumber + 1
      : latestVersionNumber;
  const latestVersionLabel =
    latestVersionNumber > 0 ? `v${latestVersionNumber}` : "the current draft";

  return (
    <Dialog
      open={isOpen}
      onOpenChange={(open) => {
        if (!open) {
          handleClose();
          return;
        }
        setIsOpen(true);
      }}
    >
      <DialogTrigger asChild>
        <Button variant="outline" size="sm">
          <Upload className="h-4 w-4 mr-2" />
          Import
        </Button>
      </DialogTrigger>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <Upload className="h-5 w-5" />
            Import {formatBepNumber(bepNumber)}
          </DialogTitle>
          <DialogDescription>
            Upload markdown files, then choose whether to create a new version
            or apply edits to the current version.
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-4">
          {/* Success message */}
          {success && (
            <Alert className="border-green-500 bg-green-50">
              <CheckCircle2 className="h-4 w-4 text-green-600" />
              <AlertDescription className="text-green-800">
                Successfully{" "}
                {success.versionAction === "created" ? "created version" : "updated current version"}{" "}
                {success.versionNumber}
                {success.pagesCreated > 0 &&
                  ` (${success.pagesCreated} new page${success.pagesCreated > 1 ? "s" : ""})`}
                {success.pagesUpdated > 0 &&
                  ` (${success.pagesUpdated} page${success.pagesUpdated > 1 ? "s" : ""} updated)`}
              </AlertDescription>
            </Alert>
          )}

          <div className="space-y-2">
            <Label>How should this import be applied?</Label>
            <div className="grid gap-2 sm:grid-cols-2">
              <button
                type="button"
                onClick={() => setVersionMode("new")}
                aria-pressed={versionMode === "new"}
                className={cn(
                  "rounded-lg border p-3 text-left transition-colors",
                  versionMode === "new"
                    ? "border-primary bg-primary/5"
                    : "hover:bg-muted/50"
                )}
              >
                <p className="font-medium text-sm">Create New Version</p>
                <p className="text-xs text-muted-foreground mt-1">
                  Use for major updates and fresh review cycles.
                </p>
              </button>
              <button
                type="button"
                onClick={() => setVersionMode("current")}
                aria-pressed={versionMode === "current"}
                className={cn(
                  "rounded-lg border p-3 text-left transition-colors",
                  versionMode === "current"
                    ? "border-primary bg-primary/5"
                    : "hover:bg-muted/50"
                )}
              >
                <p className="font-medium text-sm">Apply To Current Version</p>
                <p className="text-xs text-muted-foreground mt-1">
                  Use for small fixes to keep current comments in place.
                </p>
              </button>
            </div>
          </div>

          {/* Error message */}
          {error && (
            <Alert variant="destructive">
              <AlertCircle className="h-4 w-4" />
              <AlertDescription>{error}</AlertDescription>
            </Alert>
          )}

          {/* Folder input */}
          <div>
            <Label htmlFor="folder-upload" className="block mb-2">
              Select exported folder
            </Label>
            <div className="relative">
              <input
                ref={folderInputRef}
                id="folder-upload"
                type="file"
                // @ts-expect-error webkitdirectory is not in the type definitions
                webkitdirectory=""
                onChange={handleFolderSelect}
                className="absolute inset-0 w-full h-full opacity-0 cursor-pointer"
              />
              <Button variant="outline" className="w-full justify-start" asChild>
                <div>
                  <Folder className="h-4 w-4 mr-2" />
                  {parsedFiles.length > 0
                    ? `${parsedFiles.length} file${parsedFiles.length > 1 ? "s" : ""} selected`
                    : "Choose folder..."}
                </div>
              </Button>
            </div>
            <p className="text-xs text-muted-foreground mt-1">
              Select the exported BEP folder (e.g., BEP-001). Will import
              README.md and files from pages/.
            </p>
          </div>

          {/* Parsed files preview */}
          {parsedFiles.length > 0 && (
            <div className="space-y-2">
              <Label className="block">Files to import:</Label>
              <div className="rounded-md border divide-y">
                {/* README */}
                {readmeFile && (
                  <div className="p-3 flex items-center justify-between">
                    <div className="flex items-center gap-2">
                      <FileText className="h-4 w-4 text-muted-foreground" />
                      <span className="font-medium">README.md</span>
                      <Badge variant="secondary">README</Badge>
                    </div>
                    {readmeFile.error ? (
                      <Badge variant="destructive">{readmeFile.error}</Badge>
                    ) : (
                      <Badge
                        variant="outline"
                        className="text-green-600 border-green-600"
                      >
                        <CheckCircle2 className="h-3 w-3 mr-1" />
                        Ready
                      </Badge>
                    )}
                  </div>
                )}

                {/* Pages */}
                {pageFiles.map((file, idx) => (
                  <div
                    key={idx}
                    className="p-3 flex items-center justify-between"
                  >
                    <div className="flex items-center gap-2">
                      <FileText className="h-4 w-4 text-muted-foreground" />
                      <span className="font-medium">{file.name}</span>
                      {file.slug && (
                        <Badge variant="outline" className="font-mono text-xs">
                          {file.slug}
                        </Badge>
                      )}
                      {file.isNew ? (
                        <Badge className="bg-blue-100 text-blue-800 hover:bg-blue-100">
                          <Plus className="h-3 w-3 mr-1" />
                          New
                        </Badge>
                      ) : file.slug ? (
                        <Badge className="bg-amber-100 text-amber-800 hover:bg-amber-100">
                          <RefreshCw className="h-3 w-3 mr-1" />
                          Update
                        </Badge>
                      ) : null}
                    </div>
                    {file.error ? (
                      <Badge variant="destructive">{file.error}</Badge>
                    ) : (
                      <Badge
                        variant="outline"
                        className="text-green-600 border-green-600"
                      >
                        <CheckCircle2 className="h-3 w-3 mr-1" />
                        Ready
                      </Badge>
                    )}
                  </div>
                ))}
              </div>

              {/* Summary */}
              {errorCount > 0 && (
                <p className="text-xs text-destructive">
                  {errorCount} file{errorCount > 1 ? "s" : ""} with errors will
                  be skipped
                </p>
              )}
            </div>
          )}

          {/* Edit note */}
          {parsedFiles.length > 0 && hasValidReadme && (
            <div>
              <Label htmlFor="edit-note" className="block mb-2">
                {versionMode === "new" ? "Version note (optional)" : "Change note (optional)"}
              </Label>
              <Input
                id="edit-note"
                placeholder={
                  versionMode === "new"
                    ? "e.g., Updated after AI review"
                    : "e.g., Fixed typo and clarified wording"
                }
                value={editNote}
                onChange={(e) => setEditNote(e.target.value)}
              />
            </div>
          )}

          {/* Info text */}
          <p className="text-xs text-muted-foreground">
            {versionMode === "new"
              ? `This will create version v${targetVersionNumber}. Existing comments stay with ${latestVersionLabel} and v${targetVersionNumber} starts with zero comments.`
              : `This will update current version v${targetVersionNumber} in place and keep existing comment threads visible.`}
          </p>
        </div>

        <DialogFooter>
          <Button variant="outline" onClick={handleClose}>
            {success ? "Close" : "Cancel"}
          </Button>
          {!success && (
            <Button
              onClick={handleImport}
              disabled={!hasValidReadme || isImporting}
            >
              {isImporting ? (
                <>
                  <Loader2 className="h-4 w-4 mr-2 animate-spin" />
                  Importing...
                </>
              ) : (
                <>
                  <Upload className="h-4 w-4 mr-2" />
                  {versionMode === "new"
                    ? `Import & Create v${targetVersionNumber}`
                    : `Import To v${targetVersionNumber}`}{" "}
                  ({validPageCount > 0 ? `${validPageCount + 1} files` : "1 file"})
                </>
              )}
            </Button>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
