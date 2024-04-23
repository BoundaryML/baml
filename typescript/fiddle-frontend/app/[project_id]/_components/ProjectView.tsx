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
import Joyride, { STATUS } from 'react-joyride'
import {
  currentEditorFilesAtom,
  currentParserDbAtom,
  productTourDoneAtom,
  testRunOutputAtom,
  unsavedChangesAtom,
} from '../_atoms/atoms'
import { usePlaygroundListener } from '../_playground_controller/usePlaygroundListener'
import { CodeMirrorEditor } from './CodeMirrorEditor'
import { GithubStars } from './GithubStars'
import FileViewer from './Tree/FileViewer'
import clsx from 'clsx'
import { AlertTriangleIcon, FlaskConical, GitForkIcon, LinkIcon, ShareIcon } from 'lucide-react'
import { Separator } from '@baml/playground-common/components/ui/separator'
import { Tour } from './Tour'

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
  const projectNameInputRef = useRef(null)
  const [description, setDescription] = useState(project.description)
  const descriptionInputRef = useRef(null)
  const productTourDone = useAtomValue(productTourDoneAtom)

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
    <div className="relative flex flex-row w-full h-full bg-gray-800 main-panel overflow-x-clip overflow-y-clip">
      <ResizablePanelGroup className="w-full h-full overflow-clip" direction="horizontal">
        <ResizablePanel defaultSize={12} className="h-full bg-zinc-900">
          <div className="w-full pt-2 text-lg italic font-bold text-center">Prompt Fiddle</div>

          <ResizablePanelGroup className="h-full" direction="vertical">
            <ResizablePanel defaultSize={50} className="h-full ">
              <div className="w-full px-2 text-sm font-semibold text-center uppercase text-white/90">project files</div>
              <div className="flex flex-col w-full h-full tour-file-view">
                <FileViewer />
              </div>
            </ResizablePanel>
            <Separator className="bg-vscode-textSeparator-foreground" />

            <ResizableHandle className="bg-vscode-contrastActiveBorder border-vscode-contrastActiveBorder" />
            <ResizablePanel className="w-full pt-2 tour-templates">
              <div className="w-full px-2 pt-2 text-sm font-semibold text-center uppercase text-white/90">
                Templates
              </div>
              <div className="flex flex-col px-4 gap-y-4">
                {exampleProjects.map((p) => {
                  return <ExampleProjectCard key={p.name} project={p} />
                })}
              </div>
            </ResizablePanel>
          </ResizablePanelGroup>
        </ResizablePanel>
        <ResizableHandle className=" bg-vscode-contrastActiveBorder border-vscode-contrastActiveBorder" />
        <ResizablePanel defaultSize={88}>
          <div className="flex-col w-full h-full font-sans bg-background dark:bg-vscode-panel-background">
            <div className="flex flex-row items-center gap-x-12 border-b-[1px] border-vscode-panel-border min-h-[40px]">
              <div className="flex flex-col items-center h-full py-1 tour-title whitespace-nowrap">
                <Editable
                  text={projectName}
                  placeholder="Write a task name"
                  type="input"
                  childRef={projectNameInputRef}
                >
                  <input
                    className="px-2 text-lg border-none text-foreground"
                    type="text"
                    ref={projectNameInputRef}
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
              <div className="flex items-center justify-start h-full pt-0.5 w-full">
                <Button asChild variant={'ghost'} className="h-full py-1 gap-x-1">
                  <Link
                    href="https://docs.boundaryml.com"
                    target="_blank"
                    className="text-sm hover:text-foreground text-foreground "
                  >
                    What is BAML?
                  </Link>
                </Button>
              </div>

              {/* <div className="flex flex-row justify-center gap-x-1 item-center">
                <Button variant={'ghost'} className="h-full py-1" asChild>
                  <Link target="_blank" href="https://docs.boundaryml.com">
                    Docs
                  </Link>
                </Button>
              </div> */}

              {unsavedChanges ? (
                <div className="flex flex-row items-center whitespace-nowrap text-muted-foreground">
                  <Badge variant="outline" className="font-light text-yellow-400 gap-x-2">
                    <AlertTriangleIcon size={14} />
                    <span>Unsaved changes</span>
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
                  <div className="flex flex-col w-full py-1 pl-2 text-xs border-none items-left h-fit whitespace-nowrap">
                    <Editable
                      text={description}
                      placeholder="Write a task name"
                      type="input"
                      childRef={descriptionInputRef}
                      className="w-full px-2 text-sm font-light text-left border-none text-card-foreground"
                    >
                      <textarea
                        className="w-[95%] ml-2 px-2 text-sm border-none text-vscode-descriptionForeground"
                        // type="text"
                        ref={descriptionInputRef}
                        name="task"
                        placeholder="Write a description"
                        value={description}
                        onChange={(e) => setDescription(e.target.value)}
                      />
                    </Editable>
                  </div>
                  <div className="flex w-full h-full tour-editor">
                    <CodeMirrorEditor project={project} />
                  </div>
                </ResizablePanel>
                <ResizableHandle className="bg-vscode-contrastActiveBorder" />
                <ResizablePanel defaultSize={50} className="tour-playground">
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
  const [unsavedChanges, setUnsavedChanges] = useAtom(unsavedChangesAtom)

  return (
    <Button
      variant={'ghost'}
      className="h-full py-1 gap-x-1"
      disabled={loading}
      onClick={async () => {
        setLoading(true)
        try {
          let urlId = pathname.split('/')[1]
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
      {unsavedChanges ? <GitForkIcon size={14} /> : <LinkIcon size={14} />}
      <span>{unsavedChanges ? 'Fork & Share' : 'Share'}</span>
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
          <Tour />
        </ASTProvider>
      </CustomErrorBoundary>
    </>
  )
}

const TestToggle = () => {
  const { setSelection } = useContext(ASTContext)
  const { showTests, func } = useSelections()

  // useEffect(() => {
  //   setSelection(undefined, undefined, undefined, undefined, false)
  // }, [])
  const numTests = func?.test_cases?.length ?? 0

  return (
    <Button
      variant="outline"
      className={clsx(
        'tour-test-button p-1 text-xs w-fit h-fit border-vscode-textSeparator-foreground bg-vscode-button-background gap-x-2 pr-2',
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
