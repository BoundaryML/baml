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
import { fullDocumentRange } from './ast/findAtPosition'
import lint, { LinterInput } from './wasm/lint'
import { FileCache } from '../file/fileCache'
import generate_test_file, { GenerateResponse } from './wasm/generate_test_file'
import { ParserDatabase, TestRequest } from '@baml/common'

// import format from './prisma-schema-wasm/format'
// import lint from './prisma-schema-wasm/lint'

// import { convertDocumentTextToTrimmedLineArray } from './ast'

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
// import {
//   fullDocumentRange,
//   getWordAtPosition,
//   getBlockAtPosition,
//   Block,
//   getBlocks,
//   getDocumentationForBlock,
//   getDatamodelBlock,
// } from './ast'
export function handleGenerateTestFile(
  documents: TextDocument[],
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
  documents: TextDocument[],
  linterInput: LinterInput,
  onError?: (errorMessage: string) => void,
): { diagnostics: Map<string, Diagnostic[]>; state: ParserDatabase | undefined } {
  // console.debug(`Linting ${documents.length} documents`)
  const res = lint(linterInput, (errorMessage: string) => {
    if (onError) {
      onError(errorMessage)
    }
  })
  // console.log("res " + JSON.stringify(res, null, 2));

  let allDiagnostics: Map<string, Diagnostic[]> = new Map()

  documents.forEach((document) => {
    const documentDiagnostics: Diagnostic[] = []

    try {
      const filteredDiagnostics = res.diagnostics.filter((diag) => diag.source_file === document.uri)

      for (const diag of filteredDiagnostics) {
        const diagnostic: Diagnostic = {
          range: {
            start: document.positionAt(diag.start),
            end: document.positionAt(diag.end),
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

    allDiagnostics.set(document.uri, documentDiagnostics)
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

export function handleHoverRequest(document: TextDocument, params: HoverParams): Hover | undefined {
  const position = params.position

  // const lines = convertDocumentTextToTrimmedLineArray(document)
  // const word = getWordAtPosition(document, position)

  // if (word === '') {
  //   return
  // }

  // const block = getDatamodelBlock(word, lines)
  // if (!block) {
  //   return
  // }

  // const blockDocumentation = getDocumentationForBlock(document, block)

  // if (blockDocumentation.length !== 0) {
  //   return {
  //     contents: blockDocumentation.join('\n\n'),
  //   }
  // }

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

export function handleDocumentSymbol(params: DocumentSymbolParams, document: TextDocument): DocumentSymbol[] {
  // const lines: string[] = convertDocumentTextToTrimmedLineArray(document)
  // return Array.from(getBlocks(lines), (block) => ({
  //   kind: {
  //     model: SymbolKind.Class,
  //     enum: SymbolKind.Enum,
  //     type: SymbolKind.Interface,
  //     view: SymbolKind.Class,
  //     datasource: SymbolKind.Struct,
  //     generator: SymbolKind.Function,
  //   }[block.type],
  //   name: block.name,
  //   range: block.range,
  //   selectionRange: block.nameRange,
  // }))
  return []
}
