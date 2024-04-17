import { ParserDatabase, SFunction, StringSpan, TestRequest } from '@baml/common'
import { useAtom, useAtomValue } from 'jotai'
import { useEffect } from 'react'
import { functionsAndTestsAtom, currentParserDbAtom, currentEditorFilesAtom } from '../_atoms/atoms'
import { useTestRunner } from './useTestRunner'
import { EditorFile } from '@/app/actions'
import { BAML_DIR } from '@/lib/constants'
import { ParserDBFunctionTestModel } from '@/lib/exampleProjects'
type SaveTestRequest = {
  root_path: string
  funcName: string
  testCaseName: string | StringSpan
  params: {
    type: string
    value: any
  }
}

function generateAllEditorFiles(editorFiles: EditorFile[], functionsAndTests: ParserDBFunctionTestModel[]) {
  const testFiles: EditorFile[] = functionsAndTests.flatMap((f) => {
    const testFnDir = `${BAML_DIR}/__tests__/${f.name.value}`
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

export const usePlaygroundListener = () => {
  const [functionsAndTestsJotai, setFunctionsAndTestsJotai] = useAtom(functionsAndTestsAtom)
  const parserDb = useAtomValue(currentParserDbAtom)
  const editorFiles = useAtomValue(currentEditorFilesAtom)
  const runTests = useTestRunner()

  // Setup message event listener to handle commands
  useEffect(() => {
    let shadowedState = { functionsAndTests: functionsAndTestsJotai }
    const listener = async (event: any) => {
      const { command, data } = event.data

      switch (command) {
        case 'receiveData':
          // Example of showing received information, adapt as necessary
          // alert(data.text)
          break

        case 'commandSequence':
          // console.log('received command sequence', data)
          for (const subcommand of data) {
            // console.log('received command in sequence', subcommand)
            await listener({ data: subcommand })
          }
          break

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
              testInputContent = JSON.stringify(JSON.parse(params.value))
            } catch (e) {
              testInputContent = JSON.stringify(params.value)
            }
          } else {
            testInputContent = JSON.stringify(
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
              ),
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
          window.history.replaceState(null, '', '/')

          const testRequest: { root_path: string; tests: TestRequest } = event.data.data
          const finalEditorFiles = generateAllEditorFiles(editorFiles, shadowedState.functionsAndTests)
          runTests(finalEditorFiles, testRequest.tests)
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
