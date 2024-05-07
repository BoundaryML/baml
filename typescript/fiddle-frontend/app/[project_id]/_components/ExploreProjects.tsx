import { ExampleProjectCard } from '@/app/_components/ExampleProjectCard'
import { ScrollArea } from '@/components/ui/scroll-area'
import { SheetContent, SheetDescription, SheetHeader, SheetTitle } from '@/components/ui/sheet'
import type { BAMLProject } from '@/lib/exampleProjects'
import { type BamlProjectsGroupings, loadExampleProjects } from '@/lib/loadProject'
import { useEffect, useState } from 'react'

export const ExploreProjects = () => {
  const [projectGroups, setProjectGroups] = useState<BamlProjectsGroupings | null>(null)

  useEffect(() => {
    const fetchProjects = async () => {
      const projects = await loadExampleProjects()
      setProjectGroups(projects)
    }
    fetchProjects()
  }, [])

  return (
    <div className='flex flex-col w-full h-full'>
      <SheetHeader>
        <SheetTitle className='text-2xl'>Prompt Fiddle Examples</SheetTitle>
        <SheetDescription className='text-base'>
          Prompt Fiddle uses BAML -- a configuration language for LLM functions. Here are some example projects to get
          you started.
        </SheetDescription>
      </SheetHeader>

      <div className='overflow-y-auto'>
        {projectGroups ? (
          <div className='flex flex-col gap-y-4'>
            <ExampleCarousel title='Intros' projects={projectGroups.intros} />
            {/* <ExampleCarousel title="Advanced Prompt Syntax" projects={projectGroups.advancedPromptSyntax} /> */}
            <ExampleCarousel title='Prompt Engineering' projects={projectGroups.promptEngineering} />
          </div>
        ) : (
          <div>Loading...</div>
        )}
      </div>
    </div>
  )
}

const ExampleCarousel = ({ title, projects }: { title: string; projects: BAMLProject[] }) => {
  return (
    <>
      <div className='flex flex-col py-4 gap-y-3'>
        <div className='text-lg font-semibold'>{title}</div>
        <div className='flex flex-wrap gap-x-4 gap-y-4'>
          {projects.map((p) => {
            return <ExampleProjectCard key={p.id} project={p} />
          })}
        </div>
      </div>
    </>
  )
}
