"use server"
import { promises as fs } from 'fs';
import path from 'path';
import { EditorFile, loadUrl } from "@/app/actions"
import { BAMLProject } from "./exampleProjects"
export async function loadProject(params: { project_id: string }, chooseDefault: boolean = false) {
  const projectGroups = (await loadExampleProjects());
  let data: BAMLProject = projectGroups.intros[0] //exampleProjects[0]
  const id = params.project_id
  if (id) {
    const exampleProject = findBamlProjectById(projectGroups, id)
    if (exampleProject) {
      data = exampleProject
    } else {
      data = await loadUrl(id)
    }
  }
  return data
}

interface FileContent {
  path: string;
  content: string | null;
  error: Error | null;
}

interface FileContent {
  path: string;
  content: string | null;
  error: Error | null;
}


function findBamlProjectById(groupings: BamlProjectsGroupings, projectId: string): BAMLProject | undefined {
  // Combine all projects into a single array
  const allProjects = [
    ...groupings.intros,
    ...groupings.advancedPromptSyntax,
    ...groupings.promptEngineering
  ];

  // Search for the project by id
  return allProjects.find(project => project.id === projectId);
}

async function getAllFiles(dirPath: string, arrayOfFiles: FileContent[] = []): Promise<FileContent[]> {
  try {
    const files = await fs.readdir(dirPath);

    for (const file of files) {
      const filePath = path.join(dirPath, file);
      try {
        const stats = await fs.stat(filePath);
        if (stats.isDirectory()) {
          arrayOfFiles = await getAllFiles(filePath, arrayOfFiles);
        } else {
          const content = await fs.readFile(filePath, 'utf8');
          arrayOfFiles.push({ path: filePath, content, error: null });
        }
      } catch (error) {
        arrayOfFiles.push({ path: filePath, content: null, error: error as Error });
      }
    }
  } catch (error) {
    console.error(`Failed to access directory ${dirPath}: ${(error as Error).message}`);
  }

  return arrayOfFiles;
}


const getProjectFiles = async (projectPath: string): Promise<EditorFile[]> => {
  const examplesPath = path.join(process.cwd(), 'public/_examples')
  const projPath = path.join(examplesPath, projectPath)
  const files = await getAllFiles(projPath);
  return files.map((f) => ({ path: f.path.replace(projPath, ""), content: f.content ?? '', error: f.error ?? null }));
}

export type BamlProjectsGroupings = {
  intros: BAMLProject[];
  advancedPromptSyntax: BAMLProject[];
  promptEngineering: BAMLProject[]
}

export async function loadExampleProjects(): Promise<BamlProjectsGroupings> {

  console.log("Loading example projects")
  // TODO parallelize
  return {
    intros: [
      {
        id: 'extract-resume',
        name: 'Introduction to BAML',
        description: 'A simple LLM function extract a resume',
        files: await getProjectFiles("/intro/extract-resume/"),
      },
      {
        id: 'classify-message',
        name: 'ClassifyMessage',
        description: 'Classify a message from a user',
        files: await getProjectFiles("/intro/classify-message/"),
      },
      {
        id: 'chat-roles',
        name: 'ChatRoles',
        description: 'Use a sequence of system and user messages',
        files: await getProjectFiles("/intro/chat-roles/"),
      }
    ],
    advancedPromptSyntax: [],
    promptEngineering: [{
      id: 'chain-of-thought',
      name: 'Chain of Thought',
      description: 'Using chain of thought to improve results and reduce hallucinations',
      files: await getProjectFiles("/prompt-engineering/chain-of-thought/"),
    },
    {
      id: 'symbol-tuning',
      name: 'Symbol Tuning',
      description: 'Use symbol tuning to remove biases on schema property names',
      files: await getProjectFiles("/prompt-engineering/symbol-tuning/"),
    }]
  };
}