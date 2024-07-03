'use server'
import { promises as fs } from 'fs'
import path from 'path'
import { type EditorFile, loadUrl } from '@/app/actions'
import type { BAMLProject } from './exampleProjects'
export async function loadProject(params: { project_id: string }, chooseDefault = false) {
  const projectGroups = await loadExampleProjects()
  let data: BAMLProject = projectGroups.allProjects[0] //exampleProjects[0]
  const id = params.project_id
  if (id) {
    const exampleProject = await loadExampleProject(projectGroups, id)
    if (exampleProject) {
      data = exampleProject
    } else {
      data = await loadUrl(id)
    }
  } else {
    const exampleProject = projectGroups.allProjects[0]
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
  const exampleProjects = [...groupings.allProjects]
  if (groupings.newProject) {
    exampleProjects.push(groupings.newProject)
  }

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
  allProjects: BAMLProject[]
  newProject: BAMLProject
}

export async function loadExampleProjects(): Promise<BamlProjectsGroupings> {
  const exampleProjects: BamlProjectsGroupings = {
    allProjects: [
      {
        id: 'all-projects',
        name: 'BAML Examples',
        description: 'All BAML examples in one place',
        filePath: '/all-projects/',
        files: [],
      },
    ],
    newProject: {
      id: 'new-project',
      name: 'New Project',
      description: 'New project',
      filePath: '/new-project/',
      files: [],
    },
  }
  return exampleProjects
}
