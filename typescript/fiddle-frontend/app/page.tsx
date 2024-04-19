import { BAMLProject, exampleProjects } from '@/lib/exampleProjects'
import { Separator } from '@baml/playground-common/components/ui/separator'
import dynamic from 'next/dynamic'
import { ExampleProjectCard } from './_components/ExampleProjectCard'
import { loadUrl } from './actions'
const ProjectView = dynamic(() => import('./[project_id]/_components/ProjectView'), { ssr: false })

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
  let data: BAMLProject = exampleProjects[0]
  const id = params.project_id ?? searchParams.id
  if (id) {
    const exampleProject = exampleProjects.find((p) => p.id === id)
    if (exampleProject) {
      data = exampleProject
    } else {
      data = await loadUrl(id)
    }
  } else {
    data = exampleProjects[0]
  }
  console.log(data)
  return (
    <main className="flex flex-col items-center justify-between min-h-screen font-sans">
      <div className="w-screen h-screen dark:bg-black">
        <ProjectView project={data} />
      </div>
      {/* </div> */}
    </main>
  )
}
