// ../typescript2/app-website/playground/vfs.ts
var encoder = new TextEncoder();
var decoder = new TextDecoder();
var MEDIA_EXTENSIONS = /* @__PURE__ */ new Set([
  "png",
  "jpg",
  "jpeg",
  "gif",
  "svg",
  "webp",
  "ico",
  "bmp",
  "mp3",
  "wav",
  "ogg",
  "mp4",
  "webm",
  "pdf"
]);
function isMediaFile(path) {
  const ext = path.split(".").pop()?.toLowerCase() ?? "";
  return MEDIA_EXTENSIONS.has(ext);
}
var BamlVfs = class {
  constructor(rootPath) {
    this.files = /* @__PURE__ */ new Map();
    this.dirs = /* @__PURE__ */ new Set();
    /** Suppressed during bulk setFiles to avoid echoing main-thread state back. */
    this.suppressCallbacks = false;
    this.onChange = null;
    // -----------------------------------------------------------------------
    // WASM VFS interface — pass `this.wasmVfs` to BamlWasmRuntime.create()
    // -----------------------------------------------------------------------
    this.wasmVfs = {
      readDir: (path) => {
        const prefix = path.endsWith("/") ? path : path + "/";
        const children = /* @__PURE__ */ new Set();
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
      /** Single-round-trip directory listing with type info (added in canary).  */
      readDirEntries: (path) => {
        const prefix = path.endsWith("/") ? path : path + "/";
        const seen = /* @__PURE__ */ new Map();
        for (const p of this.files.keys()) {
          if (p.startsWith(prefix)) {
            const rest = p.slice(prefix.length);
            const slash = rest.indexOf("/");
            if (slash >= 0) {
              seen.set(rest.slice(0, slash), "directory");
            } else if (!seen.has(rest)) {
              seen.set(rest, "file");
            }
          }
        }
        for (const d of this.dirs) {
          if (d.startsWith(prefix)) {
            const rest = d.slice(prefix.length);
            if (rest && !rest.includes("/") && !seen.has(rest)) {
              seen.set(rest, "directory");
            }
          }
        }
        return Array.from(seen.entries()).map(([name, file_type]) => ({
          name,
          file_type,
          is_symlink: false
        }));
      },
      createDir: (path) => {
        this.dirs.add(path);
        this.ensureParentDirs(path);
      },
      exists: (path) => {
        if (this.files.has(path) || this.dirs.has(path)) return true;
        const prefix = path + "/";
        for (const p of this.files.keys()) {
          if (p.startsWith(prefix)) return true;
        }
        return false;
      },
      readFile: (path) => {
        const data = this.files.get(path);
        if (!data) throw new Error(`readFile: not found: ${path}`);
        return data;
      },
      writeFile: (path, data) => {
        this.files.set(path, data);
        this.ensureParentDirs(path);
        this.notifyWrite(path, data);
      },
      metadata: (path) => {
        if (this.files.has(path)) {
          return {
            file_type: "file",
            len: this.files.get(path).length,
            created: void 0,
            modified: void 0,
            accessed: void 0
          };
        }
        if (this.dirs.has(path)) {
          return {
            file_type: "directory",
            len: 0,
            created: void 0,
            modified: void 0,
            accessed: void 0
          };
        }
        const prefix = path + "/";
        for (const p of this.files.keys()) {
          if (p.startsWith(prefix)) {
            return {
              file_type: "directory",
              len: 0,
              created: void 0,
              modified: void 0,
              accessed: void 0
            };
          }
        }
        throw new Error(`metadata: not found: ${path}`);
      },
      removeFile: (path) => {
        this.files.delete(path);
        this.notifyDelete(path);
      },
      removeDir: (path) => {
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
      setTime: (_type, _path, _time) => {
      },
      copyFile: (src, dest) => {
        const data = this.files.get(src);
        if (!data) throw new Error(`copyFile: source not found: ${src}`);
        const copy = new Uint8Array(data);
        this.files.set(dest, copy);
        this.ensureParentDirs(dest);
        this.notifyWrite(dest, copy);
      },
      moveFile: (src, dest) => {
        const data = this.files.get(src);
        if (!data) throw new Error(`moveFile: source not found: ${src}`);
        this.files.set(dest, data);
        this.files.delete(src);
        this.ensureParentDirs(dest);
        this.notifyDelete(src);
        this.notifyWrite(dest, data);
      },
      moveDir: (src, dest) => {
        const srcPrefix = src + "/";
        const entries = [];
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
      readMany: (glob) => {
        const pattern = globToRegex(glob);
        const results = [];
        for (const [absPath, bytes] of this.files) {
          if (pattern.test(absPath)) results.push([absPath, bytes]);
        }
        return results;
      }
    };
    this.rootPath = rootPath;
    this.dirs.add(rootPath);
  }
  /** Convert a workspace-relative path to an absolute VFS path. */
  toAbsolute(relPath) {
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
  setFiles(files) {
    this.suppressCallbacks = true;
    try {
      const newAbsKeys = /* @__PURE__ */ new Set();
      for (const [rel, content] of Object.entries(files)) {
        const abs = this.toAbsolute(rel);
        newAbsKeys.add(abs);
        this.files.set(
          abs,
          isMediaFile(abs) ? dataUrlToBytes(content) : encoder.encode(content)
        );
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
  // Internal helpers
  // -----------------------------------------------------------------------
  /** Convert an absolute VFS path back to a workspace-relative path. */
  toRelative(absPath) {
    const prefix = this.rootPath.endsWith("/") ? this.rootPath : this.rootPath + "/";
    if (absPath.startsWith(prefix)) return absPath.slice(prefix.length);
    return absPath;
  }
  /** Notify the main thread of a file write (text content or data URL). */
  notifyWrite(absPath, bytes) {
    if (this.suppressCallbacks || !this.onChange) return;
    const rel = this.toRelative(absPath);
    const content = isMediaFile(absPath) ? bytesToDataUrl(bytes, absPath) : decoder.decode(bytes);
    this.onChange({ path: rel, content });
  }
  /** Notify the main thread of a file deletion. */
  notifyDelete(absPath) {
    if (this.suppressCallbacks || !this.onChange) return;
    this.onChange({ path: this.toRelative(absPath), deleted: true });
  }
  ensureParentDirs(absPath) {
    let i = absPath.lastIndexOf("/");
    while (i > 0) {
      const dir = absPath.slice(0, i);
      if (this.dirs.has(dir)) break;
      this.dirs.add(dir);
      i = dir.lastIndexOf("/");
    }
  }
};
var MIME_TYPES = {
  png: "image/png",
  jpg: "image/jpeg",
  jpeg: "image/jpeg",
  gif: "image/gif",
  svg: "image/svg+xml",
  webp: "image/webp",
  ico: "image/x-icon",
  bmp: "image/bmp",
  mp3: "audio/mpeg",
  wav: "audio/wav",
  ogg: "audio/ogg",
  mp4: "video/mp4",
  webm: "video/webm",
  pdf: "application/pdf"
};
function bytesToDataUrl(bytes, path) {
  const ext = path.split(".").pop()?.toLowerCase() ?? "";
  const mime = MIME_TYPES[ext] ?? "application/octet-stream";
  let binary = "";
  for (const byte of bytes) binary += String.fromCharCode(byte);
  return `data:${mime};base64,${btoa(binary)}`;
}
function globToRegex(glob) {
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
      re += glob[i].replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
      i++;
    }
  }
  return new RegExp(re + "$");
}
function dataUrlToBytes(dataUrl) {
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
export {
  BamlVfs
};
