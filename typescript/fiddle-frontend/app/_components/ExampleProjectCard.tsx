'use client'
import { Card, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { BAMLProject } from '@/lib/exampleProjects'
import clsx from 'clsx'
import { useAtomValue } from 'jotai'
import { usePathname, useRouter, useSearchParams, useSelectedLayoutSegment } from 'next/navigation'
import { unsavedChangesAtom } from '../[project_id]/_atoms/atoms'
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
  AlertDialogTrigger,
} from '@/components/ui/alert-dialog'

export const ExampleProjectCard = ({ project }: { project: BAMLProject }) => {
  // const searchParams = useSearchParams()
  const router = useRouter()
  const selectedId = usePathname().replace('/', '')
  const isSelected = selectedId === project.id || (project.id === 'extract-verbs' && selectedId === '')
  const unsavedChanges = useAtomValue(unsavedChangesAtom)
  return (
    <AlertDialog>
      <AlertDialogTrigger>
        <Card
          className={clsx(
            'flex w-full h-fit text-center px-2 font-sans border-gray-800 bg-zinc-800/40 hover:cursor-pointer hover:bg-zinc-800 rounded-sm',
            [isSelected ? 'border-gray-600 bg-zinc-800' : 'border-transparent'],
          )}
          onClick={() => {
            if (!unsavedChanges) {
              router.push(`/${project.id}`, { scroll: false })
              router.refresh()
            }
          }}
        >
          <CardHeader className="w-full px-1 py-4">
            <CardTitle className="text-xs">{project.name}</CardTitle>
            <CardDescription className="text-xs">{project.description}</CardDescription>
          </CardHeader>
        </Card>
      </AlertDialogTrigger>
      {unsavedChanges && (
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Discard unsaved changes?</AlertDialogTitle>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction
              onClick={() => {
                router.push(`/${project.id}`, { scroll: false })
              }}
            >
              Yes, continue
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      )}
    </AlertDialog>
  )
}
