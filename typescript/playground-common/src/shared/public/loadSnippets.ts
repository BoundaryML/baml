'use server'
import { promises as fs } from 'fs'
import path from 'path'
import { type EditorFile, loadUrl } from './actions'
import type { BAMLSnippet } from './snippetProjects'
export async function loadProject(params: { project_id: string }, chooseDefault = false) {
  const projectGroups = await loadExampleProjects()
  let data: BAMLSnippet = projectGroups.snippets[0] //exampleProjects[0]
  const id = params.project_id
  if (id) {
    const exampleProject = await loadExampleProject(projectGroups, id)
    if (exampleProject) {
      data = exampleProject
    } else {
      data = await loadUrl(id)
    }
  } else {
    const exampleProject = projectGroups.snippets[0]
    const loadedProject = await loadExampleProject(projectGroups, exampleProject.id)
    if (loadedProject) {
      data = loadedProject
    }
  }
  return data
}

export async function loadExampleProject(
  groupings: BamlSnippetsGroupings,
  projectId: string,
): Promise<BAMLSnippet | undefined> {
  // Combine all projects into a single array
  const exampleProjects = groupings.snippets

  // Search for the project by id
  const proj = exampleProjects.find((project) => project.id === projectId)
  if (proj) {
    if (!proj.filePath) {
      throw new Error(`Example Project ${projectId} does not have a file path`)
    }

    return {
      ...proj,
      file: await getSnippetFile(proj.filePath),
    }
  }
}

const getSnippetFile = async (projectPath: string): Promise<EditorFile> => {
  const examplesPath = path.join(process.cwd(), 'snippetFiles')
  const filePath = path.join(examplesPath, projectPath)

  let fileContent: string | null = null
  let error: Error | null = null

  try {
    fileContent = await fs.readFile(filePath, 'utf8')
  } catch (err) {
    error = err as Error
  }

  if (!fileContent) {
    throw new Error(`No file found at path ${filePath}`)
  }

  return { path: filePath, content: fileContent }
}

export type BamlSnippetsGroupings = {
  snippets: BAMLSnippet[]
}

export async function loadExampleProjects(): Promise<BamlSnippetsGroupings> {
  const exampleProjects: BamlSnippetsGroupings = {
    snippets: [
      {
        id: 'system_user_prompts',
        name: 'System vs user prompts',
        description: `Configuring roles in LLM prompts enhances the effectiveness and reliability of interactions with language models. Use the {{ _.role()}} keyword to get started.`,
        filePath: 'system_user_prompts',
        // : 'function ClassifyMessage(input: string) -> Category {\n    client GPT4Turbo\n  \n    prompt #"\n      {# _.role("system") starts a system message #}\n      {{ _.role("system") }}\n  \n      Classify the following INPUT into ONE\n      of the following categories:\n  \n      {{ ctx.output_format }}\n  \n      {# This starts a user message #}\n      {{ _.role("user") }}\n  \n      INPUT: {{ input }}\n  \n      Response:\n    "#\n  }'
      },
      // {
      //   id: "test_ai_function",
      //   name: 'Test an AI function',
      //   text1: `There are two types of tests you may want to run on your AI functions: - Unit Tests: Tests a single AI function (using the playground) - Integration Tests: Tests a pipeline of AI functions and potentially business logic`,
      //   text2: 'dynamic_clients Text 2'
      // },
      // {
      //   id: "evaluate_results",
      //   name: 'Evaluate results with assertions or LLM Evals',
      //   text1: 'Third client_options Text 1',
      //   text2: 'Third client_options Text 2'
      // },
      // {
      //   id: "starting_page",
      //   name: '',
      //   text1: 'SPs Text 1',
      //   text2: 'Third client_options Text 2'
      // },
      // Add more components as neededstarting_page
    ],
  }
  // Add more components as neededstarting_p
  return exampleProjects
}
