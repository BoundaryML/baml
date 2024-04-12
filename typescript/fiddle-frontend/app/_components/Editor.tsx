'use client'

import CodeMirror, { EditorView, useCodeMirror } from '@uiw/react-codemirror'
import { BAML } from '@baml/codemirror-lang'
import { vscodeDark } from '@uiw/codemirror-theme-vscode'
import { ResizableHandle, ResizablePanel, ResizablePanelGroup } from '@/components/ui/resizable'
import { useCallback, useEffect, useRef, useState } from 'react'
import { ASTProvider, FunctionSelector, FunctionPanel, CustomErrorBoundary } from '@baml/playground-common'
import { linter, Diagnostic } from '@codemirror/lint'
import { Button } from '@/components/ui/button'
import { fetchEventSource } from '@microsoft/fetch-event-source'
import {
  clientEventLogSchema,
  TestStatus,
  ClientEventLog,
  TestRequest,
  TestState as TestStateType,
  getFullTestName,
  ParserDatabase,
  StringSpan,
  SFunction,
} from '@baml/common'
import { atom, useAtom } from 'jotai'
import { atomWithStorage, useHydrateAtoms } from 'jotai/utils'
import { atomStore, sessionStore } from './JotaiProvider'
import Link from 'next/link'
import { atomWithHash } from '@/lib/atomWithHashBase64'
import { createUrl, updateUrl } from '../actions'
import { useFormStatus } from 'react-dom'
import { toast } from 'sonner'
import { usePathname, useRouter, useSearchParams } from 'next/navigation'
import { BAMLProject, exampleProjects } from '@/lib/exampleProjects'
import { Card, CardContent } from '@/components/ui/card'
type EditorFile = {
  path: string
  content: string
}

const functionsAndTestsAtom = atomWithStorage<ParserDatabase['functions']>(
  'parserdb_functions',
  [],
  sessionStore as any,
)
const baml_dir = 'baml_src'
const currentParserDbAtom = atom<ParserDatabase | null>(null)
const currentEditorFilesAtom = atomWithStorage<EditorFile[]>('files', [], sessionStore as any)

async function bamlLinter(view: EditorView): Promise<Diagnostic[]> {
  const lint = await import('@gloo-ai/baml-schema-wasm-web').then((m) => m.lint)
  const linterInput: LinterInput = {
    root_path: `${baml_dir}`,
    files: [
      {
        path: `${baml_dir}/main.baml`,
        content: view.state.doc.toString(),
      },
    ],
  }
  console.info(`Linting ${linterInput.files.length} files in ${linterInput.root_path}`)
  const res = lint(JSON.stringify(linterInput))
  const parsedRes = JSON.parse(res) as LintResponse
  const BamlDB = new Map<string, any>()
  BamlDB.set('baml_src', res)

  if (parsedRes.ok) {
    const newParserDb: ParserDatabase = { ...parsedRes.response }
    atomStore.set(currentParserDbAtom, newParserDb)
  }

  return parsedRes.diagnostics.map((d) => {
    return {
      from: d.start,
      to: d.end,
      message: d.text,
      severity: d.is_warning ? 'warning' : 'error',
    }
  })
}

const extensions = [
  BAML(),
  EditorView.lineWrapping,
  linter(bamlLinter, {
    delay: 200,
    // needsRefresh: (view) => ,
  }),
]

export const EditorContainer = ({ project }: { project: BAMLProject }) => {
  const [editorFiles, setEditorFiles] = useAtom(currentEditorFilesAtom)
  const router = useRouter()
  const pathname = usePathname()
  const searchParams = useSearchParams()
  console.log('project ', project)
  useEffect(() => {
    const handleKeyDown = (event: any) => {
      // Check if either Ctrl+S or Command+S is pressed
      if ((event.ctrlKey || event.metaKey) && (event.key === 's' || event.keyCode === 83)) {
        event.preventDefault()
        console.log('Custom save action triggered')
      }
    }
    window.addEventListener('keydown', handleKeyDown)
    return () => {
      window.removeEventListener('keydown', handleKeyDown)
    }
  }, [])

  useEffect(() => {
    if (project.files.length > 0) {
      setEditorFiles(project.files)
    }
  }, [project.id])

  const [url, setUrl] = useState<string | null>(null)
  const [loading, setLoading] = useState(false)
  const [functionsAndTests, setFunctionsAndTests] = useAtom(functionsAndTestsAtom)
  return (
    // firefox wont apply the background color for some reason so we forcefully set it.
    <div className="flex-col w-full h-full font-sans pl-2flex bg-background dark:bg-vscode-panel-background">
      <div className="flex justify-between border-b-[1px] border-vscode-panel-border h-[40px]">
        <div className="pt-1 pl-4 text-lg font-semibold text-foreground">{project.name}</div>
        <div className="flex flex-row justify-center gap-x-1 item-center">
          <Button
            variant={'ghost'}
            className="h-full py-1"
            disabled={loading}
            onClick={async () => {
              setLoading(true)
              try {
                const allEditorFiles = generateAllEditorFiles(editorFiles, functionsAndTests)
                let urlId = searchParams.get('id')
                console.log('existing url', urlId)
                if (!urlId) {
                  urlId = await createUrl(allEditorFiles)
                  console.log('URL:', urlId)
                  const updatedSearchParams = new URLSearchParams({
                    id: urlId,
                  })
                  router.replace(pathname + '?' + updatedSearchParams.toString(), { scroll: false })
                }

                navigator.clipboard.writeText(`${pathname}?id=${urlId}`)
                toast('URL copied to clipboard')
              } catch (e) {
                toast('Failed to generate URL')
                console.error(e)
              } finally {
                setLoading(false)
              }
              // setUrl(url)
            }}
          >
            Share
          </Button>

          {/* <TestToggle /> */}
          <Button variant={'ghost'} className="h-full py-1" asChild>
            <Link href="https://docs.boundaryml.com">Docs</Link>
          </Button>
        </div>
      </div>

      {url && <div>URL: {url}</div>}
      <div className="flex flex-row w-full h-full">
        <ResizablePanelGroup className="min-h-[200px] w-full rounded-lg border overflow-clip" direction="horizontal">
          <ResizablePanel defaultSize={50}>
            <div className="flex w-full h-full" key={project.id}>
              <CodeMirror
                value={editorFiles[0].content}
                extensions={extensions}
                theme={vscodeDark}
                height="100%"
                width="100%"
                maxWidth="100%"
                style={{ width: '100%', height: '100%' }}
                onChange={async (val, viewUpdate) => {
                  setEditorFiles((prev) => {
                    prev = prev as EditorFile[] // because of jotai jsonstorage this becomes a promise or a normal object and this isnt a promise.
                    const updatedFile: EditorFile = {
                      path: `${baml_dir}/main.baml`,
                      content: val,
                    }
                    return prev.filter((f) => f.path !== f.path).concat(updatedFile)
                  })

                  router.replace(pathname)
                }}
              />
            </div>
          </ResizablePanel>
          <ResizableHandle withHandle className="bg-vscode-contrastActiveBorder" />
          <RunTestButton />

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

export const Editor = ({ project }: { project: BAMLProject }) => {
  useHydrateAtoms([
    [currentEditorFilesAtom as any, project.files],
    [functionsAndTestsAtom as any, project.functionsWithTests],
  ]) // any cause sessionStorage screws types up somehow

  return (
    <>
      <EditorContainer project={project} />
    </>
  )
}

export const DummyHydrate = ({ files }: { files: EditorFile[] }) => {
  useHydrateAtoms([[currentEditorFilesAtom as any, files]]) // any cause sessionStorage screws types up somehow
  return <></>
}

interface UpdateTestCaseEvent {
  project_id: string
  test_cycle_id: string
  test_dataset_name: string
  test_case_definition_name: string
  test_case_arg_name: string // the full test case name we need
  status: TestStatus
  error_data: null | any
}

interface PartialResponseEvent {
  delta: string
  parsed: null | {
    value: string
  }
}

class TestState {
  private test_results: TestStateType
  private active_full_test_name: string | undefined = undefined
  private testStateListener: ((testResults: TestStateType) => void) | undefined = undefined

  constructor() {
    this.handleMessage = this.handleMessage.bind(this)
    this.handleLog = this.handleLog.bind(this)
    this.test_results = {
      results: [],
      test_url: null,
      run_status: 'NOT_STARTED',
    }
  }

  public setTestStateListener(listener: (testResults: TestStateType) => void) {
    this.testStateListener = listener
  }

  public clearTestCases() {
    this.test_results = {
      results: [],
      test_url: null,
      run_status: 'NOT_STARTED',
    }
    this.testStateListener?.(this.test_results)
  }

  public initializeTestCases(tests: TestRequest) {
    this.test_results = {
      results: tests.functions.flatMap((fn) =>
        fn.tests.flatMap((test) =>
          test.impls.map((impl) => ({
            fullTestName: getFullTestName(test.name, impl, fn.name),
            functionName: fn.name,
            testName: test.name,
            implName: impl,
            status: TestStatus.Compiling,
            output: {},
            partial_output: {},
          })),
        ),
      ),
      run_status: 'RUNNING',

      exit_code: undefined,
      test_url: null,
    }
    this.testStateListener?.(this.test_results)
  }

  public handleMessage(data: string) {
    try {
      // Data may be inadvertently concatenated together, but we actually send a \n delimiter between messages to be able to split msgs properly.
      const delimitedData = data.toString().split('<BAML_END_MSG>')
      console.log(`Got a ${delimitedData.length} message`)
      delimitedData
        .map((d) => d.trim())
        .forEach((data) => {
          if (data) {
            this.handleMessageLine(data)
          } else {
            console.log('Empty message')
          }
        })
    } catch (e) {
      console.error(e)
    }
  }

  private handleMessageLine(data: string) {
    const payload = JSON.parse(data) as {
      name: string
      data: any
    }

    console.log('Got message:', payload.name)
    switch (payload.name) {
      case 'test_url':
        this.setTestUrl(payload.data)
        break
      case 'update_test_case':
        this.handleUpdateTestCase(payload.data)
        break
      case 'log':
        let res = clientEventLogSchema.safeParse(payload.data)
        if (!res.success) {
          console.error(`Failed to parse log event: ${JSON.stringify(payload.data, null, 2)}`)
          console.error(res.error)
        } else {
          this.handleLog(payload.data)
        }
        break
      case 'partial_response':
        this.handlePartialResponse(payload.data)
        break
    }
  }

  public setExitCode(code: number | undefined) {
    this.test_results.exit_code = code
    if (code === undefined) {
      this.test_results.run_status = 'NOT_STARTED'
    } else if (code === 0) {
      this.test_results.run_status = 'COMPLETED'
    } else {
      this.test_results.run_status = 'ERROR'
    }

    this.testStateListener?.(this.test_results)
  }

  public getTestResults() {
    return this.test_results
  }

  private setTestUrl(testUrl: { dashboard_url: string }) {
    this.test_results.test_url = testUrl.dashboard_url
    this.test_results.results.forEach((test) => {
      test.status = TestStatus.Queued
    })
    this.testStateListener?.(this.test_results)
  }

  private handleUpdateTestCase(data: UpdateTestCaseEvent) {
    const testResult = this.test_results.results.find((test) => test.fullTestName === data.test_case_arg_name)

    if (testResult) {
      testResult.status = data.status
      if (data.status === TestStatus.Running) {
        this.active_full_test_name = data.test_case_arg_name
      }
      if (data.error_data) {
        testResult.output = {
          error: JSON.stringify(data.error_data),
        }
      }
      this.testStateListener?.(this.test_results)
    }
  }

  private handlePartialResponse(data: PartialResponseEvent) {
    const testResult = this.test_results.results.find((test) => test.fullTestName === this.active_full_test_name)

    if (testResult) {
      testResult.partial_output.raw = `${testResult.partial_output.raw ?? ''}${data.delta}`
      if (data.parsed) {
        testResult.partial_output.parsed = data.parsed.value
      }
      this.testStateListener?.(this.test_results)
    }
  }

  private handleLog(data: ClientEventLog) {
    const fullTestName = data.context.tags?.['test_case_arg_name']
    const testResult = this.test_results.results.find((test) => test.fullTestName === fullTestName)

    if (testResult && data.event_type === 'func_llm') {
      console.log('Found:', fullTestName, JSON.stringify(data, null, 2))
      if (this.test_results.test_url) {
        testResult.url = `${this.test_results.test_url}&s_eid=${data.event_id}&eid=${data.root_event_id}`
      }
      testResult.output = {
        error: data.error?.message ?? testResult.output.error,
        parsed: data.io.output?.value ?? testResult.output.parsed,
        raw: data.metadata?.output?.raw_text ?? testResult.output.raw,
      }
      this.testStateListener?.(this.test_results)
    } else {
      console.log('Not found:', fullTestName, JSON.stringify(data, null, 2))
    }
  }
}

type SaveTestRequest = {
  root_path: string
  funcName: string
  testCaseName: string | StringSpan
  params: {
    type: string
    value: any
  }
}

const serverBaseURL = 'http://localhost:8000'
const prodBaseURL = 'https://prompt-fiddle.fly.dev'
const baseUrl = prodBaseURL

function generateAllEditorFiles(editorFiles: EditorFile[], functionsAndTests: ParserDatabase['functions']) {
  const testFiles: EditorFile[] = functionsAndTests.flatMap((f) => {
    const testFnDir = `${baml_dir}/__tests__/${f.name.value}`
    return f.test_cases.map((test) => ({
      path: `${testFnDir}/${test.name.value}.json`,
      content: JSON.stringify({
        input: test.content,
      }),
    }))
  })

  const updatedEditorFiles = editorFiles
    // map to replace the content of existing files with the same name
    .map((ef) => {
      const newFile = testFiles.find((tf) => tf.path === ef.path)
      return newFile ? newFile : ef
    })

  // Identifying missing files to be added
  const missingFiles = testFiles.filter((tf) => !editorFiles.some((ef) => ef.path === tf.path))

  // Combine updated and missing files for the final list
  return [...updatedEditorFiles, ...missingFiles]
}

const RunTestButton = () => {
  const [data, setData] = useState<string | null>(null)
  const [functionsAndTestsJotai, setFunctionsAndTestsJotai] = useAtom(functionsAndTestsAtom)
  const [parserDb, setParserDb] = useAtom(currentParserDbAtom)
  const [editorFiles, setEditorFiles] = useAtom(currentEditorFilesAtom)

  const fetchData = useCallback(
    async (editorFiles: EditorFile[], testRequest: TestRequest) => {
      console.log('Calling backend' + JSON.stringify(editorFiles))
      const testState = new TestState()

      testState.setTestStateListener((testResults) => {
        window.postMessage({ command: 'test-results', content: testResults })
      })
      testState.initializeTestCases({
        functions: testRequest.functions,
      })
      await fetchEventSource(`${baseUrl}/fiddle`, {
        method: 'POST',

        body: JSON.stringify({
          files: editorFiles.map((f) => {
            return {
              name: f.path,
              content: f.content,
            }
          }),
          testRequest: testRequest,
        }),
        headers: {
          'Content-Type': 'application/json',
          Accept: 'text/event-stream',
        },
        async onopen(res) {
          if (res.ok && res.status === 200) {
            console.log('Connection made ', res)
          } else if (res.status >= 400 && res.status < 500 && res.status !== 429) {
            console.log('Client side error ', res)
            const result = await res.text()
            console.log('stream result:', result)
          }
        },
        onmessage(event) {
          // TODO: fix these
          if (event.data.includes('<BAML_PORT>:')) {
            const messageWithoutPort = event.data.replace('<BAML_PORT>: ', '')

            testState.handleMessage(messageWithoutPort)
          } else {
            const msg = event.data.replaceAll('<BAML_STDOUT>:', '')
            window.postMessage({ command: 'test-stdout', content: msg })
          }
        },
        onclose() {
          console.log('Connection closed by the server')
          testState.setExitCode(0) // unsure if both onerror and this get called yet.
        },
        onerror(err) {
          console.error('Error in event source', err)
          testState.setExitCode(5)
          throw err // rethrow to stop the event source
        },
      })
    },
    [JSON.stringify(editorFiles)],
  )
  // Setup message event listener to handle commands
  useEffect(() => {
    let shadowedState = { functionsAndTests: functionsAndTestsJotai }
    const listener = async (event: any) => {
      const { command, data } = event.data
      console.log('running command', { event, command, data })

      switch (command) {
        case 'receiveData':
          // Example of showing received information, adapt as necessary
          // alert(data.text)
          break

        case 'commandSequence':
          console.log('received command sequence', data)
          for (const subcommand of data) {
            console.log('received command in sequence', subcommand)
            await listener({ data: subcommand })
          }
          break

        case 'saveTest':
          const saveTestRequest = data as SaveTestRequest
          // Save test data to localStorage
          const { root_path, funcName, testCaseName, params } = saveTestRequest
          const fileName: string = typeof testCaseName === 'string' ? `${testCaseName}.json` : 'default.json' // Simplified fileName logic
          const filePath = `${root_path}/__tests__/${funcName}/${fileName}`

          let testInputContent: any
          if (params.type === 'positional') {
            try {
              testInputContent = JSON.parse(params.value)
            } catch (e) {
              testInputContent = params.value
            }
          } else {
            testInputContent = Object.fromEntries(
              saveTestRequest.params.value.map((kv: { name: any; value: any }) => {
                if (kv.value === undefined || kv.value === null || kv.value === '') {
                  return [kv.name, null]
                }
                let parsed: any
                try {
                  parsed = JSON.parse(kv.value)
                } catch (e) {
                  parsed = kv.value
                }
                return [kv.name, parsed]
              }),
            )
          }
          const newTestCase = {
            name: {
              start: 0,
              end: 0,
              value: typeof testCaseName === 'string' ? testCaseName : testCaseName.value,
              source_file: filePath,
            },
            content: testInputContent,
          }

          shadowedState.functionsAndTests = ((current) => {
            current = current as SFunction[]
            // If current is empty or does not contain the function, add a new entry
            if (!current.some((func) => (typeof func.name === 'string' ? func.name : func.name.value) === funcName)) {
              // find it from parserDb

              const existingFunc = parserDb?.functions.find((f) => {
                const parserDbFuncName = typeof f.name === 'string' ? f.name : f.name.value
                return parserDbFuncName === funcName
              })
              if (existingFunc) {
                return current.concat({
                  ...existingFunc,
                  test_cases: [newTestCase],
                })
              }
            }

            // If the function exists, update its test cases
            return current.map((func) => {
              const currFuncName = typeof func.name === 'string' ? func.name : func.name.value
              if (currFuncName === funcName) {
                // Update existing function's test cases
                return {
                  ...func,
                  test_cases: func.test_cases
                    .filter((test) => test.name !== testCaseName) // Remove existing test case with the same name
                    .concat([newTestCase]), // Add the new test case
                }
              }
              return func // Return unmodified function
            })
          })(shadowedState.functionsAndTests)
          break

        case 'removeTest':
          const { root_path: removeRootPath, funcName: removeFuncName, testCaseName: removeTestCaseName } = data

          shadowedState.functionsAndTests = ((prev) => {
            return (prev as SFunction[]).map((func) => {
              // Check if this is the function from which to remove the test
              const currFuncName = typeof func.name === 'string' ? func.name : func.name.value
              if (currFuncName === removeFuncName) {
                // Filter out the test case to be removed
                const updatedTestCases = func.test_cases.filter((test) => {
                  const testName = typeof test.name === 'string' ? test.name : test.name.value
                  return testName !== removeTestCaseName.value
                })

                // Return the function with the updated list of test cases
                return { ...func, test_cases: updatedTestCases }
              }

              // Return all other functions unmodified
              return func
            })
          })(shadowedState.functionsAndTests)
          break
        case 'runTest':
          const testRequest: { root_path: string; tests: TestRequest } = event.data.data
          const finalEditorFiles = generateAllEditorFiles(editorFiles, shadowedState.functionsAndTests)
          fetchData(finalEditorFiles, testRequest.tests)
          break
        default:
      }
    }

    const eventListener = async (event: any) => {
      await listener(event)
      setFunctionsAndTestsJotai(shadowedState.functionsAndTests)
    }

    window.addEventListener('message', eventListener)
    return () => {
      window.removeEventListener('message', eventListener)
    }
  }, [JSON.stringify(functionsAndTestsJotai), JSON.stringify(parserDb)])

  return <></>
}

type LintResponse = {
  diagnostics: LinterError[]
} & (
  | { ok: false }
  | {
      ok: true
      response: ParserDatabase
    }
)

export interface LinterError {
  start: number
  end: number
  text: string
  is_warning: boolean
  source_file: string
}

export interface LinterSourceFile {
  path: string
  content: string
}

export interface LinterInput {
  root_path: string
  files: LinterSourceFile[]
}

const PlaygroundView = () => {
  const [parserDb, setParserDb] = useAtom(currentParserDbAtom)
  const [functionsAndTests, setFunctionsAndTests] = useAtom(functionsAndTestsAtom)
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
      content: [[`${baml_dir}`, newParserDb]],
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

export default Editor
