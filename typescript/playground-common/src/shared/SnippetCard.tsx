import { Card, CardDescription, CardHeader, CardTitle } from '../components/ui/card'
import type { BAMLProject } from '../lib/utils'
import { Popover, PopoverTrigger, PopoverContent, PopoverAnchor } from '../components/ui/popover'
import { useState } from 'react'

import clsx from 'clsx'

export const SnippetCard = ({ snippet }: { snippet: BAMLProject }) => {
  const [isModalOpen, setModalOpen] = useState(false)

  const toggleModal = () => {
    setModalOpen(!isModalOpen)
  }

  const copyToClipboard = (text: string) => {
    navigator.clipboard.writeText(text)
  }

  return (
    <>
      <Card
        className={clsx(
          'flex w-[200px] h-[140px] text-center px-2 font-sans border-zinc-700 border-[1px] bg-zinc-800/40 hover:cursor-pointer hover:bg-zinc-800 rounded-sm',
        )}
        onClick={toggleModal}
      >
        <CardHeader className='w-full px-1 py-4'>
          <CardTitle className='text-base text-left'>{snippet.name}</CardTitle>
          <CardDescription className='text-sm text-left'>{snippet.description}</CardDescription>
        </CardHeader>
      </Card>
      {isModalOpen && (
        <PopoverContent>
          <div>
            <h2>{snippet.name}</h2>
            <p>{snippet.description}</p>
            {/* add in the code to the popover */}
            <button onClick={() => copyToClipboard(snippet.description)}>Copy Description</button>
          </div>
        </PopoverContent>
      )}
    </>
  )
}
