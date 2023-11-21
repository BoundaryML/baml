import CustomErrorBoundary from '@/utils/ErrorFallback'
import { ParserDatabase, TestResult } from '@baml/common'
import { VSCodeButton } from '@vscode/webview-ui-toolkit/react'
import React, { PropsWithChildren, createContext, useCallback, useEffect, useMemo, useState } from 'react'

export const ASTContext = createContext<{
  root_path: string
  db: ParserDatabase
  test_results: TestResult[]
  selections: {
    selectedFunction: string | undefined
    selectedImpl: string | undefined
    selectedTestCase: string | undefined
  }
  setSelection: (
    functionName: string | undefined,
    implName: string | undefined,
    testCaseName: string | undefined,
  ) => void
}>({
  root_path: '',
  db: {
    functions: [],
    classes: [],
    clients: [],
    enums: [],
  },
  test_results: [],
  selections: {
    selectedFunction: undefined,
    selectedImpl: undefined,
    selectedTestCase: undefined,
  },
  setSelection: () => {},
})

function useSelectionSetup() {
  const [selectedFunction, setSelectedFunction] = useState<string | undefined>(undefined)
  const [selectedImpl, setSelectedImpl] = useState<string | undefined>(undefined)
  const [selectedTestCase, setSelectedTestCase] = useState<string | undefined>(undefined)

  const setSelectionFunction = useCallback(
    (functionName: string | undefined, implName: string | undefined, testCaseName: string | undefined) => {
      console.log('setSelectionFunction', functionName, implName, testCaseName)
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
    },
    [],
  )

  return {
    selectedFunction,
    selectedImpl,
    selectedTestCase,
    setSelection: setSelectionFunction,
  }
}

export const ASTProvider: React.FC<PropsWithChildren<any>> = ({ children }) => {
  const [projects, setProjects] = useState<{ root_dir: string; db: ParserDatabase }[]>([])
  const [selectedProjectId, setSelectedProjectId] = useState<string | undefined>(undefined)
  const [testResults, setTestResults] = useState<TestResult[]>([])
  const { selectedFunction, selectedImpl, selectedTestCase, setSelection } = useSelectionSetup()

  const selectedState = useMemo(() => {
    if (selectedProjectId === undefined) return undefined
    let match = projects.find((project) => project.root_dir === selectedProjectId)
    if (match) {
      return {
        root_path: match.root_dir,
        db: match.db,
        test_results: testResults,
        selections: {
          selectedFunction,
          selectedImpl,
          selectedTestCase,
        },
        setSelection,
      }
    }
    return undefined
  }, [projects, selectedProjectId, testResults, selectedFunction, selectedImpl, selectedTestCase, setSelection])

  useEffect(() => {
    setSelectedProjectId((prev) => prev ?? projects[0]?.root_dir)
  }, [projects])

  useEffect(() => {
    const fn = (event: any) => {
      const command = event.data.command
      const messageContent = event.data.content

      switch (command) {
        case 'setDb': {
          setProjects(messageContent.map((p: any) => ({ root_dir: p[0], db: p[1] })))
          break
        }
        case 'rmDb': {
          setProjects((prev) => prev.filter((project) => project.root_dir !== messageContent))
          break
        }
        case 'setSelectedResource': {
          let content = messageContent as {
            functionName: string | undefined
            implName?: string
            testCaseName?: string
          }
          setSelection(content.functionName, content.implName, content.testCaseName)
          break
        }
        case 'test-results': {
          console.log('REACT test-results', messageContent)
          setTestResults(messageContent as TestResult[])
          break
        }
      }
    }
    console.log('REACT adding event listener')
    window.addEventListener('message', fn)

    return () => {
      console.log('REACT removing event listener')
      window.removeEventListener('message', fn)
    }
  }, [])

  return (
    <main className="w-full h-screen py-2">
      {selectedState === undefined ? (
        projects.length === 0 ? (
          <div>Loading...</div>
        ) : (
          <div>
            <h1>Projects</h1>
            <div>
              {projects.map((project) => (
                <div key={project.root_dir}>
                  <VSCodeButton onClick={() => setSelectedProjectId(project.root_dir)}>{project.root_dir}</VSCodeButton>
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
