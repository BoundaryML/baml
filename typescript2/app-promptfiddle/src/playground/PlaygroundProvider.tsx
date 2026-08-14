// biome-ignore-all lint/style/useFilenamingConvention: Keep the existing public module path.
'use client';

import { atom, useAtom } from 'jotai';
import { useCallback, useMemo } from 'react';
import defaultBaml from './default.baml';

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

type PlaygroundState = {
  /**
   * Unified file map: filename -> content.
   * Text files (.baml, .toml, .json) store raw content strings.
   * Media files (.png, .jpg, etc.) store data-URL strings.
   */
  files: Record<string, string>;
  /** Replace the full file map and persist to localStorage. */
  setFiles: (files: Record<string, string>) => void;
  /** Reset files to defaults and clear localStorage. */
  resetFiles: () => void;
};

// ---------------------------------------------------------------------------
// Defaults
// ---------------------------------------------------------------------------

const STORAGE_KEY = 'baml-playground-files';
const MEDIA_STORAGE_KEY = 'baml-playground-media';
const OLD_STORAGE_KEY = 'baml-playground-code'; // migration from single-file

const DEFAULT_FILES: Record<string, string> = {
  'baml_src/main.baml': defaultBaml,
};

// ---------------------------------------------------------------------------
// Persistence helpers
// ---------------------------------------------------------------------------

function loadPersistedFiles(): Record<string, string> {
  if (typeof window === 'undefined') return { ...DEFAULT_FILES };

  const result: Record<string, string> = {};

  try {
    const saved = localStorage.getItem(STORAGE_KEY);
    if (saved) {
      const parsed = JSON.parse(saved);
      if (typeof parsed === 'object' && parsed !== null) {
        Object.assign(result, parsed);
      }
    }

    // Migrate from old single-file key if present
    if (Object.keys(result).length === 0) {
      const oldCode = localStorage.getItem(OLD_STORAGE_KEY);
      if (oldCode && oldCode.length > 0) {
        result['main.baml'] = oldCode;
        localStorage.removeItem(OLD_STORAGE_KEY);
      }
    }
  } catch {
    /* localStorage unavailable */
  }

  // Merge in any separately-persisted media files (migration from old split storage)
  try {
    const mediaSaved = localStorage.getItem(MEDIA_STORAGE_KEY);
    if (mediaSaved) {
      const parsed = JSON.parse(mediaSaved);
      if (typeof parsed === 'object' && parsed !== null) {
        Object.assign(result, parsed);
      }
      localStorage.removeItem(MEDIA_STORAGE_KEY);
    }
  } catch {}

  if (Object.keys(result).length === 0) {
    return { ...DEFAULT_FILES };
  }

  return result;
}

// ---------------------------------------------------------------------------
// Jotai atoms
// ---------------------------------------------------------------------------

export const filesAtom = atom<Record<string, string>>(loadPersistedFiles());

/**
 * Maps workspace-relative paths (e.g. "images/photo.png") to blob: URLs
 * that can be used directly in <img src> or CSS backgrounds.
 */
export const blobUrlsAtom = atom<Record<string, string>>({});

// ---------------------------------------------------------------------------
// Hook
// ---------------------------------------------------------------------------

export const usePlayground = (): PlaygroundState => {
  const [files, setFilesRaw] = useAtom(filesAtom);

  const setFiles = useCallback(
    (value: Record<string, string>) => {
      setFilesRaw(value);
      try {
        localStorage.setItem(STORAGE_KEY, JSON.stringify(value));
      } catch {
        /* localStorage full or unavailable */
      }
    },
    [setFilesRaw],
  );

  const resetFiles = useCallback(() => {
    const defaults = { ...DEFAULT_FILES };
    setFilesRaw(defaults);
    try {
      localStorage.removeItem(STORAGE_KEY);
      localStorage.removeItem(MEDIA_STORAGE_KEY);
      localStorage.removeItem(OLD_STORAGE_KEY);
    } catch {
      /* localStorage unavailable */
    }
  }, [setFilesRaw]);

  return useMemo<PlaygroundState>(
    () => ({ files, resetFiles, setFiles }),
    [files, setFiles, resetFiles],
  );
};
