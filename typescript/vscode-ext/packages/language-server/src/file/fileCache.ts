import path from "path";
import { TextDocument } from "vscode-languageserver-textdocument";
import { convertToTextDocument, gatherFiles } from "./fileUtils";

export class BamlDirCache {
  private readonly cache: Map<string, FileCache> = new Map();

  private getBamlDir(textDocument: TextDocument) {
    let currentPath = textDocument.uri;
    let parentDir: string | null = null;
    while (currentPath !== path.parse(currentPath).root) {
      currentPath = path.dirname(currentPath);
      if (path.basename(currentPath) === 'baml_src') {
        parentDir = currentPath;
        break;
      }
    }

    return parentDir;

  }

  private getFileCache(textDocument: TextDocument) {
    const key = this.getBamlDir(textDocument);
    if (!key) {
      console.error("No baml dir found")
      return null;
    }
    const fileCache = this.cache.get(key);
    return fileCache;
  }

  public addDocument(textDocument: TextDocument) {
    try {
      let fileCache = this.getFileCache(textDocument);
      if (!fileCache) {
        console.log("Creating file cache for " + this.getBamlDir(textDocument))
        fileCache = new FileCache();
        const parentDir = this.getBamlDir(textDocument);
        if (parentDir) {
          const allFiles = gatherFiles(parentDir);
          allFiles.forEach((filePath) => {
            const doc = convertToTextDocument(filePath);
            fileCache?.addFile(doc);
          });
        } else {
          console.error("Could not find parent directory");
          return;
        }
        this.cache.set(parentDir, fileCache);
      }
      fileCache?.addFile(textDocument);
    } catch (e: any) {
      if (e instanceof Error) {
        console.log("Error adding doc" + e.message + " " + e.stack);
      }
    }
  }

  public removeDocument(textDocument: TextDocument) {
    const fileCache = this.getFileCache(textDocument)
    if (fileCache) {
      fileCache.removeFile(textDocument);
    }
  }

  public getDocuments(textDocument: TextDocument) {
    const fileCache = this.getFileCache(textDocument);
    return fileCache?.getDocuments() ?? [];
  }
}

class FileCache {
  private readonly cache: Map<string, TextDocument> = new Map();
  constructor() { }

  public addFile(textDocument: TextDocument) {
    this.cache.set(textDocument.uri, textDocument);
  }

  public removeFile(textDocument: TextDocument) {
    this.cache.delete(textDocument.uri);
  }

  public getDocuments() {
    return Array.from(this.cache.values() ?? []);
  }
}