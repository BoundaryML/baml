import path from 'path'
import { TextDocument } from 'vscode-languageserver-textdocument'
import { convertToTextDocument, gatherFiles } from './fileUtils'
const BAML_SRC = 'baml_src'
import { URI } from 'vscode-uri'
import { ParserDatabase } from '../lib/wasm/lint'

export class BamlDirCache {
  private readonly cache: Map<string, FileCache> = new Map()
  private readonly parserCache: Map<string, ParserDatabase> = new Map()
  private __lastBamlDir: string | null = null

  public get lastBamlDir(): { root_path: string | null; cache: FileCache | null } {
    if (this.__lastBamlDir) {
      return { root_path: this.__lastBamlDir, cache: this.cache.get(this.__lastBamlDir) ?? null }
    } else {
      return { root_path: null, cache: null }
    }
  }

  public getBamlDir(textDocument: TextDocument): string | null {
    let currentPath = URI.parse(textDocument.uri).fsPath
    let tries = 0
    while (currentPath !== '/' && tries < 10) {
      currentPath = path.dirname(currentPath)
      if (path.basename(currentPath) === BAML_SRC) {
        return URI.file(currentPath).toString()
      }
      tries++ // because windows may be weird and not ahve "/" as root
    }
    console.error('No baml dir found')
    return null
  }

  private getFileCache(textDocument: TextDocument): FileCache | null {
    const key = this.getBamlDir(textDocument)
    if (!key) {
      return null
    }
    let cache = this.cache.get(key) ?? null
    if (cache) {
      this.__lastBamlDir = key
    }
    return cache
  }
  private createFileCacheIfNotExist(textDocument: TextDocument): FileCache | null {
    const key = this.getBamlDir(textDocument)
    let fileCache = this.getFileCache(textDocument)
    if (!fileCache && key) {
      console.log(`Creating file cache for ${key}`)
      fileCache = new FileCache()
      const allFiles = gatherFiles(key)
      allFiles.forEach((filePath) => {
        const doc = convertToTextDocument(filePath)
        fileCache?.addFile(doc)
      })
      this.cache.set(key, fileCache)
    } else if (!key) {
      console.error('Could not find parent directory')
    }
    return fileCache
  }

  public refreshDirectory(textDocument: TextDocument): void {
    try {
      console.log('refresh')
      const fileCache = this.createFileCacheIfNotExist(textDocument)
      const parentDir = this.getBamlDir(textDocument)
      if (fileCache && parentDir) {
        const allFiles = gatherFiles(parentDir)
        fileCache.getDocuments().forEach((doc) => {
          if (!allFiles.includes(doc.uri)) {
            console.log(`removing ${doc.uri}`)
            fileCache.removeFile(doc)
          }
        })
        // add and update
        allFiles.forEach((filePath) => {
          if (!fileCache?.getDocument(filePath)) {
            console.log(`adding ${filePath}`)
            const doc = convertToTextDocument(filePath)
            fileCache?.addFile(doc)
          } else {
            // update the cache
            const doc = convertToTextDocument(filePath)
            fileCache?.addFile(doc)
          }
        })
      } else {
        console.error('Could not find parent directory')
      }
    } catch (e: any) {
      if (e instanceof Error) {
        console.log(`Error refreshing directory: ${e.message} ${e.stack}`)
      } else {
        console.log(`Error refreshing directory: ${e}`)
      }
    }
  }

  public addDatabase(root_dir: string, database: ParserDatabase | undefined): void {
    if (database) {
      this.parserCache.set(root_dir, database)
    } else {
      this.parserCache.delete(root_dir)
    }
  }

  public addDocument(textDocument: TextDocument): void {
    try {
      const fileCache = this.createFileCacheIfNotExist(textDocument)
      fileCache?.addFile(textDocument)
    } catch (e: any) {
      if (e instanceof Error) {
        console.log(`Error adding doc: ${e.message} ${e.stack}`)
      }
    }
  }
  public removeDocument(textDocument: TextDocument): void {
    const fileCache = this.getFileCache(textDocument)
    fileCache?.removeFile(textDocument)
  }

  public getDocuments(textDocument: TextDocument) {
    const fileCache = this.getFileCache(textDocument)
    return fileCache?.getDocuments() ?? []
  }
}

export class FileCache {
  // document uri to the text doc
  private readonly cache: Map<string, TextDocument> = new Map()
  constructor() { }

  public addFile(textDocument: TextDocument) {
    this.cache.set(textDocument.uri, textDocument)
  }

  public removeFile(textDocument: TextDocument) {
    this.cache.delete(textDocument.uri)
  }

  public getDocuments() {
    return Array.from(this.cache.values() ?? [])
  }

  public getDocument(uri: string) {
    return this.cache.get(uri)
  }
}
