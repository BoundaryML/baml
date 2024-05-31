import {
  type ClientEventLog,
  ParserDatabase,
  SFunction,
  StringSpan,
  type TestRequest,
  type TestState as TestStateType,
  TestStatus,
  clientEventLogSchema,
  getFullTestName,
} from '@baml/common'

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

export class TestState {
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
        const res = clientEventLogSchema.safeParse(payload.data)
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
      // this means it already errored out:
    } else if (code === 0 && this.test_results.run_status !== 'ERROR') {
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
