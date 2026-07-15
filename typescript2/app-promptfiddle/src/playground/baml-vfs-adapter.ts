/**
 * BamlVfsAdapter — wraps BamlVfs.wasmVfs in the IFileSystem interface
 * expected by just-bash, so shell commands can read/write playground files.
 */

import type {
  IFileSystem,
  FsStat,
  MkdirOptions,
  RmOptions,
  CpOptions,
} from "just-bash/browser";

type WasmVfsDirEntry = {
  name: string;
  file_type: string;
  is_symlink: boolean;
};

/**
 * The wasmVfs property shape exposed by BamlVfs.
 * We declare a minimal structural type here to avoid importing BamlVfs
 * directly (which would pull in the full WASM bootstrap in tests).
 */
interface WasmVfsLike {
  readFile(path: string): Uint8Array;
  writeFile(path: string, data: Uint8Array): void;
  exists(path: string): boolean;
  readDir(path: string): string[];
  readDirEntries(path: string): WasmVfsDirEntry[];
  createDir(path: string): void;
  removeFile(path: string): void;
  removeDir(path: string): void;
  metadata(path: string): {
    file_type: string;
    len: number;
    created: number | undefined;
    modified: number | undefined;
    accessed: number | undefined;
  };
  copyFile(src: string, dest: string): void;
  moveFile(src: string, dest: string): void;
  moveDir(src: string, dest: string): void;
  setTime(
    type: "creation" | "modification" | "access",
    path: string,
    time: number,
  ): void;
}

const enc = new TextEncoder();
const dec = new TextDecoder();

/**
 * Implements the just-bash IFileSystem interface backed by BamlVfs.wasmVfs.
 *
 * The underlying wasmVfs methods are synchronous; we wrap them in
 * Promise.resolve() to satisfy the async interface.
 */
export class BamlVfsAdapter implements IFileSystem {
  constructor(private readonly vfs: WasmVfsLike) {}

  async readFile(path: string): Promise<string> {
    return dec.decode(this.vfs.readFile(path));
  }

  async readFileBuffer(path: string): Promise<Uint8Array> {
    return this.vfs.readFile(path);
  }

  async writeFile(
    path: string,
    content: string | Uint8Array,
  ): Promise<void> {
    const bytes =
      typeof content === "string" ? enc.encode(content) : content;
    this.vfs.writeFile(path, bytes);
  }

  async appendFile(
    path: string,
    content: string | Uint8Array,
  ): Promise<void> {
    let existing: Uint8Array;
    try {
      existing = this.vfs.readFile(path);
    } catch {
      existing = new Uint8Array(0);
    }
    const appendBytes =
      typeof content === "string" ? enc.encode(content) : content;
    const merged = new Uint8Array(existing.length + appendBytes.length);
    merged.set(existing, 0);
    merged.set(appendBytes, existing.length);
    this.vfs.writeFile(path, merged);
  }

  async exists(path: string): Promise<boolean> {
    return this.vfs.exists(path);
  }

  async stat(path: string): Promise<FsStat> {
    const meta = this.vfs.metadata(path);
    return {
      isFile: meta.file_type === "file",
      isDirectory: meta.file_type === "directory",
      isSymbolicLink: false,
      mode: 0o644,
      size: meta.len,
      mtime: new Date(meta.modified ?? 0),
    };
  }

  async mkdir(path: string, _opts?: MkdirOptions): Promise<void> {
    this.vfs.createDir(path);
  }

  async readdir(path: string): Promise<string[]> {
    return this.vfs.readDir(path);
  }

  async rm(path: string, opts?: RmOptions): Promise<void> {
    let meta: { file_type: string } | null = null;
    try {
      meta = this.vfs.metadata(path);
    } catch {
      if (opts?.force) return;
      throw new Error(`rm: path not found: ${path}`);
    }
    if (meta.file_type === "directory") {
      this.vfs.removeDir(path);
    } else {
      this.vfs.removeFile(path);
    }
  }

  async cp(src: string, dest: string, _opts?: CpOptions): Promise<void> {
    this.vfs.copyFile(src, dest);
  }

  async mv(src: string, dest: string): Promise<void> {
    // Try file first, fall back to directory
    let meta: { file_type: string } | null = null;
    try {
      meta = this.vfs.metadata(src);
    } catch {
      throw new Error(`mv: path not found: ${src}`);
    }
    if (meta.file_type === "directory") {
      this.vfs.moveDir(src, dest);
    } else {
      this.vfs.moveFile(src, dest);
    }
  }

  resolvePath(base: string, path: string): string {
    if (path.startsWith("/")) return path;
    return base.replace(/\/$/, "") + "/" + path;
  }

  getAllPaths(): string[] {
    // Not enumerable from the wasmVfs API without walking
    return [];
  }

  async chmod(_path: string, _mode: number): Promise<void> {
    // No-op: wasmVfs doesn't track permissions
  }

  async symlink(_target: string, _linkPath: string): Promise<void> {
    throw new Error("EPERM: symlinks not supported in BamlVfs");
  }

  async link(_existing: string, _newPath: string): Promise<void> {
    throw new Error("EPERM: hard links not supported in BamlVfs");
  }

  async readlink(_path: string): Promise<string> {
    throw new Error("EINVAL: not a symbolic link");
  }

  async lstat(path: string): Promise<FsStat> {
    return this.stat(path);
  }

  async realpath(path: string): Promise<string> {
    return path;
  }

  async utimes(
    path: string,
    atime: Date,
    mtime: Date,
  ): Promise<void> {
    try {
      this.vfs.setTime("modification", path, mtime.getTime());
      this.vfs.setTime("access", path, atime.getTime());
    } catch {
      // Ignore if not supported
    }
  }
}
