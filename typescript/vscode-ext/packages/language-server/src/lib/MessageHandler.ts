import { ParserDatabase, TestRequest } from '@baml/common'
import {
  type CodeAction,
  type CodeActionParams,
  type CompletionItem,
  type CompletionList,
  type CompletionParams,
  type DeclarationParams,
  Diagnostic,
  DiagnosticSeverity,
  type DocumentFormattingParams,
  type DocumentSymbol,
  type DocumentSymbolParams,
  type Hover,
  type HoverParams,
  type LocationLink,
  type RenameParams,
  SymbolKind,
  TextDocuments,
  type TextEdit,
  type WorkspaceEdit,
} from 'vscode-languageserver'
import { TextDocument } from 'vscode-languageserver-textdocument'
import { URI } from 'vscode-uri'
import type { FileCache } from '../file/fileCache'
import { fullDocumentRange } from './ast'

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
// import { Range, Uri } from 'vscode'
import BamlProjectManager from './baml_project_manager'

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
  let result = lines.join('\n');
  console.log(result)
  const newDoc =  TextDocument.create(document.uri, document.languageId, document.version, result);
  const word = getWordAtPosition(newDoc, position)
  console.log('handleDefinitionRequest')
  if (word === '') {
    console.log('word is empty')
    return
  }
  console.log('word is not empty')
  console.log(word)

  // TODO: Do block level definitions
  const match = fileCache.define(word)
  
  console.log(match)
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

  const match = fileCache.define(word)

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
