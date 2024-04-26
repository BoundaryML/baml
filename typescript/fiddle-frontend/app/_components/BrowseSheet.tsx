'use client'

import { SheetTrigger, SheetContent, Sheet } from '@/components/ui/sheet'
import { Button } from '@/components/ui/button'
import { Compass } from 'lucide-react'
import { ExploreProjects } from '../[project_id]/_components/ExploreProjects'

export const BrowseSheet = () => {
  return (
    <Sheet>
      <SheetTrigger asChild>
        <Button className="flex flex-row items-center px-2 text-sm whitespace-pre-wrap bg-indigo-600 hover:bg-indigo-500 h-fit gap-x-2 text-vscode-button-foreground">
          <Compass size={24} strokeWidth={2} />
          <span>Browse Examples</span>
        </Button>
      </SheetTrigger>
      <SheetContent className="bg-zinc-900 min-w-[600px]">
        <ExploreProjects />
      </SheetContent>
    </Sheet>
  )
}
