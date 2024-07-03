import { ExampleProjectCard } from '@/app/_components/ExampleProjectCard'
import { ScrollArea } from '@/components/ui/scroll-area'
import { SheetContent, SheetDescription, SheetHeader, SheetTitle } from '@/components/ui/sheet'
import type { BAMLProject } from '@/lib/exampleProjects'
import { type BamlProjectsGroupings, loadExampleProjects } from '@/lib/loadProject'
import { useEffect, useState } from 'react'

export const ExploreProjects = () => {
  return null
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
