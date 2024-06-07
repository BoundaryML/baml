import { type ClassValue, clsx } from 'clsx'
import { twMerge } from 'tailwind-merge'

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs))
}

export type BAMLProject = {
  id: string
  name: string
  description: string
  files: EditorFile[]
  filePath?: string
  // functionsWithTests: ParserDBFunctionTestModel[];
  testRunOutput?: any
}

type EditorFile = {
  path: string
  content: string
}
