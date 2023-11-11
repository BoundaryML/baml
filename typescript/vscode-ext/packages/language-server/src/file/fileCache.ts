import path from "path";
import { TextDocument } from "vscode-languageserver-textdocument";
import { convertToTextDocument, gatherFiles } from "./fileUtils";
const BAML_SRC = 'baml_src';
import { URI } from 'vscode-uri';

export class BamlDirCache {
  private readonly cache: Map<string, FileCache> = new Map();

  public getBamlDir(textDocument: TextDocument): string | null {
    let currentPath = URI.parse(textDocument.uri).fsPath;
    let tries = 0;
    while (currentPath !== "/" && tries < 10) {
      currentPath = path.dirname(currentPath);
      if (path.basename(currentPath) === BAML_SRC) {
        return URI.file(currentPath).toString();
      }
      tries++; // because windows may be weird and not ahve "/" as root
    }
    console.error("No baml dir found");
    return null;
  }

  private getFileCache(textDocument: TextDocument): FileCache | null {
    const key = this.getBamlDir(textDocument);
    return this.cache.get(key ?? "") ?? null;
  }
  private createFileCacheIfNotExist(textDocument: TextDocument): FileCache | null {
    const key = this.getBamlDir(textDocument);
    let fileCache = this.getFileCache(textDocument);
    if (!fileCache && key) {
      console.log(`Creating file cache for ${key}`);
      fileCache = new FileCache();
      const allFiles = gatherFiles(key);
      allFiles.forEach((filePath) => {
        const doc = convertToTextDocument(filePath);
        fileCache?.addFile(doc);
      });
      this.cache.set(key, fileCache);
    } else if (!key) {
      console.error("Could not find parent directory");
    }
    return fileCache;
  }

  public refreshDirectory(textDocument: TextDocument): void {
    try {
      console.log("refresh")
      const fileCache = this.createFileCacheIfNotExist(textDocument);
      const parentDir = this.getBamlDir(textDocument);
      if (fileCache && parentDir) {
        console.log("bamlDir", parentDir)
        const allFiles = gatherFiles(parentDir);
        fileCache.getDocuments().forEach((doc) => {

          if (!allFiles.includes(doc.uri)) {
            fileCache.removeFile(doc);
          }
        });
      } else {
        console.error("Could not find parent directory");
      }
      console.log("end refresh");
    } catch (e: any) {
      if (e instanceof Error) {
        console.log(`Error refreshing directory: ${e.message} ${e.stack}`);
      } else {
        console.log(`Error refreshing directory: ${e}`);

      }
    }
  }

  public addDocument(textDocument: TextDocument): void {
    try {
      const fileCache = this.createFileCacheIfNotExist(textDocument);
      fileCache?.addFile(textDocument);
    } catch (e: any) {
      if (e instanceof Error) {
        console.log(`Error adding doc: ${e.message} ${e.stack}`);
      }
    }
  }
  public removeDocument(textDocument: TextDocument): void {
    const fileCache = this.getFileCache(textDocument);
    fileCache?.removeFile(textDocument);
  }

  public getDocuments(textDocument: TextDocument) {
    const fileCache = this.getFileCache(textDocument);
    return fileCache?.getDocuments() ?? [];
  }
}

export class FileCache {
  // document uri to the text doc
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

  public getDocument(uri: string) {
    return this.cache.get(uri);
  }
}