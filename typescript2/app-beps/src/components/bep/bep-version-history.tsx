"use client";

import { useState } from "react";
import { Id } from "../../../convex/_generated/dataModel";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
import { BepContent } from "./bep-content";
import { History, ChevronDown, ChevronUp } from "lucide-react";

interface PageSnapshot {
  slug: string;
  title: string;
  content: string;
  order: number;
}

interface Version {
  _id: Id<"bepVersions">;
  version: number;
  title: string;
  content?: string;  // Optional during migration
  pagesSnapshot?: PageSnapshot[];
  editedBy: Id<"users">;
  editNote?: string;
  createdAt: number;
}

interface BepVersionHistoryProps {
  versions: Version[];
  currentTitle: string;
}

export function BepVersionHistory({
  versions,
  currentTitle,
}: BepVersionHistoryProps) {
  const [selectedVersion, setSelectedVersion] = useState<Version | null>(null);
  const [expandedSection, setExpandedSection] = useState<string | null>(null);

  const formatDate = (timestamp: number) => {
    return new Date(timestamp).toLocaleDateString("en-US", {
      month: "short",
      day: "numeric",
      year: "numeric",
      hour: "2-digit",
      minute: "2-digit",
    });
  };

  if (versions.length === 0) {
    return null;
  }

  return (
    <Dialog>
      <DialogTrigger asChild>
        <Button variant="outline" size="sm">
          <History className="h-4 w-4 mr-2" />
          History ({versions.length})
        </Button>
      </DialogTrigger>
      <DialogContent className="max-w-4xl max-h-[80vh] overflow-hidden flex flex-col">
        <DialogHeader>
          <DialogTitle>Version History: {currentTitle}</DialogTitle>
        </DialogHeader>

        <div className="flex-1 overflow-hidden flex gap-4">
          {/* Version list */}
          <div className="w-64 border-r pr-4 overflow-y-auto">
            <div className="space-y-2">
              {versions.map((version) => (
                <button
                  key={version._id}
                  onClick={() => setSelectedVersion(version)}
                  className={`w-full text-left p-3 rounded-lg border transition-colors ${
                    selectedVersion?._id === version._id
                      ? "border-primary bg-accent"
                      : "border-transparent hover:bg-accent/50"
                  }`}
                >
                  <div className="font-medium">Version {version.version}</div>
                  <div className="text-xs text-muted-foreground">
                    {formatDate(version.createdAt)}
                  </div>
                  {version.editNote && (
                    <div className="text-xs mt-1 text-muted-foreground truncate">
                      {version.editNote}
                    </div>
                  )}
                </button>
              ))}
            </div>
          </div>

          {/* Version content */}
          <div className="flex-1 overflow-y-auto">
            {selectedVersion ? (
              <div className="space-y-4">
                <div>
                  <h3 className="font-semibold text-lg">
                    {selectedVersion.title}
                  </h3>
                  {selectedVersion.editNote && (
                    <p className="text-sm text-muted-foreground mt-1">
                      Note: {selectedVersion.editNote}
                    </p>
                  )}
                </div>

                {/* Main content section */}
                <div className="border rounded-lg">
                  <button
                    onClick={() =>
                      setExpandedSection(
                        expandedSection === "_main" ? null : "_main"
                      )
                    }
                    className="w-full flex items-center justify-between p-3 hover:bg-accent/50"
                  >
                    <span className="font-medium">README</span>
                    {expandedSection === "_main" ? (
                      <ChevronUp className="h-4 w-4" />
                    ) : (
                      <ChevronDown className="h-4 w-4" />
                    )}
                  </button>
                  {expandedSection === "_main" && (
                    <div className="p-3 pt-0 border-t">
                      {selectedVersion.content ? (
                        <BepContent content={selectedVersion.content} />
                      ) : (
                        <p className="text-muted-foreground italic">
                          No content
                        </p>
                      )}
                    </div>
                  )}
                </div>

                {/* Pages sections */}
                {selectedVersion.pagesSnapshot?.map((page) => (
                  <div key={page.slug} className="border rounded-lg">
                    <button
                      onClick={() =>
                        setExpandedSection(
                          expandedSection === page.slug ? null : page.slug
                        )
                      }
                      className="w-full flex items-center justify-between p-3 hover:bg-accent/50"
                    >
                      <span className="font-medium">{page.title}</span>
                      {expandedSection === page.slug ? (
                        <ChevronUp className="h-4 w-4" />
                      ) : (
                        <ChevronDown className="h-4 w-4" />
                      )}
                    </button>
                    {expandedSection === page.slug && (
                      <div className="p-3 pt-0 border-t">
                        <BepContent content={page.content} />
                      </div>
                    )}
                  </div>
                ))}
              </div>
            ) : (
              <div className="text-center text-muted-foreground py-8">
                Select a version to view its content
              </div>
            )}
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
}
