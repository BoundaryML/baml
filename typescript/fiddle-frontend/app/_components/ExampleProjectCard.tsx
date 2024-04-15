'use client'
import { Card, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { BAMLProject } from '@/lib/exampleProjects'
import clsx from 'clsx'
import { usePathname, useRouter, useSearchParams, useSelectedLayoutSegment } from 'next/navigation'

export const ExampleProjectCard = ({ project }: { project: BAMLProject }) => {
  // const searchParams = useSearchParams()
  const router = useRouter()
  const selectedId = usePathname().replace('/', '')
  const isSelected = selectedId === project.id || (project.id === 'extract-verbs' && selectedId === '')
  return (
    <Card
      className={clsx(
        'flex w-full h-fit px-2 font-sans border-gray-800 bg-zinc-900 hover:cursor-pointer hover:bg-zinc-800 rounded-sm',
        [isSelected ? 'border-gray-600 bg-zinc-800' : 'border-transparent'],
      )}
      onClick={() => {
        router.push(`/${project.id}`)
        // router.refresh()
      }}
    >
      <CardHeader className="px-1 py-4">
        <CardTitle className="text-xs">{project.name}</CardTitle>
        <CardDescription className="text-xs">{project.description}</CardDescription>
      </CardHeader>
    </Card>
  )
}
