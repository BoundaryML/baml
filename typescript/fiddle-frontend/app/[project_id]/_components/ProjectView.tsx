'use client'

import { Button } from '@/components/ui/button'
import { ResizableHandle, ResizablePanel, ResizablePanelGroup } from '@/components/ui/resizable'
import { useCommandS } from '@/hooks/command-s'
import { BAML_DIR } from '@/lib/constants'
import { BAMLProject } from '@/lib/exampleProjects'
import { ASTProvider, CustomErrorBoundary, FunctionPanel, FunctionSelector } from '@baml/playground-common'
import { useAtom, useAtomValue, useSetAtom } from 'jotai'
import { useHydrateAtoms } from 'jotai/utils'
import Link from 'next/link'
import { usePathname } from 'next/navigation'
import { useEffect, useRef, useState } from 'react'
import { toast } from 'sonner'
import { Editable } from '../../_components/EditableText'
import { EditorFile, createUrl } from '../../actions'
import { currentEditorFilesAtom, currentParserDbAtom, functionsAndTestsAtom, testRunOutputAtom } from '../_atoms/atoms'
import { CodeMirrorEditor } from './CodeMirrorEditor'
import { usePlaygroundListener } from '../_playground_controller/usePlaygroundListener'

const ProjectViewImpl = ({ project }: { project: BAMLProject }) => {
  const setEditorFiles = useSetAtom(currentEditorFilesAtom)
  const setTestRunOutput = useSetAtom(testRunOutputAtom)
  useCommandS()
  const setFunctionsAndTests = useSetAtom(functionsAndTestsAtom)

  useEffect(() => {
    if (project && project?.files?.length > 0) {
      setEditorFiles([...project.files])
    }
  }, [project.id])
  const [projectName, setProjectName] = useState(project.name)
  const inputRef = useRef(null)

  useEffect(() => {
    if (project) {
      setFunctionsAndTests(project.functionsWithTests)
      if (project.testRunOutput) {
        setTestRunOutput(project.testRunOutput)
      }
      if (project.files) {
        setEditorFiles(project.files)
      }
    }
  }, [project.id])
  console.log('project editor files', project.files)

  return (
    // firefox wont apply the background color for some reason so we forcefully set it.
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
        <ShareButton project={project} projectName={projectName} />

        <div className="flex flex-row justify-center gap-x-1 item-center">
          {/* <TestToggle /> */}
          <Button variant={'ghost'} className="h-full py-1" asChild>
            <Link href="https://docs.boundaryml.com">Docs</Link>
          </Button>
        </div>
      </div>

      <div className="flex flex-row w-full h-full">
        <ResizablePanelGroup className="min-h-[200px] w-full rounded-lg border overflow-clip" direction="horizontal">
          <ResizablePanel defaultSize={50}>
            <div className="flex w-full h-full">
              <CodeMirrorEditor project={project} />
            </div>
          </ResizablePanel>
          <ResizableHandle withHandle className="bg-vscode-contrastActiveBorder" />
          <ResizablePanel defaultSize={50}>
            <div className="flex flex-row h-full bg-vscode-panel-background">
              <PlaygroundView />
            </div>
          </ResizablePanel>
        </ResizablePanelGroup>
      </div>
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
  const functionsAndTests = useAtomValue(functionsAndTestsAtom)
  const editorFiles = useAtomValue(currentEditorFilesAtom)

  return (
    <Button
      variant={'ghost'}
      className="h-full py-1"
      disabled={loading}
      onClick={async () => {
        setLoading(true)
        try {
          let urlId = window.location.pathname.split('/')[1]
          if (!urlId) {
            urlId = await createUrl({
              ...project,
              name: projectName,
              files: editorFiles,
              functionsWithTests: functionsAndTests,
            })

            const newUrl = `${window.location.origin}/${urlId}`
            window.history.replaceState(null, '', newUrl)
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
  const [functionsAndTests] = useAtom(functionsAndTestsAtom)
  usePlaygroundListener()

  useEffect(() => {
    if (!parserDb) {
      return
    }
    const newParserDb = { ...parserDb }

    if (newParserDb.functions.length > 0) {
      functionsAndTests.forEach((func) => {
        const existingFunc = newParserDb.functions.find((f) => f.name.value === func.name.value)
        if (existingFunc) {
          existingFunc.test_cases = func.test_cases
        } else {
          // can happen if you reload and linter hasnt run.
          console.error(`Function ${JSON.stringify(func.name)} not found in parserDb`)
        }
      })
    }
    window.postMessage({
      command: 'setDb',
      content: [[`${BAML_DIR}`, newParserDb]],
    })
  }, [JSON.stringify(parserDb), JSON.stringify(functionsAndTests)])

  return (
    <>
      <CustomErrorBoundary>
        <ASTProvider>
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

export default ProjectView
