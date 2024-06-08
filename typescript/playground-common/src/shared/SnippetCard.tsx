import { Card, CardDescription, CardHeader, CardTitle } from '../components/ui/card'
import type { BAMLProject } from '../lib/utils'
import { Popover, PopoverTrigger, PopoverContent, PopoverAnchor } from '../components/ui/popover'
import { Button } from '../components/ui/button'

import clsx from 'clsx'

export const SnippetCard = ({ snippet }: { snippet: BAMLProject }) => {
  const copyToClipboard = (text: string) => {
    navigator.clipboard.writeText(text)
  }

  return (
    <>
      <Popover>
        <PopoverTrigger>
          <Card
            className={clsx(
              'flex w-[200px] h-[140px] text-center px-2 font-sans border-zinc-700 border-[1px] bg-zinc-800/40 hover:cursor-pointer hover:bg-zinc-800 rounded-sm',
            )}
          >
            <CardHeader className='w-full px-1 py-4'>
              <CardTitle className='text-base text-left'>{snippet.name}</CardTitle>
              <CardDescription className='text-sm text-left'>{snippet.description}</CardDescription>
            </CardHeader>
          </Card>
        </PopoverTrigger>
        <PopoverContent>
          <div>
            <h2>{snippet.name}</h2>
            <p>{snippet.description}</p>
            {/* add in the BAML code to the popover <p> {snippet.file.content} </p> */}
            <Button onClick={() => copyToClipboard(snippet.description)}>Copy</Button>
          </div>
        </PopoverContent>
      </Popover>
    </>
  )
}
