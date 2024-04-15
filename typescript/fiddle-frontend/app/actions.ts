'use server'
import { BAMLProject } from "@/lib/exampleProjects";
import { kv } from "@vercel/kv";
import { nanoid } from 'nanoid'
import { revalidatePath } from "next/cache";

export type EditorFile = {
  path: string;
  content: string;
}

export async function createUrl(project: BAMLProject): Promise<string> {
  const urlId = nanoid(6)
  console.log(project)

  const urlResponse = await kv.set(urlId, project, {
    nx: true,
  });
  if (!urlResponse) {
    throw new Error("Failed to create URL");
  }
  return urlId;
}

export async function updateUrl(urlId: string, editorFiles: EditorFile[]): Promise<void> {
  console.log("setting files", editorFiles);
  const user = await kv.set(urlId, editorFiles);
  revalidatePath(`/`);
}

export async function loadUrl(urlId: string): Promise<BAMLProject> {
  const user = await kv.get(urlId);
  // console.log("loading files", user);

  return user as BAMLProject;
}