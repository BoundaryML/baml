import dynamic from 'next/dynamic'
import { loadProject } from '../../lib/loadProject'
import path from 'path'
import { promises as fs } from 'fs'
import { headers } from 'next/headers'

const ClientWrapper = dynamic(() => import('./clientwrapper'), {})

async function loadDocsMainBaml(exampleName: string): Promise<string | undefined> {
  const sanitizedExampleName = exampleName.replace(/[^a-zA-Z0-9-_]/g, '')
  const fsPath = path.join(
    process.cwd(),
    'public',
    '_docs',
    sanitizedExampleName,
    'baml_src',
    'main.baml',
  )
  try {
    await fs.access(fsPath)
    return await fs.readFile(fsPath, 'utf-8')
  } catch {
    // Likely not available at runtime on serverless; fetch via HTTP
  }

  try {
    const h = await headers()
    const host = h.get('x-vercel-deployment-url') || h.get('x-forwarded-host') || h.get('host')
    const proto = h.get('x-forwarded-proto') || 'https'
    if (!host) return undefined
    const base = `${proto}://${host}`
    const url = `${base}/_docs/${sanitizedExampleName}/baml_src/main.baml`
    const resp = await fetch(url, { cache: 'force-cache' })
    if (!resp.ok) return undefined
    return await resp.text()
  } catch {
    return undefined
  }
}

export default async function EmbedComponent({
  searchParams,
}: {
  searchParams: Promise<{
    id?: string
    showFileTree?: string
    fileTree?: string
    defaultFile?: string
    showFile?: string
    showPlayground?: string
    playground?: string
  }>
}) {
  const params = await searchParams
  const id = typeof params.id === 'string' ? params.id : undefined


  if (!id) {
    return <div className='flex items-center justify-center w-screen h-screen'>No project id provided</div>
  }

  const project = await loadProject(Promise.resolve({ project_id: id }))
  if (project) {
    return (
      <div className='flex justify-center items-center h-screen rounded-lg border-2 border-purple-900/30 overflow-y-clip'>
        <div className='flex w-full h-full'>
          <ClientWrapper files={project.files} />
        </div>
      </div>
    )
  }

  // Fallback: load from public docs like the previous implementation
  const mainContent = (await loadDocsMainBaml(id)) ?? (await loadDocsMainBaml('default-example'))
  if (!mainContent) {
    return <div className='flex items-center justify-center w-screen h-screen'>No project found</div>
  }
  const files = [
    { path: '/baml_src/main.baml', content: mainContent },
  ]

  return (
    <div className='flex justify-center items-center h-screen rounded-lg border-2 border-purple-900/30 overflow-y-clip'>
      <div className='flex w-full h-full'>
        <ClientWrapper files={files} />
      </div>
    </div>
  )
}