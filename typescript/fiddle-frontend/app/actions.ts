'use server'
import type { BAMLProject } from '@/lib/exampleProjects'
import { kv } from '@vercel/kv'
import { nanoid } from 'nanoid'
import { revalidatePath } from 'next/cache'

export type EditorFile = {
  path: string
  content: string
}

export async function createUrl(project: BAMLProject): Promise<string> {
  // Replace spaces, slashes, and other non-alphanumeric characters with dashes
  const projectName = project.name
  const safeProjectName = projectName.replace(/[^a-zA-Z0-9]/g, '-')
  const urlId = `${safeProjectName}-${nanoid(5)}`
  console.log(project)
  const urlResponse = await kv.set(urlId, project, {
    nx: true,
  })
  if (!urlResponse) {
    throw new Error('Failed to create URL')
  }
  return urlId
}

export async function updateUrl(urlId: string, editorFiles: EditorFile[]): Promise<void> {
  console.log('setting files', editorFiles)
  const user = await kv.set(urlId, editorFiles)
  revalidatePath(`/`)
}

export async function loadUrl(urlId: string): Promise<BAMLProject> {
  const user = await kv.get(urlId)
  // console.log("loading files", user);

  return user as BAMLProject
}
