import { EditorFile } from '@/app/actions'
import { TestRequest } from '@baml/common'
import { fetchEventSource } from '@microsoft/fetch-event-source'
import { useCallback } from 'react'
import { TestState } from './TestState'
import { useAtom, useSetAtom } from 'jotai'
import { testRunOutputAtom } from '../_atoms/atoms'

const serverBaseURL = 'http://localhost:8000'
const prodBaseURL = 'https://prompt-fiddle.fly.dev'
const baseUrl = prodBaseURL

export const useTestRunner = () => {
  const setTestRunOutput = useSetAtom(testRunOutputAtom)
  const fetchData = useCallback(async (editorFiles: EditorFile[], testRequest: TestRequest) => {
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
  }, [])

  return fetchData
}
