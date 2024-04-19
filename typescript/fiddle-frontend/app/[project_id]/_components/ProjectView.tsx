'use client'

import { Button } from '@/components/ui/button'
import { ResizableHandle, ResizablePanel, ResizablePanelGroup } from '@/components/ui/resizable'
import { useKeybindingOverrides } from '@/hooks/command-s'
import { BAML_DIR } from '@/lib/constants'
import { BAMLProject, exampleProjects } from '@/lib/exampleProjects'
import {
  ASTProvider,
  CustomErrorBoundary,
  FunctionPanel,
  FunctionSelector,
  useSelections,
} from '@baml/playground-common'
import { useAtom, useAtomValue, useSetAtom } from 'jotai'
import { useHydrateAtoms } from 'jotai/utils'
import Link from 'next/link'
import { useParams, usePathname } from 'next/navigation'
import { useContext, useEffect, useRef, useState } from 'react'
import { toast } from 'sonner'
import { Editable } from '../../_components/EditableText'
import { EditorFile, createUrl } from '../../actions'
import { currentEditorFilesAtom, currentParserDbAtom, testRunOutputAtom, unsavedChangesAtom } from '../_atoms/atoms'
import { CodeMirrorEditor } from './CodeMirrorEditor'
import { usePlaygroundListener } from '../_playground_controller/usePlaygroundListener'
import { ASTContext } from '@baml/playground-common/shared/ASTProvider'
import { Badge } from '@/components/ui/badge'
import { useRouter } from 'next/navigation'
import { atomStore, sessionStore } from '@/app/_components/JotaiProvider'
import FileViewer from './Tree/FileViewer'
import { ExampleProjectCard } from '@/app/_components/ExampleProjectCard'
import { Separator } from '@baml/playground-common/components/ui/separator'
import { ScrollArea } from '@/components/ui/scroll-area'

const ProjectViewImpl = ({ project }: { project: BAMLProject }) => {
  const setEditorFiles = useSetAtom(currentEditorFilesAtom)
  const setTestRunOutput = useSetAtom(testRunOutputAtom)
  useKeybindingOverrides()
  // Tried to use url pathnames for this but nextjs hijacks the pathname state (even the window.location) so we have to manually track unsaved changes in the app.
  const [unsavedChanges, setUnsavedChanges] = useAtom(unsavedChangesAtom)

  useEffect(() => {
    if (project && project?.files?.length > 0) {
      setEditorFiles([...project.files])
    }
  }, [project.id])
  const [projectName, setProjectName] = useState(project.name)
  const inputRef = useRef(null)

  useEffect(() => {
    if (project) {
      if (project.testRunOutput) {
        setTestRunOutput(project.testRunOutput)
      }
      if (project.files) {
        setEditorFiles(project.files)
      }
    }
  }, [project.id])

  return (
    // firefox wont apply the background color for some reason so we forcefully set it.
    <div className="flex flex-row w-full h-full bg-gray-800">
      <ResizablePanelGroup className="w-full h-full overflow-clip" direction="horizontal">
        <ResizablePanel defaultSize={12} className="h-full bg-zinc-900">
          <div className="w-full pt-1 text-lg italic font-bold text-center">Prompt Fiddle</div>

          <div className="flex flex-col w-full pt-2 h-[30%] overflow-y-auto">
            <div className="w-full px-2 text-sm font-semibold text-center uppercase text-white/90">project files</div>
            <FileViewer />
          </div>
          {/* <Separator className="bg-vscode-textSeparator-foreground" /> */}
          <div className="w-full px-2 pt-2 text-sm font-semibold text-center uppercase text-white/90">Templates</div>
          <div className="flex flex-col h-[70%] pb-16">
            <ScrollArea type="always">
              <div className="flex flex-col px-2 gap-y-4">
                {exampleProjects.map((p) => {
                  return <ExampleProjectCard key={p.name} project={p} />
                })}
              </div>
            </ScrollArea>
          </div>
        </ResizablePanel>
        <ResizableHandle className="bg-vscode-contrastActiveBorder border-vscode-contrastActiveBorder" />
        <ResizablePanel defaultSize={88}>
          <div className="flex-col w-full h-full font-sans pl-2flex bg-background dark:bg-vscode-panel-background">
            <div className="flex flex-row gap-x-12 border-b-[1px] border-vscode-panel-border h-[40px]">
              <div className="flex flex-col items-center h-full py-1">
                <Editable text={projectName} placeholder="Write a task name" type="input" childRef={inputRef}>
                  <input
                    className="px-2 text-lg border-none text-foreground"
                    type="text"
                    ref={inputRef}
                    name="task"
                    placeholder="Write a task name"
                    value={projectName}
                    onChange={(e) => setProjectName(e.target.value)}
                  />
                </Editable>
              </div>
              <div className="flex flex-row items-center gap-x-2">
                <ShareButton project={project} projectName={projectName} />
              </div>

              <div className="flex flex-row justify-center gap-x-1 item-center">
                {/* <TestToggle /> */}
                <Button variant={'ghost'} className="h-full py-1" asChild>
                  <Link href="https://docs.boundaryml.com">Docs</Link>
                </Button>
              </div>
              {unsavedChanges ? (
                <div className="flex flex-row items-center text-muted-foreground">
                  <Badge variant="outline" className="font-light text-red-400">
                    Unsaved changes
                  </Badge>
                </div>
              ) : (
                <></>
              )}
            </div>

            <div className="flex flex-row w-full h-full">
              <ResizablePanelGroup
                className="min-h-[200px] w-full rounded-lg border overflow-clip"
                direction="horizontal"
              >
                <ResizablePanel defaultSize={50}>
                  <div className="flex w-full h-full">
                    <CodeMirrorEditor project={project} />
                  </div>
                </ResizablePanel>
                <ResizableHandle className="bg-vscode-contrastActiveBorder" />
                <ResizablePanel defaultSize={50}>
                  <div className="flex flex-row h-full bg-vscode-panel-background">
                    <PlaygroundView />
                  </div>
                </ResizablePanel>
              </ResizablePanelGroup>
            </div>
          </div>
        </ResizablePanel>
      </ResizablePanelGroup>
    </div>
  )
}

export const ProjectView = ({ project }: { project: BAMLProject }) => {
  return (
    <>
      {/* <DummyHydrate files={project.files} /> */}
      <ProjectViewImpl project={project} />
    </>
  )
}

const ShareButton = ({ project, projectName }: { project: BAMLProject; projectName: string }) => {
  const [loading, setLoading] = useState(false)
  const editorFiles = useAtomValue(currentEditorFilesAtom)
  const runTestOutput = useAtomValue(testRunOutputAtom)
  const pathname = usePathname()
  const setUnsavedChanges = useSetAtom(unsavedChangesAtom)

  return (
    <Button
      variant={'ghost'}
      className="h-full py-1"
      disabled={loading}
      onClick={async () => {
        setLoading(true)
        try {
          let urlId = pathname.split('/')[1]
          console.log('urlId', urlId)
          if (!urlId) {
            urlId = await createUrl({
              ...project,
              name: projectName,
              files: editorFiles,
              testRunOutput: runTestOutput ?? undefined,
            })

            const newUrl = `${window.location.origin}/${urlId}`
            window.history.replaceState({ ...window.history.state, as: newUrl, url: newUrl }, '', newUrl)
            setUnsavedChanges(false)
            // router.replace(pathname + '?' + updatedSearchParams.toString(), { scroll: false })
          }

          navigator.clipboard.writeText(`${window.location.origin}/${urlId}`)
          toast('URL copied to clipboard')
        } catch (e) {
          toast('Failed to generate URL')
          console.error(e)
        } finally {
          setLoading(false)
        }
      }}
    >
      Share
    </Button>
  )
}

const DummyHydrate = ({ files }: { files: EditorFile[] }) => {
  useHydrateAtoms([[currentEditorFilesAtom as any, files]]) // any cause sessionStorage screws types up somehow
  return <></>
}

const PlaygroundView = () => {
  const [parserDb] = useAtom(currentParserDbAtom)
  usePlaygroundListener()
  const testRunOutput = useAtomValue(testRunOutputAtom)

  useEffect(() => {
    if (!parserDb) {
      return
    }
    const newParserDb = { ...parserDb }

    window.postMessage({
      command: 'setDb',
      content: [[BAML_DIR, newParserDb]],
    })
  }, [parserDb])

  useEffect(() => {
    if (testRunOutput) {
      window.postMessage({
        command: 'test-results',
        content: testRunOutput.testState,
      })
      window.postMessage({
        command: 'test-stdout',
        content: testRunOutput.outputLogs.join('\n'),
      })
    }
  }, [testRunOutput])

  return (
    <>
      <CustomErrorBoundary>
        <ASTProvider>
          {/* <TestToggle /> */}
          <div className="flex flex-col gap-2 px-2 pb-4">
            <FunctionSelector />
            {/* <Separator className="bg-vscode-textSeparator-foreground" /> */}
            <FunctionPanel />
          </div>
        </ASTProvider>
      </CustomErrorBoundary>
    </>
  )
}

const TestToggle = () => {
  const { setSelection } = useContext(ASTContext)
  const { showTests } = useSelections()

  return (
    <Button
      variant="outline"
      className="p-1 text-xs w-fit h-fit border-vscode-textSeparator-foreground"
      onClick={() => setSelection(undefined, undefined, undefined, undefined, !showTests)}
    >
      {showTests ? 'Hide tests' : 'Show tests'}
    </Button>
  )
}

export default ProjectView
