import path from 'path'
import { TextDocument } from 'vscode-languageserver-textdocument'
import { convertToTextDocument, gatherFiles } from './fileUtils'
const BAML_SRC = 'baml_src'
import { URI } from 'vscode-uri'
import { ParserDatabase } from '@baml/common'
import { Position, Range } from 'vscode-languageserver'
import { getPositionFromIndex } from '../lib/ast'

export class BamlDirCache {
  private readonly cache: Map<string, FileCache> = new Map()
  private readonly parserCache: Map<string, ParserDatabase> = new Map()
  private __lastBamlDir: URI | null = null

  public get lastBamlDir(): { root_path: URI | null; cache: FileCache | null } {
    if (this.__lastBamlDir) {
      return { root_path: this.__lastBamlDir, cache: this.cache.get(this.__lastBamlDir.toString()) ?? null }
    } else {
      return { root_path: null, cache: null }
    }
  }

  public getBamlDir(textDocument: TextDocument): URI | null {
    let uri = URI.parse(textDocument.uri)
    return this.getBamlDirUri(uri)
  }

  private getBamlDirUri(uri: URI): URI | null {
    const MAX_TRIES = 10 // configurable maximum depth

    // Check if the scheme is 'file', return null for non-file schemes
    if (uri.scheme !== 'file') {
      console.error(`Unsupported URI scheme ${JSON.stringify(uri.toJSON(), null, 2)}`)
      return null
    }

    let currentPath = uri.fsPath
    let tries = 0

    while (path.isAbsolute(currentPath) && tries < MAX_TRIES) {
      if (path.basename(currentPath) === BAML_SRC) {
        return URI.file(currentPath)
      }
      currentPath = path.dirname(currentPath)
      tries++
    }

    console.error('No baml dir found within the specified depth')
    return null
  }

  public getFileCache(textDocument: TextDocument): FileCache | null {
    const key = this.getBamlDir(textDocument)
    if (!key) {
      return null
    }

    let cache = this.cache.get(key.toString()) ?? null
    if (cache) {
      this.__lastBamlDir = key
    }
    return cache
  }

  public getCacheForUri(uri: string): FileCache | null {
    const key = this.getBamlDirUri(URI.parse(uri))
    if (!key) {
      return null
    }

    let cache = this.cache.get(key.toString()) ?? null
    if (cache) {
      this.__lastBamlDir = key
    }
    return cache
  }

  public getParserDatabase(textDocument: TextDocument): ParserDatabase | undefined {
    const key = this.getBamlDir(textDocument)
    if (!key) {
      return undefined
    }

    return this.parserCache.get(key.toString())
  }

  private createFileCacheIfNotExist(textDocument: TextDocument): FileCache | null {
    const key = this.getBamlDir(textDocument)
    let fileCache = this.getFileCache(textDocument)
    if (!fileCache && key) {
      fileCache = new FileCache()
      const allFiles = gatherFiles(key)
      allFiles.forEach((filePath) => {
        const doc = convertToTextDocument(filePath)
        fileCache?.addFile(doc)
      })
      this.cache.set(key.toString(), fileCache)
    } else if (!key) {
      console.error('Could not find parent directory')
    }
    return fileCache
  }

  public refreshDirectory(textDocument: TextDocument): void {
    try {
      console.log('refreshDirectory')
      const fileCache = this.createFileCacheIfNotExist(textDocument)
      const parentDir = this.getBamlDir(textDocument)
      if (fileCache && parentDir) {
        const allFiles = gatherFiles(parentDir)
        if (allFiles.length === 0) {
          console.error('No files found')
          // try again with debug to find issues (temporary hack..)
          gatherFiles(parentDir, true)
        }

        // remove files that are no longer in the directory
        fileCache.getDocuments().forEach(({ path, doc }) => {
          if (!allFiles.find((a) => a.fsPath === path)) {
            fileCache.removeFile(doc)
          }
        })

        // add and update
        allFiles.forEach((filePath) => {
          if (!fileCache.getDocument(filePath)) {
            const doc = convertToTextDocument(filePath)
            fileCache.addFile(doc)
          } else {
            // update the cache
            const doc = convertToTextDocument(filePath)
            fileCache.addFile(doc)
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

  public addDatabase(root_dir: URI, database: ParserDatabase | undefined): void {
    if (database) {
      this.parserCache.set(root_dir.toString(), database)
    } else {
      this.parserCache.delete(root_dir.toString())
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

let counter = 0

type Definition = {
  name: string
  range: Range
  uri: URI
} & (
  | {
      type: 'class' | 'enum' | 'client'
    }
  | {
      type: 'function'
      input: string
      output: string
    }
)

export class FileCache {
  // document uri to the text doc
  private cache: Map<string, TextDocument>
  private cacheSummary: { path: string; doc: TextDocument }[]
  private __definitions: Map<string, Definition> = new Map()

  constructor() {
    this.cache = new Map()
    this.cacheSummary = new Array()
  }

  public addFile(textDocument: TextDocument) {
    this.cache.set(textDocument.uri, textDocument)
    this.cacheSummary = Array.from(this.cache).map(([uri, doc]) => ({
      path: URI.parse(uri).fsPath,
      doc: doc,
    }))
  }

  public removeFile(textDocument: TextDocument) {
    this.cache.delete(textDocument.uri)
    this.cacheSummary = Array.from(this.cache).map(([uri, doc]) => ({
      path: URI.parse(uri).fsPath,
      doc: doc,
    }))
  }

  public getDocuments() {
    return this.cacheSummary
  }

  public getDocument(uri: URI) {
    return this.cache.get(uri.toString())
  }

  public define(name: string) {
    return this.__definitions.get(name)
  }

  get definitions(): Array<Definition> {
    return Array.from(this.__definitions.values())
  }

  public setDB(parser: ParserDatabase) {
    this.__definitions.clear()
    ;[
      { type: 'enum', v: parser.enums },
      { type: 'class', v: parser.classes },
      { type: 'client', v: parser.clients },
      { type: 'functions', v: parser.functions },
    ].forEach(({ type, v }) => {
      v.forEach((e) => {
        let doc = this.getDocument(URI.file(e.name.source_file))
        if (!doc) {
          return
        }

        let start = getPositionFromIndex(doc, e.name.start)
        let end = getPositionFromIndex(doc, e.name.end)

        if (type === 'functions') {
          let func = e as ParserDatabase['functions'][0]
          const fromArgType = (arg: ParserDatabase['functions'][0]['input']) => {
            if (arg.arg_type === 'positional') {
              return `${arg.type}`
            } else {
              return arg.values.map((v) => `${v.name.value}: ${v.type}`).join(', ')
            }
          }
          this.__definitions.set(e.name.value, {
            name: e.name.value,
            range: { start, end },
            uri: URI.file(e.name.source_file),
            type: 'function',
            input: fromArgType(func.input),
            output: fromArgType(func.output),
          })
        } else {
          this.__definitions.set(e.name.value, {
            name: e.name.value,
            range: { start, end },
            uri: URI.file(e.name.source_file),
            type: type as 'enum' | 'class' | 'client',
          })
        }
      })
    })
  }
}
