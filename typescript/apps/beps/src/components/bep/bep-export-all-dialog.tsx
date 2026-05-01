"use client";

import { useState, useCallback } from "react";
import { useQuery } from "convex/react";
import { api } from "../../../convex/_generated/api";
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
import { Badge } from "@/components/ui/badge";
import {
  Download,
  Loader2,
  FolderArchive,
  FileText,
  CheckCircle2,
  Circle,
  Clock,
  XCircle,
  RefreshCw,
} from "lucide-react";
import {
  ExportAllData,
  generateAllBepsExportFiles,
} from "@/lib/export-all-utils";

interface BepExportAllDialogProps {
  trigger?: React.ReactNode;
}

const STATUS_CONFIG: Record<string, { icon: React.ReactNode; color: string; label: string }> = {
  implemented: {
    icon: <CheckCircle2 className="h-3 w-3" />,
    color: "bg-blue-500/20 text-blue-700 dark:bg-blue-500/30 dark:text-blue-300",
    label: "Implemented",
  },
  accepted: {
    icon: <CheckCircle2 className="h-3 w-3" />,
    color: "bg-green-500/20 text-green-700 dark:bg-green-500/30 dark:text-green-300",
    label: "Accepted",
  },
  proposed: {
    icon: <Clock className="h-3 w-3" />,
    color: "bg-yellow-500/20 text-yellow-700 dark:bg-yellow-500/30 dark:text-yellow-300",
    label: "Proposed",
  },
  draft: {
    icon: <Circle className="h-3 w-3" />,
    color: "bg-gray-500/20 text-gray-700 dark:bg-gray-500/30 dark:text-gray-300",
    label: "Draft",
  },
  superseded: {
    icon: <RefreshCw className="h-3 w-3" />,
    color: "bg-orange-500/20 text-orange-700 dark:bg-orange-500/30 dark:text-orange-300",
    label: "Superseded",
  },
  rejected: {
    icon: <XCircle className="h-3 w-3" />,
    color: "bg-red-500/20 text-red-700 dark:bg-red-500/30 dark:text-red-300",
    label: "Rejected",
  },
};

export function BepExportAllDialog({ trigger }: BepExportAllDialogProps) {
  const [isOpen, setIsOpen] = useState(false);
  const [isExporting, setIsExporting] = useState(false);

  // Fetch all BEPs for export when dialog opens
  const exportData = useQuery(
    api.export.getAllBepsForExport,
    isOpen ? {} : "skip"
  );

  const handleExport = useCallback(async () => {
    if (!exportData) return;

    setIsExporting(true);

    try {
      const zip = new JSZip();
      const folderName = "all-beps";
      const folder = zip.folder(folderName);

      if (!folder) {
        throw new Error("Failed to create ZIP folder");
      }

      // Cast the export data to our expected type
      const data = exportData as unknown as ExportAllData;

      // Generate all files with the current origin as API base
      const apiBaseUrl = typeof window !== "undefined" ? window.location.origin : "https://beps.boundaryml.com";
      const files = generateAllBepsExportFiles(data, apiBaseUrl);

      for (const file of files) {
        // Handle nested paths by creating folders as needed
        const fileOptions = file.unixPermissions
          ? { unixPermissions: file.unixPermissions }
          : undefined;
        const parts = file.path.split("/");
        if (parts.length > 1) {
          const folderPath = parts.slice(0, -1).join("/");
          const fileName = parts[parts.length - 1];
          const nestedFolder = folder.folder(folderPath);
          if (nestedFolder) {
            nestedFolder.file(fileName, file.content, fileOptions);
          }
        } else {
          folder.file(file.path, file.content, fileOptions);
        }
      }

      // Generate the ZIP file with UNIX platform for permissions
      const content = await zip.generateAsync({ type: "blob", platform: "UNIX" });

      // Download with date in filename
      const date = new Date().toISOString().split("T")[0];
      saveAs(content, `all-beps-${date}.zip`);

      setIsOpen(false);
    } catch (error) {
      console.error("Export failed:", error);
    } finally {
      setIsExporting(false);
    }
  }, [exportData]);

  // Calculate stats from export data for display
  const stats = exportData
    ? (() => {
        const data = exportData as unknown as ExportAllData;
        const statusCounts: Record<string, number> = {};
        let totalPages = 0;

        for (const bep of data.beps) {
          statusCounts[bep.status] = (statusCounts[bep.status] || 0) + 1;
          totalPages += bep.pages.length;
        }

        return {
          total: data.beps.length,
          statusCounts,
          totalPages,
        };
      })()
    : null;

  const defaultTrigger = (
    <Button variant="outline" size="sm">
      <FolderArchive className="h-4 w-4 mr-2" />
      Export All BEPs
    </Button>
  );

  return (
    <Dialog open={isOpen} onOpenChange={setIsOpen}>
      <DialogTrigger asChild>{trigger || defaultTrigger}</DialogTrigger>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <FolderArchive className="h-5 w-5" />
            Export All BEPs
          </DialogTitle>
          <DialogDescription>
            Download all BEPs as a reference bundle for creating new proposals.
            BEPs are sorted by maturity - implemented and accepted BEPs are highlighted.
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
              <div className="space-y-3">
                <div className="flex items-center justify-between text-sm">
                  <span className="text-muted-foreground">Total BEPs:</span>
                  <span className="font-medium">{stats.total}</span>
                </div>

                {/* Status breakdown */}
                <div className="space-y-2">
                  <Label className="text-xs text-muted-foreground">By Status:</Label>
                  <div className="flex flex-wrap gap-2">
                    {Object.entries(stats.statusCounts)
                      .sort(([a], [b]) => {
                        const order = ["implemented", "accepted", "proposed", "draft", "superseded", "rejected"];
                        return order.indexOf(a) - order.indexOf(b);
                      })
                      .map(([status, count]) => {
                        const config = STATUS_CONFIG[status];
                        return (
                          <Badge
                            key={status}
                            variant="secondary"
                            className={`${config?.color || ""} flex items-center gap-1`}
                          >
                            {config?.icon}
                            {count} {config?.label || status}
                          </Badge>
                        );
                      })}
                  </div>
                </div>

                {stats.totalPages > 0 && (
                  <div className="flex items-center justify-between text-sm">
                    <span className="text-muted-foreground">Total additional pages:</span>
                    <span className="font-medium">{stats.totalPages}</span>
                  </div>
                )}
              </div>
            )}

            {/* Preview of structure */}
            <div className="rounded-md bg-muted p-3">
              <Label className="text-xs font-medium text-muted-foreground">
                Archive structure:
              </Label>
              <pre className="mt-2 text-xs font-mono text-muted-foreground whitespace-pre">
{`all-beps/
  Claude.md             # Main index with status-sorted list
  NEW-BEP/
    INSTRUCTIONS.md     # How to create & upload a new BEP
  BEP-001-proposal-slug/
    meta.json           # Metadata (status, version, pages)
    README.md           # Full proposal content
    pages/              # Additional pages (if any)
  BEP-002-another-slug/
    ...`}
              </pre>
            </div>

            <div className="rounded-md border border-green-600/20 bg-green-500/10 p-3 dark:border-green-500/30 dark:bg-green-500/20">
              <div className="flex items-start gap-2">
                <FileText className="h-4 w-4 text-green-600 dark:text-green-400 mt-0.5" />
                <div className="text-xs text-green-800 dark:text-green-300">
                  <p className="font-medium">Optimized for AI context</p>
                  <p className="mt-1">
                    INDEX.md highlights implemented and accepted BEPs as the best references.
                    Status badges help prioritize which proposals to learn from.
                  </p>
                </div>
              </div>
            </div>
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
                Download ({stats?.total || 0} BEPs)
              </>
            )}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
