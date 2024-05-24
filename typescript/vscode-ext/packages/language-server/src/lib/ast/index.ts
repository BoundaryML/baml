import type { TextDocument } from 'vscode-languageserver-textdocument'
import { getCurrentLine } from './findAtPosition'

export * from './findAtPosition'

export function convertDocumentTextToTrimmedLineArray(document: TextDocument): string[] {
  return Array(document.lineCount)
    .fill(0)
    .map((_, i) => getCurrentLine(document, i).trim())
}

export function trimLine(line: string): string {
  return line.trim().replace(/[^a-zA-Z0-9_.]/g, '')
}
