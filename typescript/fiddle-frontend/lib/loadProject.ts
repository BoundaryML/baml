'use server'
import { promises as fs } from 'fs'
import path from 'path'
import { type EditorFile, loadUrl } from '@/app/actions'
import type { BAMLProject } from './exampleProjects'
export async function loadProject(params: { project_id: string }, chooseDefault = false) {
  const projectGroups = await loadExampleProjects()
  let data: BAMLProject = projectGroups.intros[0] //exampleProjects[0]
  const id = params.project_id
  if (id) {
    const exampleProject = await loadExampleProject(projectGroups, id)
    if (exampleProject) {
      data = exampleProject
    } else {
      data = await loadUrl(id)
    }
  } else {
    const exampleProject = projectGroups.intros[0]
    const loadedProject = await loadExampleProject(projectGroups, exampleProject.id)
    if (loadedProject) {
      data = loadedProject
    }
  }
  return data
}

interface FileContent {
  path: string
  content: string | null
  error: Error | null
}

interface FileContent {
  path: string
  content: string | null
  error: Error | null
}

async function loadExampleProject(
  groupings: BamlProjectsGroupings,
  projectId: string,
): Promise<BAMLProject | undefined> {
  // Combine all projects into a single array
  const exampleProjects = [...groupings.intros, ...groupings.advancedPromptSyntax, ...groupings.promptEngineering]

  // Search for the project by id
  const proj = exampleProjects.find((project) => project.id === projectId)
  if (proj) {
    if (!proj.filePath) {
      throw new Error(`Example Project ${projectId} does not have a file path`)
    }

    return {
      ...proj,
      files: await getProjectFiles(proj.filePath),
    }
  }
}

async function getAllFiles(dirPath: string, arrayOfFiles: FileContent[] = []): Promise<FileContent[]> {
  try {
    const files = await fs.readdir(dirPath)

    for (const file of files) {
      const filePath = path.join(dirPath, file)
      try {
        const stats = await fs.stat(filePath)
        if (stats.isDirectory()) {
          arrayOfFiles = await getAllFiles(filePath, arrayOfFiles)
        } else {
          const content = await fs.readFile(filePath, 'utf8')
          arrayOfFiles.push({ path: filePath, content, error: null })
        }
      } catch (error) {
        arrayOfFiles.push({ path: filePath, content: null, error: error as Error })
      }
    }
  } catch (error) {
    console.error(`Failed to access directory ${dirPath}: ${(error as Error).message}`)
  }

  return arrayOfFiles
}

const getProjectFiles = async (projectPath: string): Promise<EditorFile[]> => {
  const examplesPath = path.join(process.cwd(), 'public/_examples')
  const projPath = path.join(examplesPath, projectPath)
  const files = await getAllFiles(projPath)
  if (files.length === 0) {
    throw new Error(`No files found in project path ${projPath}`)
  }
  return files.map((f) => ({ path: f.path.replace(projPath, ''), content: f.content ?? '', error: f.error ?? null }))
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
        files: [],
      },
      {
        id: 'classify-message',
        name: 'ClassifyMessage',
        description: 'Classify a message from a user',
        filePath: '/intro/classify-message/',
        files: [],
      },
      {
        id: 'chat-roles',
        name: 'ChatRoles',
        description: 'Use a sequence of system and user messages',
        filePath: '/intro/chat-roles/',
        files: [],
      },
      // {
      //   id: 'images',
      //   name: 'Using Vision / Image APIs',
      //   description: 'Extract resume from image',
      //   filePath: '/intro/images/',
      //   files: [],
      // },
    ],
    advancedPromptSyntax: [],
    promptEngineering: [
      {
        id: 'chain-of-thought',
        name: 'Chain of Thought',
        description: 'Using chain of thought to improve results and reduce hallucinations',
        filePath: '/prompt-engineering/chain-of-thought/',
        files: [],
      },
      {
        id: 'symbol-tuning',
        name: 'Symbol Tuning',
        filePath: '/prompt-engineering/symbol-tuning/',
        description: 'Use symbol tuning to remove biases on schema property names',
        files: [],
      },
    ],
  }
  return exampleProjects
}
