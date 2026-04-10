/**
 * Simplified in-memory virtual filesystem for the BAML WASM runtime.
 * Text-only (no media files). All paths stored as absolute paths internally.
 */

import type { WasmVfsMetadata } from '@/wasm/bridge_wasm';

const encoder = new TextEncoder();
const decoder = new TextDecoder();

export class BamlVfs {
  private files = new Map<string, Uint8Array>();
  private dirs = new Set<string>();
  private rootPath: string;

  constructor(rootPath: string) {
    this.rootPath = rootPath;
    this.dirs.add(rootPath);
  }

  toAbsolute(relPath: string): string {
    if (relPath.startsWith('/')) return relPath;
    return `${this.rootPath}/${relPath}`;
  }

  setFiles(files: Record<string, string>): void {
    const newAbsKeys = new Set<string>();
    for (const [rel, content] of Object.entries(files)) {
      const abs = this.toAbsolute(rel);
      newAbsKeys.add(abs);
      this.files.set(abs, encoder.encode(content));
      this.ensureParentDirs(abs);
    }
    for (const abs of this.files.keys()) {
      if (!newAbsKeys.has(abs)) {
        this.files.delete(abs);
      }
    }
  }

  readonly wasmVfs = {
    readDir: (path: string): string[] => {
      const prefix = path.endsWith('/') ? path : path + '/';
      const children = new Set<string>();
      for (const p of this.files.keys()) {
        if (p.startsWith(prefix)) {
          const rest = p.slice(prefix.length);
          const slash = rest.indexOf('/');
          children.add(slash >= 0 ? rest.slice(0, slash) : rest);
        }
      }
      for (const d of this.dirs) {
        if (d.startsWith(prefix)) {
          const rest = d.slice(prefix.length);
          if (rest && !rest.includes('/')) {
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
      const prefix = path + '/';
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
    },

    metadata: (path: string): WasmVfsMetadata => {
      if (this.files.has(path)) {
        return {
          file_type: 'file',
          len: this.files.get(path)!.length,
          created: undefined,
          modified: undefined,
          accessed: undefined,
        };
      }
      if (this.dirs.has(path)) {
        return { file_type: 'directory', len: 0, created: undefined, modified: undefined, accessed: undefined };
      }
      const prefix = path + '/';
      for (const p of this.files.keys()) {
        if (p.startsWith(prefix)) {
          return { file_type: 'directory', len: 0, created: undefined, modified: undefined, accessed: undefined };
        }
      }
      throw new Error(`metadata: not found: ${path}`);
    },

    removeFile: (path: string): void => {
      this.files.delete(path);
    },

    removeDir: (path: string): void => {
      this.dirs.delete(path);
      const prefix = path + '/';
      for (const p of this.files.keys()) {
        if (p.startsWith(prefix)) this.files.delete(p);
      }
      for (const d of this.dirs) {
        if (d.startsWith(prefix)) this.dirs.delete(d);
      }
    },

    setTime: (): void => {},

    copyFile: (src: string, dest: string): void => {
      const data = this.files.get(src);
      if (!data) throw new Error(`copyFile: source not found: ${src}`);
      this.files.set(dest, new Uint8Array(data));
      this.ensureParentDirs(dest);
    },

    moveFile: (src: string, dest: string): void => {
      const data = this.files.get(src);
      if (!data) throw new Error(`moveFile: source not found: ${src}`);
      this.files.set(dest, data);
      this.files.delete(src);
      this.ensureParentDirs(dest);
    },

    moveDir: (src: string, dest: string): void => {
      const srcPrefix = src + '/';
      const entries: [string, Uint8Array][] = [];
      for (const [p, data] of this.files) {
        if (p.startsWith(srcPrefix)) {
          entries.push([dest + '/' + p.slice(srcPrefix.length), data]);
          this.files.delete(p);
        }
      }
      for (const [p, data] of entries) {
        this.files.set(p, data);
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

  private ensureParentDirs(absPath: string): void {
    let i = absPath.lastIndexOf('/');
    while (i > 0) {
      const dir = absPath.slice(0, i);
      if (this.dirs.has(dir)) break;
      this.dirs.add(dir);
      i = dir.lastIndexOf('/');
    }
  }
}

function globToRegex(glob: string): RegExp {
  let re = '^';
  let i = 0;
  while (i < glob.length) {
    if (glob[i] === '*' && glob[i + 1] === '*') {
      re += '.*';
      i += 2;
      if (glob[i] === '/') i++;
    } else if (glob[i] === '*') {
      re += '[^/]*';
      i++;
    } else if (glob[i] === '?') {
      re += '[^/]';
      i++;
    } else {
      re += glob[i]!.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
      i++;
    }
  }
  return new RegExp(re + '$');
}
