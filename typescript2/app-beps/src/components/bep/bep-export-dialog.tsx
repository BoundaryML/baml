"use client";

import { useState, useCallback } from "react";
import { useQuery } from "convex/react";
import { api } from "../../../convex/_generated/api";
import { Id } from "../../../convex/_generated/dataModel";
import JSZip from "jszip";
import { saveAs } from "file-saver";
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
import {
  Download,
  Loader2,
  FolderArchive,
} from "lucide-react";
import {
  ExportData,
  generateAllExportFiles,
} from "@/lib/export-utils";

interface BepExportDialogProps {
  bepId: Id<"beps">;
  bepNumber: number;
}

export function BepExportDialog({
  bepId,
  bepNumber,
}: BepExportDialogProps) {
  const [isOpen, setIsOpen] = useState(false);
  const [isExporting, setIsExporting] = useState(false);

  // Fetch export data when dialog opens
  const exportData = useQuery(
    api.export.getFullBepForExport,
    isOpen ? { bepId } : "skip"
  );

  const formatBepNumber = (num: number) => `BEP-${String(num).padStart(3, "0")}`;

  const handleExport = useCallback(async () => {
    if (!exportData) return;

    setIsExporting(true);

    try {
      const zip = new JSZip();
      const folderName = formatBepNumber(bepNumber);
      const folder = zip.folder(folderName);

      if (!folder) {
        throw new Error("Failed to create ZIP folder");
      }

      // Cast the export data to our expected type
      const data = exportData as unknown as ExportData;

      // Generate all files with the new structure (comments embedded inline)
      const files = generateAllExportFiles(data);

      for (const file of files) {
        // Handle nested paths by creating folders as needed
        const parts = file.path.split("/");
        if (parts.length > 1) {
          const folderPath = parts.slice(0, -1).join("/");
          const fileName = parts[parts.length - 1];
          const nestedFolder = folder.folder(folderPath);
          if (nestedFolder) {
            nestedFolder.file(fileName, file.content);
          }
        } else {
          folder.file(file.path, file.content);
        }
      }

      // Generate the ZIP file
      const content = await zip.generateAsync({ type: "blob" });

      // Download the file
      saveAs(content, `${folderName}.zip`);

      setIsOpen(false);
    } catch (error) {
      console.error("Export failed:", error);
    } finally {
      setIsExporting(false);
    }
  }, [exportData, bepNumber]);

  // Calculate stats from export data for display
  const stats = exportData
    ? (() => {
        const data = exportData as unknown as ExportData;
        return {
          pages: data.pages?.length ?? 0,
          comments: data.comments?.length ?? 0,
          issues: data.issues?.length ?? 0,
          openIssues: data.issues?.filter((i) => !i.resolved).length ?? 0,
          decisions: data.decisions?.length ?? 0,
          versions: data.versions?.length ?? 0,
          summaries: data.summaries?.length ?? 0,
        };
      })()
    : null;

  return (
    <Dialog open={isOpen} onOpenChange={setIsOpen}>
      <DialogTrigger asChild>
        <Button variant="outline" size="sm">
          <Download className="h-4 w-4 mr-2" />
          Export
        </Button>
      </DialogTrigger>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <FolderArchive className="h-5 w-5" />
            Export {formatBepNumber(bepNumber)}
          </DialogTitle>
          <DialogDescription>
            Download this BEP as a ZIP archive optimized for AI agents.
            Comments are embedded inline next to the content they reference.
          </DialogDescription>
        </DialogHeader>

        {!exportData ? (
          <div className="flex items-center justify-center py-8">
            <Loader2 className="h-6 w-6 animate-spin text-muted-foreground" />
          </div>
        ) : (
          <div className="space-y-4">
            {/* Stats summary */}
            {stats && (
              <div className="grid grid-cols-3 gap-2 text-sm">
                <div className="rounded-md bg-muted p-2 text-center">
                  <div className="font-medium">{stats.versions}</div>
                  <div className="text-xs text-muted-foreground">versions</div>
                </div>
                <div className="rounded-md bg-muted p-2 text-center">
                  <div className="font-medium">{stats.comments}</div>
                  <div className="text-xs text-muted-foreground">comments</div>
                </div>
                <div className="rounded-md bg-muted p-2 text-center">
                  <div className="font-medium">{stats.openIssues}</div>
                  <div className="text-xs text-muted-foreground">open issues</div>
                </div>
              </div>
            )}

            {/* Preview of structure */}
            <div className="rounded-md bg-muted p-3">
              <Label className="text-xs font-medium text-muted-foreground">
                Archive structure:
              </Label>
              <pre className="mt-2 text-xs font-mono text-muted-foreground whitespace-pre">
{`${formatBepNumber(bepNumber)}/
  README.md                 # Current content + current version comments
  AGENT_CONTEXT.md          # AI-friendly summary
  metadata.json             # Machine-readable metadata${stats && stats.pages > 0 ? `
  pages/                    # Additional pages + current version comments` : ""}
  discussion/
    issues.md               # Open and resolved issues
    decisions.md            # Recorded decisions
  history/
    versions.md             # Version index
    v1/, v2/, ...           # Per-version content + comments${stats && stats.summaries > 0 ? `
    summaries.md            # AI-generated summaries` : ""}`}
              </pre>
            </div>

            <p className="text-xs text-muted-foreground">
              README and pages show only current version comments. Historical
              comments are preserved in version-specific folders under history/.
            </p>
          </div>
        )}

        <DialogFooter>
          <Button variant="outline" onClick={() => setIsOpen(false)}>
            Cancel
          </Button>
          <Button
            onClick={handleExport}
            disabled={!exportData || isExporting}
          >
            {isExporting ? (
              <>
                <Loader2 className="h-4 w-4 mr-2 animate-spin" />
                Exporting...
              </>
            ) : (
              <>
                <Download className="h-4 w-4 mr-2" />
                Download
              </>
            )}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
