import CustomErrorBoundary from '../utils/ErrorFallback'
import { ParserDatabase, TestState } from '@baml/common'
import { VSCodeButton } from '@vscode/webview-ui-toolkit/react'
import React, { PropsWithChildren, createContext, useCallback, useEffect, useMemo, useState } from 'react'

export const ASTContext = createContext<{
  projects: { root_dir: string; db: ParserDatabase }[]
  selectedProjectId: string
  root_path: string
  db: ParserDatabase
  jsonSchema: {
    definitions: { [k: string]: any }
  }
  test_results?: TestState
  test_log?: string
  selections: {
    selectedFunction: string | undefined
    selectedImpl: string | undefined
    selectedTestCase: string | undefined
    showTests: boolean
  }
  setSelection: (
    selectedProjectId: string | undefined,
    functionName: string | undefined,
    implName: string | undefined,
    testCaseName: string | undefined,
    showTests: boolean | undefined,
  ) => void
}>({
  projects: [],
  selectedProjectId: '',
  root_path: '',
  db: {
    functions: [],
    classes: [],
    clients: [],
    enums: [],
  },
  jsonSchema: {
    definitions: {},
  },
  test_log: undefined,
  test_results: undefined,
  selections: {
    selectedFunction: undefined,
    selectedImpl: undefined,
    selectedTestCase: undefined,
    showTests: true,
  },
  setSelection: () => {},
})

function useSelectionSetup() {
  const [selectedProjectId, setSelectedProjectId] = useState<string | undefined>(undefined)
  const [selectedFunction, setSelectedFunction] = useState<string | undefined>(undefined)
  const [selectedImpl, setSelectedImpl] = useState<string | undefined>(undefined)
  const [selectedTestCase, setSelectedTestCase] = useState<string | undefined>(undefined)
  const [showTests, setShowTests] = useState<boolean>(true)

  const setSelectionFunction = useCallback(
    (
      selectedProjectId: string | undefined,
      functionName: string | undefined,
      implName: string | undefined,
      testCaseName: string | undefined,
      showTests: boolean | undefined,
    ) => {
      if (selectedProjectId) {
        setSelectedProjectId(selectedProjectId)
      }
      if (functionName) {
        setSelectedFunction(functionName)
        setSelectedImpl(implName)
        setSelectedTestCase(testCaseName)
      } else {
        if (implName) {
          setSelectedImpl(implName)
        }
        if (testCaseName) {
          setSelectedTestCase(testCaseName)
        }
      }
      if (showTests !== undefined) {
        setShowTests(showTests)
      } else if (testCaseName !== undefined) {
        setShowTests(true)
      }
    },
    [],
  )

  return {
    selectedProjectId,
    selectedFunction,
    selectedImpl,
    selectedTestCase,
    showTests,
    setSelection: setSelectionFunction,
  }
}

export const ASTProvider: React.FC<PropsWithChildren<any>> = ({ children }) => {
  const [projects, setProjects] = useState<{ root_dir: string; db: ParserDatabase }[]>([])
  const [testResults, setTestResults] = useState<TestState | undefined>(undefined)
  const { selectedProjectId, selectedFunction, selectedImpl, selectedTestCase, showTests, setSelection } =
    useSelectionSetup()
  const [testLog, setTestLog] = useState<string | undefined>(undefined)

  const selectedState = useMemo(() => {
    if (selectedProjectId === undefined) return undefined
    let match = projects.find((project) => project.root_dir === selectedProjectId)
    if (match) {
      let jsonSchema = {
        definitions: Object.fromEntries([
          ...match.db.classes.flatMap((c) => Object.entries(c.jsonSchema)),
          ...match.db.enums.flatMap((c) => Object.entries(c.jsonSchema)),
        ]),
      }
      return {
        projects,
        selectedProjectId,
        root_path: match.root_dir,
        db: match.db,
        jsonSchema: jsonSchema,
        test_results: testResults,
        test_log: testLog,
        selections: {
          selectedFunction,
          selectedImpl,
          selectedTestCase,
          showTests,
        },
        setSelection,
      }
    }
    return undefined
  }, [
    projects,
    selectedProjectId,
    testResults,
    selectedFunction,
    selectedImpl,
    selectedTestCase,
    showTests,
    setSelection,
  ])

  useEffect(() => {
    if (projects.length === 0) return
    if (selectedProjectId === undefined) {
      setSelection(projects[0].root_dir, undefined, undefined, undefined, undefined)
    }
  }, [selectedProjectId, projects])

  useEffect(() => {
    const fn = (event: any) => {
      // console.log('event.data', event.data)
      const command = event.data.command
      const messageContent = event.data.content

      switch (command) {
        case 'test-stdout': {
          if (messageContent === '<BAML_RESTART>') {
            setTestLog(undefined)
          } else {
            setTestLog((prev) => (prev ? prev + messageContent : messageContent))
          }
          break
        }
        case 'setDb': {
          console.log('parser db : ' + JSON.stringify(messageContent, undefined, 2))
          if (messageContent && messageContent !== '') {
            setProjects(messageContent.map((p: any) => ({ root_dir: p[0], db: p[1] })))
          }
          break
        }
        case 'rmDb': {
          setProjects((prev) => prev.filter((project) => project.root_dir !== messageContent))
          break
        }
        case 'setSelectedResource': {
          let content = messageContent as {
            projectId: string | undefined
            functionName: string | undefined
            implName?: string
            testCaseName?: string
            showTests?: boolean
          }
          setSelection(
            content.projectId,
            content.functionName,
            content.implName,
            content.testCaseName,
            content.showTests,
          )
          break
        }
        case 'test-results': {
          setTestResults(messageContent as TestState)
          break
        }
      }
    }
    window.addEventListener('message', fn)

    return () => {
      window.removeEventListener('message', fn)
    }
  }, [])

  return (
    <main className="w-full h-screen px-0 py-2 overflow-y-clip">
      {selectedState === undefined ? (
        projects.length === 0 ? (
          <div>
            No baml projects loaded yet.
            <br />
            Open a baml file or wait for the extension to finish loading!
          </div>
        ) : (
          <div>
            <h1>Projects</h1>
            <div>
              {projects.map((project) => (
                <div key={project.root_dir}>
                  <VSCodeButton
                    onClick={() => setSelection(project.root_dir, undefined, undefined, undefined, undefined)}
                  >
                    {project.root_dir}
                  </VSCodeButton>
                </div>
              ))}
            </div>
          </div>
        )
      ) : (
        <CustomErrorBoundary>
          <ASTContext.Provider value={selectedState}>{children}</ASTContext.Provider>
        </CustomErrorBoundary>
      )}
    </main>
  )
}
