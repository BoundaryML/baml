import { ParserDatabase, SFunction, StringSpan, TestRequest } from '@baml/common'
import { useAtom, useAtomValue, useSetAtom } from 'jotai'
import { useEffect } from 'react'
import { currentParserDbAtom, currentEditorFilesAtom } from '../_atoms/atoms'
import { useTestRunner } from './useTestRunner'
import { EditorFile } from '@/app/actions'
import { BAML_DIR } from '@/lib/constants'
type SaveTestRequest = {
  root_path: string
  funcName: string
  testCaseName: string | StringSpan
  params: {
    type: string
    value: any
  }
}

export const usePlaygroundListener = () => {
  // const parserDb = useAtomValue(currentParserDbAtom)
  const setEditorFiles = useSetAtom(currentEditorFilesAtom)
  const runTests = useTestRunner()

  // Setup message event listener to handle commands
  useEffect(() => {
    const listener = async (event: any) => {
      const { command, data } = event.data

      switch (command) {
        case 'saveTest':
          // reset the url
          window.history.replaceState(null, '', '/')

          const saveTestRequest = data as SaveTestRequest
          console.log('savetestreq', saveTestRequest)
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
            testInputContent =
              Object.fromEntries(
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
              );
          }

          setEditorFiles(prev => {
            prev = prev as EditorFile[]
            prev.push({
              path: filePath,
              content: JSON.stringify({ input: testInputContent }, null, 2),
            })
            return prev
          })
          break

        case 'removeTest':
          const { root_path: removeRootPath, funcName: removeFuncName, testCaseName: removeTestCaseName } = data

          setEditorFiles((prev) => {
            return (prev as EditorFile[]).filter((file) => {
              return file.path !== `${removeRootPath}/__tests__/${removeFuncName}/${removeTestCaseName}.json`
            })
          })
          break
        case 'runTest':
          window.history.replaceState(null, '', '/')

          const testRequest: { root_path: string; tests: TestRequest } = event.data.data
          await runTests(testRequest.tests)
          break
        default:
      }
    }

    const eventListener = async (event: any) => {
      await listener(event)
    }

    window.addEventListener('message', eventListener)
    return () => {
      window.removeEventListener('message', eventListener)
    }
  }, [])

  return <></>
}
