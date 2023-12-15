import {
  DocumentFormattingParams,
  TextEdit,
  DeclarationParams,
  CompletionParams,
  CompletionList,
  CompletionItem,
  HoverParams,
  Hover,
  CodeActionParams,
  CodeAction,
  Diagnostic,
  DiagnosticSeverity,
  RenameParams,
  WorkspaceEdit,
  DocumentSymbolParams,
  DocumentSymbol,
  SymbolKind,
  LocationLink,
  TextDocuments,
} from 'vscode-languageserver'
import type { TextDocument } from 'vscode-languageserver-textdocument'
import { fullDocumentRange } from './ast'
import lint, { LinterInput } from './wasm/lint'
import { FileCache } from '../file/fileCache'
import generate_test_file, { GenerateResponse } from './wasm/generate_test_file'
import { ParserDatabase, TestRequest } from '@baml/common'
import { URI } from 'vscode-uri'

// import format from './prisma-schema-wasm/format'
// import lint from './prisma-schema-wasm/lint'

import { convertDocumentTextToTrimmedLineArray } from './ast'

// import { quickFix } from './code-actions'
// import {
//   insertBasicRename,
//   renameReferencesForModelName,
//   isEnumValue,
//   renameReferencesForEnumValue,
//   isValidFieldName,
//   extractCurrentName,
//   mapExistsAlready,
//   insertMapAttribute,
//   renameReferencesForFieldValue,
//   printLogMessage,
//   isRelationField,
//   isBlockName,
// } from './code-actions/rename'
import {
  //   fullDocumentRange,
  getWordAtPosition,
  //   getBlockAtPosition,
  //   Block,
  //   getBlocks,
  //   getDocumentationForBlock,
  //   getDatamodelBlock,
} from './ast'
import { Range, Uri } from 'vscode'

export function handleGenerateTestFile(
  documents: { path: string; doc: TextDocument }[],
  linterInput: LinterInput,
  test_request: TestRequest,
  onError?: (errorMessage: string) => void,
): GenerateResponse {
  let result = generate_test_file(
    {
      ...linterInput,
      test_request: test_request,
    },
    (errorMessage: string) => {
      if (onError) {
        onError(errorMessage)
      }
    },
  )

  return result
}
export function handleDiagnosticsRequest(
  rootPath: URI,
  documents: { path: string; doc: TextDocument }[],
  onError?: (errorMessage: string) => void,
): { diagnostics: Map<string, Diagnostic[]>; state: ParserDatabase | undefined } {
  const linterInput: LinterInput = {
    root_path: rootPath.fsPath,
    files: documents.map(({ path, doc }) => ({
      path,
      content: doc.getText(),
    })),
  }


  console.debug(`Linting ${linterInput.files.length} files in ${linterInput.root_path}`)
  const res = lint(linterInput, (errorMessage: string) => {
    if (onError) {
      onError(errorMessage)
    }
  })

  let allDiagnostics: Map<string, Diagnostic[]> = new Map()

  documents.forEach((docDetails) => {
    const documentDiagnostics: Diagnostic[] = []

    try {
      const filteredDiagnostics = res.diagnostics.filter((diag) => diag.source_file === docDetails.path)

      for (const diag of filteredDiagnostics) {
        const diagnostic: Diagnostic = {
          range: {
            start: docDetails.doc.positionAt(diag.start),
            end: docDetails.doc.positionAt(diag.end),
          },
          message: diag.text,
          source: 'baml',
        }
        if (diag.is_warning) {
          diagnostic.severity = DiagnosticSeverity.Warning
        } else {
          diagnostic.severity = DiagnosticSeverity.Error
        }
        documentDiagnostics.push(diagnostic)
      }
    } catch (e: any) {
      if (e instanceof Error) {
        console.log('Error handling diagnostics' + e.message + ' ' + e.stack)
      }
      onError?.(e.message)
    }
    allDiagnostics.set(docDetails.doc.uri, documentDiagnostics)
  })

  return { diagnostics: allDiagnostics, state: res.ok ? res.response : undefined }
}

/**
 * This handler provides the modification to the document to be formatted.
 */
export function handleDocumentFormatting(
  params: DocumentFormattingParams,
  document: TextDocument,
  onError?: (errorMessage: string) => void,
): TextEdit[] {
  // const formatted = format(document.getText(), params, onError)
  // return [TextEdit.replace(fullDocumentRange(document), formatted)]
  return []
}

export function handleDefinitionRequest(
  fileCache: FileCache,
  document: TextDocument,
  params: DeclarationParams,
): LocationLink[] | undefined {
  const position = params.position

  const lines = convertDocumentTextToTrimmedLineArray(document)
  const word = getWordAtPosition(document, position)

  if (word === '') {
    return
  }

  // TODO: Do block level definitions
  let match = fileCache.define(word)

  if (match) {
    return [
      {
        targetUri: match.uri.toString(),
        targetRange: match.range,
        targetSelectionRange: match.range,
      },
    ]
  }
  return
}

export function handleHoverRequest(
  fileCache: FileCache,
  document: TextDocument,
  params: HoverParams,
): Hover | undefined {
  const position = params.position

  const lines = convertDocumentTextToTrimmedLineArray(document)
  const word = getWordAtPosition(document, position)

  if (word === '') {
    return
  }

  let match = fileCache.define(word)

  if (match) {
    if (match.type === 'function') {
      return {
        contents: {
          kind: 'markdown',
          value: `**${match.name}**\n\n(${match.input}) -> ${match.output}`,
        },
      }
    }
    return {
      contents: {
        kind: 'markdown',
        value: `**${match.name}**\n\n${match.type}`,
      },
    }
  }

  return
}

/**
 *
 * This handler provides the initial list of the completion items.
 */
export function handleCompletionRequest(
  params: CompletionParams,
  document: TextDocument,
  onError?: (errorMessage: string) => void,
): CompletionList | undefined {
  // return prismaSchemaWasmCompletions(params, document, onError) || localCompletions(params, document, onError)
  return undefined
}

export function handleRenameRequest(params: RenameParams, document: TextDocument): WorkspaceEdit | undefined {
  return undefined
}

/**
 *
 * @param item This handler resolves additional information for the item selected in the completion list.
 */
export function handleCompletionResolveRequest(item: CompletionItem): CompletionItem {
  return item
}

export function handleCodeActions(
  params: CodeActionParams,
  document: TextDocument,
  onError?: (errorMessage: string) => void,
): CodeAction[] {
  // if (!params.context.diagnostics.length) {
  //   return []
  // }

  // return quickFix(document, params, onError)
  return []
}

export function handleDocumentSymbol(
  fileCache: FileCache,
  params: DocumentSymbolParams,
  document: TextDocument,
): DocumentSymbol[] {
  // Since baml is global scope, we can just return all the definitions
  return fileCache.definitions
    .filter((def) => def.uri.toString() === document.uri)
    .map(({ name, range, uri, type }) => ({
      kind: {
        class: SymbolKind.Class,
        enum: SymbolKind.Enum,
        function: SymbolKind.Interface,
        client: SymbolKind.Object,
      }[type],
      name: name,
      range: range,
      selectionRange: range,
    }))
    .sort((a, b) => {
      // by kind first
      if (a.kind === b.kind) {
        return a.name.localeCompare(b.name)
      }
      // then by name
      return a.kind - b.kind
    })
}
