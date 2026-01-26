"use client";

import { createContext, useContext, useState, useCallback, ReactNode } from "react";
import { Id } from "../../../convex/_generated/dataModel";

interface PageChange {
  pageId: Id<"bepPages"> | "main";
  originalContent: string;
  currentContent: string;
  status: "modified" | "new" | "deleted";
  title: string;
}

interface EditContextValue {
  isEditMode: boolean;
  setEditMode: (value: boolean) => void;
  changes: Map<string, PageChange>;
  trackChange: (pageId: Id<"bepPages"> | "main", title: string, original: string, current: string) => void;
  trackNewPage: (tempId: string, title: string, slug: string, content: string) => void;
  trackDeletePage: (pageId: Id<"bepPages">, title: string) => void;
  discardChanges: () => void;
  hasChanges: boolean;
  openedAt: number; // Timestamp when edit mode was entered (for conflict detection)
}

const EditContext = createContext<EditContextValue | null>(null);

export function BepEditProvider({ children }: { children: ReactNode }) {
  const [isEditMode, setIsEditMode] = useState(false);
  const [changes, setChanges] = useState<Map<string, PageChange>>(new Map());
  const [openedAt, setOpenedAt] = useState(0);

  const setEditMode = useCallback((value: boolean) => {
    setIsEditMode(value);
    if (value) {
      setOpenedAt(Date.now());
    } else {
      setChanges(new Map());
      setOpenedAt(0);
    }
  }, []);

  const trackChange = useCallback(
    (pageId: Id<"bepPages"> | "main", title: string, original: string, current: string) => {
      setChanges((prev) => {
        const next = new Map(prev);
        if (original === current) {
          // No change, remove from tracking
          next.delete(String(pageId));
        } else {
          next.set(String(pageId), {
            pageId,
            originalContent: original,
            currentContent: current,
            status: "modified",
            title,
          });
        }
        return next;
      });
    },
    []
  );

  const trackNewPage = useCallback(
    (tempId: string, title: string, _slug: string, content: string) => {
      setChanges((prev) => {
        const next = new Map(prev);
        next.set(tempId, {
          pageId: "main", // Will be replaced on save
          originalContent: "",
          currentContent: content,
          status: "new",
          title,
        });
        return next;
      });
    },
    []
  );

  const trackDeletePage = useCallback(
    (pageId: Id<"bepPages">, title: string) => {
      setChanges((prev) => {
        const next = new Map(prev);
        next.set(String(pageId), {
          pageId,
          originalContent: "",
          currentContent: "",
          status: "deleted",
          title,
        });
        return next;
      });
    },
    []
  );

  const discardChanges = useCallback(() => {
    setChanges(new Map());
  }, []);

  const hasChanges = changes.size > 0;

  return (
    <EditContext.Provider
      value={{
        isEditMode,
        setEditMode,
        changes,
        trackChange,
        trackNewPage,
        trackDeletePage,
        discardChanges,
        hasChanges,
        openedAt,
      }}
    >
      {children}
    </EditContext.Provider>
  );
}

export function useEditContext() {
  const context = useContext(EditContext);
  if (!context) {
    throw new Error("useEditContext must be used within BepEditProvider");
  }
  return context;
}
