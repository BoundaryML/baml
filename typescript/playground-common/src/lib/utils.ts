import { type ClassValue, clsx } from 'clsx'
import { twMerge } from 'tailwind-merge'

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs))
}

export type BAMLProject = {
  id: string
  name: string
  description: string
  file: EditorFile
  filePath?: string
  // functionsWithTests: ParserDBFunctionTestModel[];
  testRunOutput?: any
}

type EditorFile = {
  path: string
  content: string
}

export type BamlProjectsGroupings = {
  intros: BAMLProject[]
  advancedPromptSyntax: BAMLProject[]
  promptEngineering: BAMLProject[]
}

export async function loadExampleProjects(): Promise<BamlProjectsGroupings> {
  const exampleProjects: BamlProjectsGroupings = {
    intros: [
      {
        id: 'extract-resume',
        name: 'Introduction to BAML',
        description: 'A simple LLM function extract a resume',
        filePath: '/intro/extract-resume/',
        file: { path: '', content: '' },
      },
      {
        id: 'classify-message',
        name: 'ClassifyMessage',
        description: 'Classify a message from a user',
        filePath: '/intro/classify-message/',
        file: { path: '', content: '' },
      },
      {
        id: 'chat-roles',
        name: 'ChatRoles',
        description: 'Use a sequence of system and user messages',
        filePath: '/intro/chat-roles/',
        file: { path: '', content: '' },
      },
    ],
    advancedPromptSyntax: [],
    promptEngineering: [
      {
        id: 'chain-of-thought',
        name: 'Chain of Thought',
        description: 'Using chain of thought to improve results and reduce hallucinations',
        filePath: '/prompt-engineering/chain-of-thought/',
        file: { path: '', content: '' },
      },
      {
        id: 'symbol-tuning',
        name: 'Symbol Tuning',
        filePath: '/prompt-engineering/symbol-tuning/',
        description: 'Use symbol tuning to remove biases on schema property names',
        file: { path: '', content: '' },
      },
    ],
  }
  return exampleProjects
}
