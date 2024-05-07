'use client'

import { Button } from '@/components/ui/button'
import { Sheet, SheetContent, SheetTrigger } from '@/components/ui/sheet'
import { useAtom } from 'jotai'
import { Compass } from 'lucide-react'
import { isMobile } from 'react-device-detect'
import { exploreProjectsOpenAtom } from '../[project_id]/_atoms/atoms'
import { ExploreProjects } from '../[project_id]/_components/ExploreProjects'
export const BrowseSheet = () => {
  const [open, setOpen] = useAtom(exploreProjectsOpenAtom)

  if (isMobile) return null
  return (
    <Sheet open={open} onOpenChange={() => setOpen(!open)}>
      <SheetTrigger asChild>
        {/* Fake button, not used. the real button is in the actual project page.
        We do this shit because if we add the Sheet to the page, and not the layout, the sheet will reset everytime the url changes to a new project, but we want it to be not re-render. Layouts aid in that. */}
        <Button className='flex-row items-center hidden px-2 text-sm whitespace-no-wrap bg-indigo-600 hover:bg-indigo-500 h-fit gap-x-2 text-vscode-button-foreground'>
          <Compass size={24} strokeWidth={2} />
          <span>Browse Examples</span>
        </Button>
      </SheetTrigger>
      <SheetContent className='bg-zinc-900 min-w-[600px]' onInteractOutside={() => setOpen(false)}>
        <ExploreProjects />
      </SheetContent>
    </Sheet>
  )
}
