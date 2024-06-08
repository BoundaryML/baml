import { atom } from 'jotai'
import { SnippetCard } from './SnippetCard'
import { SheetDescription, SheetHeader, SheetTitle } from '../components/ui/sheet'
import type { BAMLProject } from '../lib/utils'
import { BamlProjectsGroupings, loadExampleProjects } from '../lib/utils'
import { useEffect, useState } from 'react'

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
    <div className='flex flex-col w-full h-full'>
      <SheetHeader>
        <SheetTitle className='text-2xl'>Prompt Fiddle Examples</SheetTitle>
        <SheetDescription className='text-base'>
          Prompt Fiddle uses BAML -- a configuration language for LLM functions. Here are some example projects to get
          you started.
        </SheetDescription>
      </SheetHeader>

      <div className='overflow-y-auto'>
        {snippetGroups ? (
          <div className='flex flex-col gap-y-4'>
            <SnippetCarousel title='Intros' projects={snippetGroups.intros} />
            {/* <ExampleCarousel title="Advanced Prompt Syntax" projects={projectGroups.advancedPromptSyntax} /> */}
            <SnippetCarousel title='Prompt Engineering' projects={snippetGroups.promptEngineering} />
          </div>
        ) : (
          <div>Loading...</div>
        )}
      </div>
    </div>
  )
}

export const SnippetCarousel = ({ title, projects }: { title: string; projects: BAMLProject[] }) => {
  return (
    <>
      <div className='flex flex-col py-4 gap-y-3'>
        <div className='text-lg font-semibold'>{title}</div>
        <div className='flex flex-wrap gap-x-4 gap-y-4'>
          {projects.map((p) => {
            return <SnippetCard key={p.id} snippet={p} />
          })}
        </div>
      </div>
    </>
  )
}
