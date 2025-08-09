import dynamic from 'next/dynamic'
import { loadProject } from '../../lib/loadProject'

const ClientWrapper = dynamic(() => import('./clientwrapper'), {})

export default async function EmbedComponent({
  searchParams,
}: {
  searchParams: Promise<{ id?: string }>
}) {
  const params = await searchParams
  const id = typeof params.id === 'string' ? params.id : undefined

  if (!id) {
    return <div className='flex items-center justify-center w-screen h-screen'>No project id provided</div>
  }

  const project = await loadProject(Promise.resolve({ project_id: id }))

  if (!project) {
    return <div className='flex items-center justify-center w-screen h-screen'>No project found</div>
  }

  return (
    <div className='flex justify-center items-center h-screen rounded-lg border-2 border-purple-900/30 overflow-y-clip'>
      <div className='flex w-full h-full'>
        <ClientWrapper files={project.files} />
      </div>
    </div>
  )
}