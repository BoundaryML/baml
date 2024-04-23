import { EditorFile } from '@/app/actions'
import { StringSpan, TestFileContent, TestRequest } from '@baml/common'
import { useAtom } from 'jotai'
import { useEffect } from 'react'
import { Config, adjectives, animals, colors, uniqueNamesGenerator } from 'unique-names-generator'
import { currentEditorFilesAtom } from '../_atoms/atoms'
import { useTestRunner } from './useTestRunner'
import posthog from 'posthog-js'

const customConfig: Config = {
  dictionaries: [adjectives, colors, animals],
  separator: '_',
  length: 2,
}

export const usePlaygroundListener = () => {
  // const parserDb = useAtomValue(currentParserDbAtom)
  const [editorFiles, setEditorFiles] = useAtom(currentEditorFilesAtom)
  const runTests = useTestRunner()

  // Setup message event listener to handle commands
  useEffect(() => {
    const listener = async (event: any) => {
      const { command, data } = event.data

      switch (command) {
        case 'saveTest':
          // reset the url
          window.history.replaceState(null, '', '/')

          const saveTestRequest: {
            root_path: string
            funcName: string
            testCaseName: StringSpan | undefined | string
            params: any
          } = data

          let fileName
          if (typeof saveTestRequest.testCaseName === 'string') {
            if (saveTestRequest.testCaseName.length > 0) {
              fileName = `${saveTestRequest.testCaseName}.json`
            } else {
              fileName = `${uniqueNamesGenerator(customConfig)}.json`
            }
          } else if (saveTestRequest.testCaseName?.source_file) {
            fileName = saveTestRequest.testCaseName?.source_file.split('/').pop()
          } else {
            fileName = `${uniqueNamesGenerator(customConfig)}.json`
          }

          if (!fileName) {
            console.log(
              'No file name provided for test' +
                saveTestRequest.funcName +
                ' ' +
                JSON.stringify(saveTestRequest.testCaseName),
            )
            return
          }

          const uri = `${saveTestRequest.root_path}/__tests__/${saveTestRequest.funcName}/${fileName}`
          let testInputContent: any
          if (saveTestRequest.params.type === 'positional') {
            // Directly use the value if the type is 'positional'
            try {
              testInputContent = JSON.parse(saveTestRequest.params.value)
            } catch (e) {
              testInputContent = saveTestRequest.params.value
            }
          } else {
            // Create an object from the entries if the type is not 'positional'
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

          const testFileContent: TestFileContent = {
            input: testInputContent,
          }
          console.log(
            'uri',
            uri,
            editorFiles.map((f) => f.path),
          )
          const existingTestFile = editorFiles.find((file) => file.path === uri)

          setEditorFiles((prev) => {
            const prevFiles = prev as EditorFile[]
            return [
              ...prevFiles,
              {
                path: existingTestFile ? `${uri.replaceAll('.json', '')}-${new Date().toISOString()}.json` : uri,
                content: JSON.stringify(testFileContent, null, 2),
              },
            ]
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
          posthog.capture('run_test', { test: data })

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
  }, [editorFiles])

  return <></>
}
