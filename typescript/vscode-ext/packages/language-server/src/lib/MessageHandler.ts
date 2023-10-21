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

export function handleDiagnosticsRequest(
  documents: TextDocuments<TextDocument>,
  linterInput: LinterInput,
  onError?: (errorMessage: string) => void,
): Diagnostic[] {

  const res = lint(linterInput, (errorMessage: string) => {
    if (onError) {
      onError(errorMessage)
    }
  })

  let allDiagnostics: Diagnostic[] = []

  documents.all().forEach(document => {
    const documentDiagnostics: Diagnostic[] = []

    if (
      res.some(
        (diagnostic) =>
          diagnostic.text === "Field declarations don't require a `:`." ||
          diagnostic.text === 'Model declarations have to be indicated with the `model` keyword.',
      )
    ) {
      if (onError) {
        onError("Unexpected error.")
      }
    }

    try {
      const filteredDiagnostics = res.filter((diag) => diag.source_file === document.uri)

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
        console.log("Error handling diagnostics" + e.message + " " + e.stack);
      }
    }

    allDiagnostics = allDiagnostics.concat(documentDiagnostics)
  })

  return allDiagnostics
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
