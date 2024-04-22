'use client'

import { ExampleProjectCard } from '@/app/_components/ExampleProjectCard'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { ResizableHandle, ResizablePanel, ResizablePanelGroup } from '@/components/ui/resizable'
import { ScrollArea } from '@/components/ui/scroll-area'
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
import { ASTContext } from '@baml/playground-common/shared/ASTProvider'
import { useAtom, useAtomValue, useSetAtom } from 'jotai'
import { useHydrateAtoms } from 'jotai/utils'
import Image from 'next/image'
import Link from 'next/link'
import { usePathname } from 'next/navigation'
import { useContext, useEffect, useRef, useState } from 'react'
import { toast } from 'sonner'
import { Editable } from '../../_components/EditableText'
import { EditorFile, createUrl } from '../../actions'
import { currentEditorFilesAtom, currentParserDbAtom, testRunOutputAtom, unsavedChangesAtom } from '../_atoms/atoms'
import { usePlaygroundListener } from '../_playground_controller/usePlaygroundListener'
import { CodeMirrorEditor } from './CodeMirrorEditor'
import { GithubStars } from './GithubStars'
import FileViewer from './Tree/FileViewer'
import clsx from 'clsx'
import { FlaskConical } from 'lucide-react'

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
    setUnsavedChanges(false)
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
          <div className="w-full pt-2 text-lg italic font-bold text-center">Prompt Fiddle</div>

          <div className="flex flex-col w-full pt-2 h-[50%] ">
            <div className="w-full px-2 text-sm font-semibold text-center uppercase text-white/90">project files</div>
            {/* <ScrollArea type="hover" className="flex flex-col w-full"> */}
            <div className="flex flex-col w-full">
              <FileViewer />
            </div>
            {/* </ScrollArea> */}
          </div>
          {/* <Separator className="bg-vscode-textSeparator-foreground" /> */}
          <div className="w-full px-2 pt-2 text-sm font-semibold text-center uppercase text-white/90">Templates</div>
          <div className="flex flex-col h-[50%] pb-16">
            <ScrollArea type="hover">
              <div className="flex flex-col px-4 gap-y-4">
                {exampleProjects.map((p) => {
                  return <ExampleProjectCard key={p.name} project={p} />
                })}
              </div>
            </ScrollArea>
          </div>
        </ResizablePanel>
        <ResizableHandle className="bg-vscode-contrastActiveBorder border-vscode-contrastActiveBorder" />
        <ResizablePanel defaultSize={88}>
          <div className="flex-col w-full h-full font-sans bg-background dark:bg-vscode-panel-background">
            <div className="flex flex-row items-center gap-x-12 border-b-[1px] border-vscode-panel-border min-h-[40px]">
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

              {/* <div className="flex flex-row justify-center gap-x-1 item-center">
                <Button variant={'ghost'} className="h-full py-1" asChild>
                  <Link target="_blank" href="https://docs.boundaryml.com">
                    Docs
                  </Link>
                </Button>
              </div> */}

              {unsavedChanges ? (
                <div className="flex flex-row items-center text-muted-foreground">
                  <Badge variant="outline" className="font-light text-red-400">
                    Unsaved changes
                  </Badge>
                </div>
              ) : (
                <></>
              )}
              <div className="flex flex-row items-center justify-end w-full pr-4 gap-x-8">
                <div className="flex h-full">
                  <Link
                    href="https://docs.boundaryml.com/v3/home/installation"
                    className="h-full pt-1 w-fit text-zinc-300 hover:text-zinc-50"
                  >
                    <div className="flex flex-row items-center text-sm gap-x-4">
                      <Image src="/vscode_logo.svg" width={20} height={20} alt="VSCode extension" />
                      <div>Get VSCode extension</div>
                    </div>
                  </Link>
                </div>
                <div className="flex h-full">
                  <GithubStars />
                </div>
              </div>
            </div>

            <div
              style={{
                height: 'calc(100% - 40px)',
              }}
              className="flex flex-row h-full overflow-clip"
            >
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
          <div></div>

          <div className="relative flex flex-col gap-2 px-2 pb-4">
            <div className="absolute z-10 flex flex-col items-end gap-1 right-8 top-2 text-end">
              <TestToggle />
            </div>
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
  const { showTests, func } = useSelections()

  useEffect(() => {
    setSelection(undefined, undefined, undefined, undefined, false)
  }, [])
  const numTests = func?.test_cases?.length ?? 0

  return (
    <Button
      variant="outline"
      className={clsx(
        'p-1 text-xs w-fit h-fit border-vscode-textSeparator-foreground bg-vscode-button-background gap-x-2 pr-2',
        [!showTests ? 'bg-vscode-button-background' : 'bg-vscode-panel-background'],
      )}
      onClick={() => setSelection(undefined, undefined, undefined, undefined, !showTests)}
    >
      <FlaskConical size={16} />
      <span>{showTests ? 'Hide tests' : `Show  ${numTests > 0 ? numTests : ''} tests`}</span>
    </Button>
  )
}

export default ProjectView
