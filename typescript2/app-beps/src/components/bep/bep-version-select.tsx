"use client";

import { Id } from "../../../convex/_generated/dataModel";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { History } from "lucide-react";

interface Version {
  _id: Id<"bepVersions">;
  version: number;
  title: string;
  content?: string;
  editNote?: string;
  createdAt: number;
}

interface BepVersionSelectProps {
  versions: Version[];
  currentVersionNumber: number | null; // null = viewing current (latest)
  onVersionChange: (versionNumber: number | null) => void;
  disabled?: boolean;
}

export function BepVersionSelect({
  versions,
  currentVersionNumber,
  onVersionChange,
  disabled = false,
}: BepVersionSelectProps) {
  if (versions.length === 0) {
    return null;
  }

  // Sort versions by version number descending (latest first)
  const sortedVersions = [...versions].sort((a, b) => b.version - a.version);
  const latestVersion = sortedVersions[0];

  const formatDate = (timestamp: number) => {
    return new Date(timestamp).toLocaleDateString("en-US", {
      month: "short",
      day: "numeric",
    });
  };

  // Determine the display value
  const selectedValue =
    currentVersionNumber === null || currentVersionNumber === latestVersion.version
      ? "current"
      : `v-${currentVersionNumber}`;

  return (
    <div className="flex items-center gap-2">
      <History className="h-4 w-4 text-muted-foreground" />
      <Select
        value={selectedValue}
        onValueChange={(value) => {
          if (value === "current") {
            onVersionChange(null);
          } else {
            const parsed = Number.parseInt(value.replace("v-", ""), 10);
            if (Number.isFinite(parsed) && parsed > 0) {
              onVersionChange(parsed);
            }
          }
        }}
        disabled={disabled}
      >
        <SelectTrigger className="w-[180px] h-8">
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          {/* Current (latest) version option */}
          <SelectItem value="current">
            <div className="flex items-center gap-2">
              <span className="font-medium">v{latestVersion.version}</span>
              <span className="text-muted-foreground text-xs">(current)</span>
            </div>
          </SelectItem>

          {/* Historical versions (skip the latest since it's the "current" option) */}
          {sortedVersions.slice(1).map((version) => (
            <SelectItem key={version._id} value={`v-${version.version}`}>
              <div className="flex items-center gap-2">
                <span>v{version.version}</span>
                <span className="text-muted-foreground text-xs">
                  {formatDate(version.createdAt)}
                </span>
              </div>
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
    </div>
  );
}
