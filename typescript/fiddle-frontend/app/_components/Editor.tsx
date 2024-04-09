'use client'

import CodeMirror, { EditorView, useCodeMirror } from '@uiw/react-codemirror'
import { BAML } from '@baml/codemirror-lang'
import { vscodeDark } from '@uiw/codemirror-theme-vscode'
import { ResizableHandle, ResizablePanel, ResizablePanelGroup } from '@/components/ui/resizable'
import { useEffect, useRef, useState } from 'react'
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
} from '@baml/common'

async function bamlLinter(view: EditorView): Promise<Diagnostic[]> {
  const lint = await import('@gloo-ai/baml-schema-wasm-web').then((m) => m.lint)
  const linterInput: LinterInput = {
    root_path: 'project/baml_src',
    files: [
      {
        path: 'project/baml_src/main.baml',
        content: view.state.doc.toString(),
      },
    ],
  }
  console.info(`Linting ${linterInput.files.length} files in ${linterInput.root_path}`)
  const res = lint(JSON.stringify(linterInput))
  const parsedRes = JSON.parse(res) as LintResponse
  console.log(`res ${JSON.stringify(res, null, 2)}`)
  const BamlDB = new Map<string, any>()
  // res is of type ParserDB
  BamlDB.set('baml_src', res)

  if (parsedRes.ok) {
    const newParserDb: ParserDatabase = { ...parsedRes.response }
    // console.log('newParserDb', newParserDb)
    if (newParserDb.functions.length > 0) {
      console.log('modifying functions array')
      newParserDb.functions[0].test_cases.push({
        name: {
          start: 0,
          end: 0,
          value: 'test1',
          source_file: 'baml_src/__tests__/ExtractVerbs/test1.json',
        },
        content: defaultTestFile,
      })
    }
    console.log('newParserDb', newParserDb)
    window.postMessage({
      command: 'setDb',
      content: [['project/baml_src', newParserDb]],
    })
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
const defaultMainBaml = `
generator lang_python {
  language python
  // This is where your non-baml source code located
  // (relative directory where pyproject.toml, package.json, etc. lives)
  project_root ".."
  // This command is used by "baml test" to run tests
  // defined in the playground
  test_command "pytest -s"
  // This command is used by "baml update-client" to install
  // dependencies to your language environment
  install_command "poetry add baml@latest"
  package_version_command "poetry show baml"
}

function ExtractVerbs {
    input string
    /// list of verbs
    output string[]
}

client<llm> GPT4 {
  provider baml-openai-chat
  options {
    model gpt-4 
    api_key env.OPENAI_API_KEY
  }
}

impl<llm, ExtractVerbs> version1 {
  client GPT4
  prompt #"
    Extract the verbs from this INPUT:
 
    INPUT:
    ---
    {#input}
    ---
    {// this is a comment inside a prompt! //}
    Return a {#print_type(output)}.

    Response:
  "#
}
`

const defaultTestFile = `
{
  "input": "Lou and Jim Whittaker built the Rainier climbing culture Rainier from scratch. There were no outfitters anywhere near the mountain, so they took over a building and made their own store. There was nowhere to get a beer after a summit day, so they built Whittaker’s Bunkhouse bar and hotel to serve guests with more than 30 rooms. There was nowhere to throw a party after a successful trip, so Lou bought a 12-foot-by-six-foot barrel from a company that made pickles and built a hot tub that could hold 18 naked people during big celebrations."
}
`

export const Editor = () => {
  const [value, setValue] = useState(defaultMainBaml)

  useEffect(() => {
    const handleKeyDown = (event: any) => {
      // Check if either Ctrl+S or Command+S is pressed
      if ((event.ctrlKey || event.metaKey) && (event.key === 's' || event.keyCode === 83)) {
        event.preventDefault()
        // Place your custom save logic here
        console.log('Custom save action triggered')
      }
    }

    // Add the event listener
    window.addEventListener('keydown', handleKeyDown)

    // Remove the event listener on cleanup
    return () => {
      window.removeEventListener('keydown', handleKeyDown)
    }
  }, [])

  useEffect(() => {
    const lintWithWasm = async () => {}
    lintWithWasm()
  }, [value])

  return (
    <>
      <ResizablePanelGroup className="min-h-[200px] w-full rounded-lg border overflow-clip" direction="horizontal">
        <ResizablePanel defaultSize={50}>
          <div className="flex w-full h-full">
            <CodeMirror
              value={value}
              extensions={extensions}
              theme={vscodeDark}
              height="100%"
              width="100%"
              maxWidth="100%"
              style={{ width: '100%', height: '100%' }}
              onChange={async (val, viewUpdate) => {
                setValue(val)
              }}
            />
          </div>
        </ResizablePanel>
        <ResizableHandle withHandle />
        <RunTestButton />

        <ResizablePanel defaultSize={50}>
          <div className="flex flex-row h-full bg-vscode-panel-background">
            <PlaygroundView />
          </div>
        </ResizablePanel>
      </ResizablePanelGroup>
    </>
  )
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

      // outputChannel.appendLine(JSON.stringify(e, null, 2))
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

const serverBaseURL = 'http://localhost:8000'
const RunTestButton = () => {
  const [data, setData] = useState<string | null>(null)

  const fetchData = async () => {
    const testState = new TestState()

    testState.setTestStateListener((testResults) => {
      window.postMessage({ command: 'test-results', content: testResults })
    })
    testState.initializeTestCases({
      functions: [
        {
          name: 'ExtractVerbs',
          tests: [
            {
              name: 'test1',
              impls: ['version1'],
            },
          ],
        },
      ],
    })
    await fetchEventSource(`${serverBaseURL}/fiddle`, {
      method: 'POST',

      body: JSON.stringify({
        files: [
          {
            name: 'baml_src/main.baml',
            content: defaultMainBaml,
          },
          {
            name: 'baml_src/__tests__/ExtractVerbs/test1.json',
            content: defaultTestFile,
          },
        ],
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
        console.log('Message received')
        console.log(event.data)
        // only send messages that have PORT: , and don't include the PORT: part
        if (event.data.includes('PORT:')) {
          const messageWithoutPort = event.data.replace('PORT: ', '')

          testState.handleMessage(messageWithoutPort)
          //  setData((currentData) => currentData + messageWithoutPort)
        } else {
          //  window.postMessage({ command: 'test-stdout', content: event.data })
        }

        // testState.handleMessage(event.data)

        // setData((currentData) => currentData + (event.data ?? ''))
        //const parsedData = JSON.parse(event.data)
        //setData((currentData) => [...currentData, parsedData])
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
  }

  return (
    <>
      <Button
        onClick={async () => {
          console.log('Running test')
          fetchData()
        }}
      >
        Run Test
      </Button>
      <>{data && <pre>{JSON.stringify(data, null, 2)}</pre>}</>
    </>
  )
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
  return (
    <>
      <CustomErrorBoundary>
        <ASTProvider>
          <div className="absolute z-10 flex flex-col items-end gap-1 right-1 top-2 text-end">
            {/* <TestToggle /> */}
            {/* <VSCodeLink href="https://docs.boundaryml.com">Docs</VSCodeLink> */}
          </div>
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
