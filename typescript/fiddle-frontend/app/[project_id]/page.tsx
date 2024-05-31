import type { BAMLProject } from '@/lib/exampleProjects'
import { loadProject } from '@/lib/loadProject'
import type { Metadata, ResolvingMetadata } from 'next'
import dynamic from 'next/dynamic'
const ProjectView = dynamic(() => import('./_components/ProjectView'), { ssr: false })

type Props = {
  params: { project_id: string }
  searchParams: { [key: string]: string | string[] | undefined }
}
export async function generateMetadata({ params, searchParams }: Props, parent: ResolvingMetadata): Promise<Metadata> {
  // read route params
  const project = await loadProject({ project_id: params.project_id })
  return {
    title: `${project.name} — Prompt Fiddle`,
    description: project.description,
  }
}

type SearchParams = {
  id: string
}

export default async function Home({
  searchParams,
  params,
}: {
  searchParams: SearchParams
  params: { project_id: string }
}) {
  const data: BAMLProject = await loadProject(params)
  // console.log(data)
  return (
    <main className='flex flex-col items-center justify-between min-h-screen font-sans'>
      <div className='w-screen h-screen dark:bg-black'>
        <ProjectView project={data} />
      </div>
      {/* </div> */}
    </main>
  )
}
