'use client'
import { ResizableHandle, ResizablePanelGroup } from '@/components/ui/resizable'
import { ResizablePanel } from '@/components/ui/resizable'
import { ScrollArea } from '@/components/ui/scroll-area'
import { filesAtom } from '@/shared/baml-project-panel/atoms'
import { CustomErrorBoundary } from '@baml/playground-common'
import { useAtom, useAtomValue } from 'jotai'
import dynamic from 'next/dynamic'
import { isMobile } from 'react-device-detect'

import { Suspense, useEffect, useState } from 'react'
import { activeFileNameAtom } from '../[project_id]/_atoms/atoms'
const CodeMirrorViewer = dynamic(() => import('@baml/playground-common').then((mod) => mod.CodeMirrorViewer), {
  ssr: false,
})
const PromptPreview = dynamic(() => import('@baml/playground-common').then((mod) => mod.PromptPreview), {
  ssr: false,
})
const EventListener = dynamic(() => import('@baml/playground-common').then((mod) => mod.EventListener), {
  ssr: false,
})

interface EmbedComponentProps {
  bamlContent: string
}

export default function EmbedComponent({ bamlContent }: EmbedComponentProps) {
  const [files, setFiles] = useAtom(filesAtom)
  const [isLoading, setIsLoading] = useState(true)
  const activeFileName = useAtomValue(activeFileNameAtom)

  useEffect(() => {
    // Set the files with the BAML content passed from the server
    setFiles({
      'main.baml': bamlContent,
    })
    setIsLoading(false)
  }, [bamlContent, setFiles])

  if (isLoading) {
    return <div className='text-white'>Loading BAML file...</div>
  }

  return (
    <div className='flex justify-center items-center w-screen h-screen bg-background'>
      <EventListener>
        {/* noop for now -- we dont need to nest all the other components in the EventListener since we use jotaiprovider store and we dont want to rerender needlessly */}
        <div></div>
      </EventListener>
      {/* <h1 className='text-xl font-bold text-gray-500'>This is an embeddable React Component!</h1> */}
      {/* <p className='text-gray-600'>You can use this inside an iframe.</p> */}
      <ResizablePanelGroup className='min-h-[200px] w-full rounded-lg overflow-clip' direction='horizontal'>
        <ResizablePanel defaultSize={50}>
          <div className='flex pl-1 w-full h-full tour-editor dark:bg-muted/70'>
            <ScrollArea className='w-full h-full'>
              {activeFileName && (
                <CodeMirrorViewer
                  lang='baml'
                  fileContent={{
                    code: files[activeFileName],
                    language: 'baml',
                    id: activeFileName,
                  }}
                  hideLineNumbers={true}
                  shouldScrollDown={false}
                  onContentChange={(v) => {
                    const newFiles: Record<string, string> = {}
                    Object.entries(files).map(([key, value]) => {
                      const newVal = key === activeFileName ? v : value
                      newFiles[key] = newVal
                    })
                    setFiles(newFiles)
                  }}
                />
              )}
            </ScrollArea>
          </div>
        </ResizablePanel>
        <ResizableHandle className='' />
        {!isMobile && (
          <ResizablePanel defaultSize={50} className='tour-playground'>
            <div className='flex flex-row h-full'>
              <PlaygroundView />
            </div>
          </ResizablePanel>
        )}
      </ResizablePanelGroup>
    </div>
  )
}

const PlaygroundView = () => {
  return (
    <>
      <CustomErrorBoundary message='Error loading playground'>
        <Suspense fallback={<div>Loading...</div>}>
          <div className='flex flex-col w-full h-full'>
            <PromptPreview isEmbed={true} />
          </div>

          {/* <InitialTour /> */}
          {/* <PostTestRunTour /> */}
        </Suspense>
      </CustomErrorBoundary>
    </>
  )
}
