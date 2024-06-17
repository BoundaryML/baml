'use server'
import type { BAMLSnippet } from './snippetProjects'
import { kv } from '@vercel/kv'

export type EditorFile = {
  path: string
  content: string
}

export async function loadUrl(urlId: string): Promise<BAMLSnippet> {
  const user = await kv.get(urlId)
  // console.log("loading files", user);

  return user as BAMLSnippet
}
