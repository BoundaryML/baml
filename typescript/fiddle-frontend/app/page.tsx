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
      <div className="z-10 items-center justify-between w-screen h-screen text-sm overflow-clip lg:flex">
        <div className="w-[200px] justify-start flex flex-col px-1 pr-0 gap-y-2 items-start h-full dark:bg-vscode-sideBar-background">
          <div className="w-full pt-1 text-lg italic font-bold text-center">Prompt Fiddle</div>
          <div className="w-full text-center text-muted-foreground">Templates</div>
          <div className="flex flex-col h-full overflow-y-auto gap-y-2">
            {exampleProjects.map((p) => {
              return <ExampleProjectCard key={p.name} project={p} />
            })}
          </div>
        </div>
        <Separator className="h-full bg-vscode-panel-border" orientation="vertical" />
        <div className="w-screen h-screen dark:bg-black">
          <ProjectView project={data} />
        </div>
      </div>
    </main>
  )
}
