import { BAMLProject } from '@/lib/exampleProjects'
import { loadProject } from '@/lib/loadProject'
import dynamic from 'next/dynamic'
import { generateMetadata } from './[project_id]/page'
const ProjectView = dynamic(() => import('./[project_id]/_components/ProjectView'), { ssr: false })

type SearchParams = {
  id: string
}

// We don't need this since it's already part of layout.tsx
// export const metadata: Metadata = {
//   title: 'Prompt Fiddle',
//   description: '...',
// }

export default async function Home({
  searchParams,
  params,
}: {
  searchParams: SearchParams
  params: { project_id: string }
}) {
  let data: BAMLProject = await loadProject(params, true)
  console.log(data)
  return (
    <main className="flex flex-col items-center justify-between min-h-screen font-sans">
      <div className="w-screen h-screen dark:bg-black">
        <ProjectView project={data} />
      </div>
    </main>
  )
}
