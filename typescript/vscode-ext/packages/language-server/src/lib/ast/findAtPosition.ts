import type { Position, Range } from 'vscode-languageserver'
import type { TextDocument } from 'vscode-languageserver-textdocument'
import { MAX_SAFE_VALUE_i32 } from '../constants'

export function fullDocumentRange(document: TextDocument): Range {
  const lastLineId = document.lineCount - 1
  return {
    start: { line: 0, character: 0 },
    end: { line: lastLineId, character: MAX_SAFE_VALUE_i32 },
  }
}

export function getCurrentLine(document: TextDocument, line: number): string {
  return document.getText({
    start: { line: line, character: 0 },
    end: { line: line, character: MAX_SAFE_VALUE_i32 },
  })
}

export function isFirstInsideBlock(position: Position, currentLine: string): boolean {
  if (currentLine.trim().length === 0) {
    return true
  }

  const stringTilPosition = currentLine.slice(0, position.character)
  const matchArray = /\w+/.exec(stringTilPosition)

  if (!matchArray) {
    return true
  }
  return (
    matchArray.length === 1 &&
    matchArray.index !== undefined &&
    stringTilPosition.length - matchArray.index - matchArray[0].length === 0
  )
}

export function getWordAtPosition(document: TextDocument, position: Position): string {
  const currentLine = getCurrentLine(document, position.line)

  // search for the word's beginning and end
  const beginning: number = currentLine.slice(0, position.character + 1).search(/\S+$/)
  const end: number = currentLine.slice(position.character).search(/\W/)
  if (end < 0) {
    return ''
  }
  return currentLine.slice(beginning, end + position.character)
}

export function getSymbolBeforePosition(document: TextDocument, position: Position): string {
  return document.getText({
    start: {
      line: position.line,
      character: position.character - 1,
    },
    end: { line: position.line, character: position.character },
  })
}

export function getPositionFromIndex(document: TextDocument, index: number): Position {
  let line = 0
  let character = 0
  for (let i = 0; i < index; i++) {
    if (document.getText()[i] === '\n') {
      line++
      character = 0
    } else {
      character++
    }
  }

  return { line, character }
}
