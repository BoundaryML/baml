import { TestRequest } from '@baml/common'
import { fetchEventSource } from '@microsoft/fetch-event-source'
import { useSetAtom } from 'jotai'
import { useAtomCallback } from 'jotai/utils'
import { currentEditorFilesAtom, testRunOutputAtom } from '../_atoms/atoms'
import { TestState } from './TestState'
import posthog from 'posthog-js'

const serverBaseURL = 'http://localhost:8000'
const prodBaseURL = 'https://prompt-fiddle.fly.dev'
const baseUrl = prodBaseURL

export const useTestRunner = () => {
  const setTestRunOutput = useSetAtom(testRunOutputAtom)
  const fetchData = useAtomCallback(async (get, set, testRequest: TestRequest) => {
    const editorFiles = await get(currentEditorFilesAtom)
    const testState = new TestState()

    setTestRunOutput((prev) => {
      return {
        testState: {
          results: [],
          run_status: 'RUNNING',
          exit_code: undefined,
          test_url: null,
        },
        outputLogs: [],
      }
    })

    testState.setTestStateListener((testResults) => {
      window.postMessage({ command: 'test-results', content: testResults })
      setTestRunOutput((prev) => {
        if (prev) {
          return {
            ...prev,
            testState: testResults,
          }
        } else {
          return {
            testState: testResults,
            outputLogs: [],
          }
        }
      })
      if (testResults.run_status === 'ERROR') {
        posthog.capture('test_run_error', {
          testResults: testResults,
          testRequest: testRequest,
        })
      } else if (testResults.run_status === 'COMPLETED') {
        posthog.capture('test_run_finished', {
          testResults: testResults,
          testRequest: testRequest,
        })
      }
    })
    console.log('initialize test cases')
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
          const result = await res.clone().text()
          console.log('stream result:', result)
          window.postMessage({ command: 'test-stdout', content: result })
          testState.setExitCode(5)
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
          setTestRunOutput((prev) => {
            if (prev) {
              return {
                ...prev,
                outputLogs: prev.outputLogs.concat(msg),
              }
            } else {
              return {
                testState: {
                  results: [],
                  run_status: 'RUNNING',
                  exit_code: undefined,
                  test_url: null,
                },
                outputLogs: [msg],
              }
            }
          })
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
  })

  return fetchData
}
