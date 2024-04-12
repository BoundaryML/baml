'use client'
import { Card, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { BAMLProject } from '@/lib/exampleProjects'
import { useRouter } from 'next/navigation'

export const ExampleProjectCard = ({ project }: { project: BAMLProject }) => {
  const router = useRouter()
  return (
    <Card
      className="flex w-full px-2 overflow-y-auto font-sans hover:cursor-pointer hover:bg-secondary"
      onClick={() => {
        router.push('/?id=' + project.id)
        router.refresh()
      }}
    >
      <CardHeader className="px-1">
        <CardTitle className="text-base">{project.name}</CardTitle>
        <CardDescription>{project.description}</CardDescription>
      </CardHeader>
    </Card>
  )
}
