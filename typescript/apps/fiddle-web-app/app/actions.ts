'use server';
import { kv } from '@vercel/kv';
import { nanoid } from 'nanoid';
import { revalidatePath } from 'next/cache';
import type { BAMLProject } from '../lib/exampleProjects';

export type EditorFile = {
  path: string;
  content: string;
};

export async function test() {
  await new Promise((resolve) => setTimeout(resolve, 1000));
  console.log('test');
  return 'test';
}

export async function createUrl(project: BAMLProject): Promise<string> {
  // Replace spaces, slashes, and other non-alphanumeric characters with dashes
  const projectName = project.name;
  const safeProjectName = projectName.replace(/[^a-zA-Z0-9]/g, '-');
  const urlId = `${safeProjectName}-${nanoid(5)}`;
  console.log('Created urlId', urlId);
  try {
    const urlResponse = await kv.set(urlId, project, {
      nx: true,
    });
    if (!urlResponse) {
      throw new Error('Failed to create URL');
    }
    return urlId;
  } catch (e) {
    console.log('Error creating url', e);
    throw new Error('Failed to create URL');
  }
}

export async function updateUrl(
  urlId: string,
  editorFiles: EditorFile[],
): Promise<void> {
  const user = await kv.set(urlId, editorFiles);
  revalidatePath(`/`);
}

export async function loadUrl(urlId: string): Promise<BAMLProject | undefined> {
  const user = await kv.get(urlId);

  if (!user) {
    return undefined;
  }
  // console.log("loading files", user);

  return user as BAMLProject;
}
