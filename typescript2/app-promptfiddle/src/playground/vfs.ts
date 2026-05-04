import type { WasmVfsMetadata } from "@b/bridge_wasm";

const encoder = new TextEncoder();
const decoder = new TextDecoder();

const MEDIA_EXTENSIONS = new Set([
  "png", "jpg", "jpeg", "gif", "svg", "webp", "ico", "bmp",
  "mp3", "wav", "ogg",
  "mp4", "webm",
  "pdf",
]);

function isMediaFile(path: string): boolean {
  const ext = path.split(".").pop()?.toLowerCase() ?? "";
  return MEDIA_EXTENSIONS.has(ext);
}

/**
 * Callback fired when the WASM runtime mutates a file (write, copy, move,
 * delete). Paths are relative to the workspace root.
 *
 * For writes: `content` is the raw string (text) or data-URL (media).
 * For deletes: `deleted` is true and `content` is undefined.
 */
export type VfsChangeCallback = (
  change: { path: string; content: string; deleted?: false }
        | { path: string; deleted: true },
) => void;

/**
 * In-memory virtual filesystem that implements the WASM VFS interface.
 *
 * Stores both text files (BAML sources) and binary files (images, etc.)
 * and tracks explicitly-created directories. All paths are stored as
 * absolute paths internally (e.g. "/workspace/baml_src/main.baml").
 *
 * Text files (.baml, .toml, .json, etc.) are stored as UTF-8 bytes.
 * Media files (.png, .jpg, etc.) are decoded from data URLs to raw bytes.
 */
export class BamlVfs {
  private files = new Map<string, Uint8Array>();
  private dirs = new Set<string>();
  private rootPath: string;

  /** Suppressed during bulk setFiles to avoid echoing main-thread state back. */
  private suppressCallbacks = false;
  onChange: VfsChangeCallback | null = null;

  constructor(rootPath: string) {
    this.rootPath = rootPath;
    this.dirs.add(rootPath);
  }

  /** Convert a workspace-relative path to an absolute VFS path. */
  toAbsolute(relPath: string): string {
    if (relPath.startsWith("/")) return relPath;
    return `${this.rootPath}/${relPath}`;
  }

  // -----------------------------------------------------------------------
  // Bulk updates from main thread
  // -----------------------------------------------------------------------

  /**
   * Replace all files. Keys are relative paths.
   * Text files (e.g. "baml_src/main.baml") have raw content strings.
   * Media files (e.g. "images/photo.png") have data-URL strings.
   */
  setFiles(files: Record<string, string>): void {
    this.suppressCallbacks = true;
    try {
      const newAbsKeys = new Set<string>();
      for (const [rel, content] of Object.entries(files)) {
        const abs = this.toAbsolute(rel);
        newAbsKeys.add(abs);
        this.files.set(abs, isMediaFile(abs) ? dataUrlToBytes(content) : encoder.encode(content));
        this.ensureParentDirs(abs);
      }
      for (const abs of this.files.keys()) {
        if (!newAbsKeys.has(abs)) {
          this.files.delete(abs);
        }
      }
    } finally {
      this.suppressCallbacks = false;
    }
  }

  // -----------------------------------------------------------------------
  // WASM VFS interface — pass `this.wasmVfs` to BamlWasmRuntime.create()
  // -----------------------------------------------------------------------

  readonly wasmVfs = {
    readDir: (path: string): string[] => {
      const prefix = path.endsWith("/") ? path : path + "/";
      const children = new Set<string>();

      for (const p of this.files.keys()) {
        if (p.startsWith(prefix)) {
          const rest = p.slice(prefix.length);
          const slash = rest.indexOf("/");
          children.add(slash >= 0 ? rest.slice(0, slash) : rest);
        }
      }
      for (const d of this.dirs) {
        if (d.startsWith(prefix)) {
          const rest = d.slice(prefix.length);
          if (rest && !rest.includes("/")) {
            children.add(rest);
          }
        }
      }
      return Array.from(children);
    },

    createDir: (path: string): void => {
      this.dirs.add(path);
      this.ensureParentDirs(path);
    },

    exists: (path: string): boolean => {
      if (this.files.has(path) || this.dirs.has(path)) return true;
      const prefix = path + "/";
      for (const p of this.files.keys()) {
        if (p.startsWith(prefix)) return true;
      }
      return false;
    },

    readFile: (path: string): Uint8Array => {
      const data = this.files.get(path);
      if (!data) throw new Error(`readFile: not found: ${path}`);
      return data;
    },

    writeFile: (path: string, data: Uint8Array): void => {
      this.files.set(path, data);
      this.ensureParentDirs(path);
      this.notifyWrite(path, data);
    },

    metadata: (path: string): WasmVfsMetadata => {
      if (this.files.has(path)) {
        return {
          file_type: "file",
          len: this.files.get(path)!.length,
          created: undefined,
          modified: undefined,
          accessed: undefined,
        };
      }
      if (this.dirs.has(path)) {
        return {
          file_type: "directory",
          len: 0,
          created: undefined,
          modified: undefined,
          accessed: undefined,
        };
      }
      const prefix = path + "/";
      for (const p of this.files.keys()) {
        if (p.startsWith(prefix)) {
          return {
            file_type: "directory",
            len: 0,
            created: undefined,
            modified: undefined,
            accessed: undefined,
          };
        }
      }
      throw new Error(`metadata: not found: ${path}`);
    },

    removeFile: (path: string): void => {
      this.files.delete(path);
      this.notifyDelete(path);
    },

    removeDir: (path: string): void => {
      this.dirs.delete(path);
      const prefix = path + "/";
      for (const p of this.files.keys()) {
        if (p.startsWith(prefix)) {
          this.files.delete(p);
          this.notifyDelete(p);
        }
      }
      for (const d of this.dirs) {
        if (d.startsWith(prefix)) this.dirs.delete(d);
      }
    },

    setTime: (
      _type: "creation" | "modification" | "access",
      _path: string,
      _time: number,
    ): void => {
      // timestamps not tracked in the in-memory VFS
    },

    copyFile: (src: string, dest: string): void => {
      const data = this.files.get(src);
      if (!data) throw new Error(`copyFile: source not found: ${src}`);
      const copy = new Uint8Array(data);
      this.files.set(dest, copy);
      this.ensureParentDirs(dest);
      this.notifyWrite(dest, copy);
    },

    moveFile: (src: string, dest: string): void => {
      const data = this.files.get(src);
      if (!data) throw new Error(`moveFile: source not found: ${src}`);
      this.files.set(dest, data);
      this.files.delete(src);
      this.ensureParentDirs(dest);
      this.notifyDelete(src);
      this.notifyWrite(dest, data);
    },

    moveDir: (src: string, dest: string): void => {
      const srcPrefix = src + "/";
      const entries: [string, Uint8Array][] = [];
      for (const [p, data] of this.files) {
        if (p.startsWith(srcPrefix)) {
          entries.push([dest + "/" + p.slice(srcPrefix.length), data]);
          this.files.delete(p);
          this.notifyDelete(p);
        }
      }
      for (const [p, data] of entries) {
        this.files.set(p, data);
        this.notifyWrite(p, data);
      }
      this.dirs.delete(src);
      this.dirs.add(dest);
    },

    readMany: (glob: string): Array<[string, Uint8Array]> => {
      const pattern = globToRegex(glob);
      const results: [string, Uint8Array][] = [];
      for (const [absPath, bytes] of this.files) {
        if (pattern.test(absPath)) results.push([absPath, bytes]);
      }
      return results;
    },
  };

  // -----------------------------------------------------------------------
  // Internal helpers
  // -----------------------------------------------------------------------

  /** Convert an absolute VFS path back to a workspace-relative path. */
  private toRelative(absPath: string): string {
    const prefix = this.rootPath.endsWith("/") ? this.rootPath : this.rootPath + "/";
    if (absPath.startsWith(prefix)) return absPath.slice(prefix.length);
    return absPath;
  }

  /** Notify the main thread of a file write (text content or data URL). */
  private notifyWrite(absPath: string, bytes: Uint8Array): void {
    if (this.suppressCallbacks || !this.onChange) return;
    const rel = this.toRelative(absPath);
    const content = isMediaFile(absPath)
      ? bytesToDataUrl(bytes, absPath)
      : decoder.decode(bytes);
    this.onChange({ path: rel, content });
  }

  /** Notify the main thread of a file deletion. */
  private notifyDelete(absPath: string): void {
    if (this.suppressCallbacks || !this.onChange) return;
    this.onChange({ path: this.toRelative(absPath), deleted: true });
  }

  private ensureParentDirs(absPath: string): void {
    let i = absPath.lastIndexOf("/");
    while (i > 0) {
      const dir = absPath.slice(0, i);
      if (this.dirs.has(dir)) break;
      this.dirs.add(dir);
      i = dir.lastIndexOf("/");
    }
  }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

const MIME_TYPES: Record<string, string> = {
  png: "image/png", jpg: "image/jpeg", jpeg: "image/jpeg", gif: "image/gif",
  svg: "image/svg+xml", webp: "image/webp", ico: "image/x-icon", bmp: "image/bmp",
  mp3: "audio/mpeg", wav: "audio/wav", ogg: "audio/ogg",
  mp4: "video/mp4", webm: "video/webm",
  pdf: "application/pdf",
};

function bytesToDataUrl(bytes: Uint8Array, path: string): string {
  const ext = path.split(".").pop()?.toLowerCase() ?? "";
  const mime = MIME_TYPES[ext] ?? "application/octet-stream";
  let binary = "";
  for (const byte of bytes) binary += String.fromCharCode(byte);
  return `data:${mime};base64,${btoa(binary)}`;
}

/**
 * Convert a glob pattern to a RegExp.
 * Supports: `**` (any path), `*` (single segment), `?` (single char).
 */
function globToRegex(glob: string): RegExp {
  let re = "^";
  let i = 0;
  while (i < glob.length) {
    if (glob[i] === "*" && glob[i + 1] === "*") {
      re += ".*";
      i += 2;
      if (glob[i] === "/") i++;
    } else if (glob[i] === "*") {
      re += "[^/]*";
      i++;
    } else if (glob[i] === "?") {
      re += "[^/]";
      i++;
    } else {
      re += glob[i]!.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
      i++;
    }
  }
  return new RegExp(re + "$");
}

function dataUrlToBytes(dataUrl: string): Uint8Array {
  const commaIdx = dataUrl.indexOf(",");
  if (commaIdx < 0) return encoder.encode(dataUrl);
  const base64 = dataUrl.slice(commaIdx + 1);
  const binary = atob(base64);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) {
    bytes[i] = binary.charCodeAt(i);
  }
  return bytes;
}
