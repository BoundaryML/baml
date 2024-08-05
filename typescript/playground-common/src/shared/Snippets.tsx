import { atom } from 'jotai'
import SnippetCard from './SnippetCard'
import { DialogDescription, DialogHeader, DialogTitle } from '../components/ui/dialog'
import { Button } from '../components/ui/button'
import type { BAMLProject } from '../lib/utils'
import { BamlProjectsGroupings, loadExampleProjects } from '../lib/utils'
import { useEffect, useState } from 'react'
import FileViewer from './Tree/FileViewer'
import TextComponentList from './SnippetList'
import { activeFileAtom } from './Tree/atoms'
import { useAtom } from 'jotai'

export const showSnippetsAtom = atom(false)

export const Snippets = () => {
  const [snippetGroups, setSnippetGroups] = useState<BamlProjectsGroupings | null>(null)

  useEffect(() => {
    const fetchProjects = async () => {
      const projects = await loadExampleProjects()
      setSnippetGroups(projects)
    }
    fetchProjects()
  }, [])

  return (
    <div className='flex flex-col w-full h-full overflow-auto'>
      <DialogHeader>
        <DialogTitle className='text-2xl '>BAML Snippets</DialogTitle>
        <DialogDescription className='text-xs text-white-500'>
          BAML is a configuration language for making your LLMs useful. Here are some snippets to show you core concepts
          of the language.
        </DialogDescription>
      </DialogHeader>
      <div className='flex justify-center'>
        <a href='https://docs.boundaryml.com' target='_blank' rel='noopener noreferrer'>
          <Button
            variant={'ghost'}
            className='flex flex-row items-center px-2 py-1 mt-3 mr-2 text-base whitespace-pre-wrap bg-indigo-600 hover:bg-indigo-500 h-fit gap-x-2 text-vscode-button-foreground'
          >
            <span className='whitespace-nowrap'>View our full docs!</span>
          </Button>
        </a>
      </div>

      {/* Use flex-row to layout the FileViewer alongside the snippet carousels */}
      <div className='flex flex-row items-start w-full h-full'>
        {/* Adjust the flex property to control space allocation */}
        <div className='flex-none w-48 overflow-hidden pt-7'>
          <div className='px-4 text-sm text-white-500'>Table of Contents</div>
          <FileViewer />
        </div>

        <div className='w-6' />

        <div className='flex flex-grow w-full max-w-2xl overflow-x-auto pt-7'>
          <TextComponentList selectedId={useAtom(activeFileAtom)[0] || 'system_user_prompts'} />
          <div className='flex-grow overflow-y-auto'>
            {snippetGroups ? <div className='flex flex-col overflow-auto gap-y-4'></div> : <div>Loading...</div>}
          </div>
        </div>
      </div>
    </div>
  )
}
