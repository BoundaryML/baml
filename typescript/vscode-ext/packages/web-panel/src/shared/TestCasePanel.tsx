/// Content once a function has been selected.

import { ParserDatabase, StringSpan, TestRequest } from '@baml/common'
import { useSelections } from './hooks'
import TypeComponent from './TypeComponent'
import { VSCodeButton, VSCodeTextArea, VSCodeTextField } from '@vscode/webview-ui-toolkit/react'
import { useContext, useEffect, useMemo, useState } from 'react'
import { vscode } from '@/utils/vscode'
import { ASTContext } from './ASTProvider'
import { Accordion, AccordionContent, AccordionItem, AccordionTrigger } from '@/components/ui/accordion'
import { Separator } from '@/components/ui/separator'
import { Button } from '@/components/ui/button'
import { Edit, Edit2, Play } from 'lucide-react'
import { TestRunRequest } from 'vscode'
import { RJSFSchema, UiSchema } from '@rjsf/utils'
import validator from '@rjsf/validator-ajv8'
import { Dialog, DialogContent, DialogTrigger } from '@/components/ui/dialog'
import Form from '@rjsf/core'

const schema: RJSFSchema = {
  title: 'Test form',
  type: 'object',
  properties: {
    name: {
      type: 'string',
    },
    age: {
      type: 'number',
    },
  },
}

const uiSchema: UiSchema = {
  name: {
    'ui:classNames': 'custom-class-name',
  },
  age: {
    'ui:classNames': 'custom-class-age',
  },
}

type Func = ParserDatabase['functions'][0]

const TestCasePanel: React.FC<{ func: Func }> = ({ func }) => {
  const test_cases = func?.test_cases.map((cases) => cases) ?? []
  const { impl } = useSelections()

  const getTestParams = (testCase: Func['test_cases'][0]): TestRequest['functions'][0]['tests'][0]['params'] => {
    if (func.input.arg_type === 'positional') {
      return {
        type: 'positional',
        value: testCase.content,
      }
    } else {
      let parsed = JSON.parse(testCase.content)
      let contentMap = new Map<string, string>()
      if (typeof parsed === 'object') {
        contentMap = new Map(Object.entries(parsed).map(([k, v]) => [k, JSON.stringify(v, null, 2)]))
      }
      return {
        type: 'named',
        value: func.input.values.map(({ name }) => ({
          name: name.value,
          value: contentMap.get(name.value) ?? '',
        })),
      }
    }
  }
  // const { test_case } = useSelections()
  return (
    <>
      <div>Test Cases</div>
      <div className="flex flex-row justify-between">
        <VSCodeTextField placeholder="Search test cases" />
        <VSCodeButton
          onClick={() => {
            // vscode.postMessage({
            //   command: 'addTestCase',
            //   data: {
            //     funcName: func.name.value,
            //   },
            // })
          }}
        >
          Run all tests
        </VSCodeButton>
      </div>
      <div className="flex flex-col py-4 divide-y gap-y-4 divide-vscode-descriptionForeground">
        {test_cases.map((test_case) => (
          <div key={test_case.name.value}>
            <div className="flex flex-row items-center gap-x-1">
              <Button
                variant={'ghost'}
                size={'icon'}
                className="p-1 w-fit h-fit"
                onClick={() => {
                  const runTestRequest: TestRequest = {
                    functions: [
                      {
                        name: func.name.value,
                        tests: [
                          {
                            name: test_case.name.value,
                            params: getTestParams(test_case),
                            impls: impl ? [impl.name.value] : [],
                          },
                        ],
                      },
                    ],
                  }
                  vscode.postMessage({
                    command: 'runTest',
                    data: runTestRequest,
                  })
                }}
              >
                <Play size={10} />
              </Button>
              <div>{test_case.name.value}</div>
              <EditTestCaseForm />
            </div>
            <TestCaseCard content={test_case.content} testCaseName={test_case.name.value} />
          </div>
        ))}
      </div>
    </>
  )

  // return (
  //   <div className="flex flex-col">
  //     {func.input.arg_type === 'positional' ? (
  //       <PositionalTestCase input={func.input.type} content={test_case?.content} />
  //     ) : (
  //       <NamedTestCase values={func.input.values} content={test_case?.content} />
  //     )}
  //   </div>
  // )
}

const EditTestCaseForm = ({}: {}) => {
  return (
    <Dialog>
      <DialogTrigger asChild={true}>
        <Button variant={'ghost'} size="icon" className="p-1 w-fit h-fit">
          <Edit2 className="w-3 h-3 text-vscode-descriptionForeground" />
        </Button>
      </DialogTrigger>
      <DialogContent className="bg-vscode-editorWidget-background border-vscode-descriptionForeground">
        <Form schema={schema} validator={validator} />
      </DialogContent>
    </Dialog>
  )
}

const TestCaseCard: React.FC<{ testCaseName: string; content: string }> = ({ testCaseName, content }) => {
  return (
    <div className="flex flex-col gap-2 text-xs text-vscode-descriptionForeground">
      <div>
        {content.substring(0, 200)}
        {content.length > 200 && '...'}
      </div>
    </div>
  )
}

const PositionalTestCase: React.FC<{ input: string; content: string | undefined }> = ({ input, content }) => {
  const [singleArgValue, setSingleArgValue] = useState(content ?? '')

  return (
    <div className="flex flex-col gap-2">
      <div className="flex flex-row items-baseline gap-1">
        <span>input</span>
        <div className="text-xs italic">
          <TypeComponent typeString={input} />
        </div>
      </div>

      <VSCodeTextArea
        placeholder="Enter the input as json like { 'hello': 'world' } or a string"
        className="w-full"
        resize="vertical"
        value={singleArgValue}
        onInput={(e: any) => {
          setSingleArgValue(e.target.value)
        }}
      />
      <TestButtons
        data={() => ({
          type: 'positional',
          value: singleArgValue,
        })}
      />
    </div>
  )
}

const NamedTestCase: React.FC<{ values: { name: StringSpan; type: string }[]; content: string | any | undefined }> = ({
  values,
  content: raw_content,
}) => {
  const [content, setContent] = useState(new Map<string, string>())

  useEffect(() => {
    if (!raw_content) setContent(new Map<string, string>())

    try {
      const parsed = JSON.parse(raw_content)
      if (typeof parsed === 'object') {
        // As a key value pair
        setContent(new Map(Object.entries(parsed).map(([k, v]) => [k, JSON.stringify(v, null, 2)])))
      }
    } catch (e) {}
  }, [raw_content])

  return (
    <div className="flex flex-col gap-2">
      <div className="flex flex-col gap-1">
        {typeof content === 'string' ? (
          <div className="text-red-500">
            Something went wrong. Expect a json, but got invalid content. Open the file and fix it manually.
          </div>
        ) : (
          <>
            {values.map(({ name, type }) => (
              <div key={name.value} className="flex flex-col gap-1">
                <div className="flex flex-row items-baseline gap-1">
                  <span>{name.value}</span>
                  <div className="text-xs italic">
                    <TypeComponent typeString={type} />
                  </div>
                </div>
                <VSCodeTextArea
                  placeholder="Enter the input as json like { 'hello': 'world' } or a string"
                  className="w-full"
                  resize="vertical"
                  value={content.get(name.value) ?? ''}
                  onInput={(e: any) => {
                    setContent((c) => {
                      c.set(name.value, e.target.value)
                      return c
                    })
                  }}
                />
              </div>
            ))}
            <TestButtons
              data={() => ({
                type: 'named',
                value: values.map(({ name }) => ({
                  name: name.value,
                  value: content.get(name.value) ?? '',
                })),
              })}
            />
          </>
        )}
      </div>
    </div>
  )
}

const TestButtons: React.FC<{ data: () => TestRequest['functions'][0]['tests'][0]['params'] }> = ({ data }) => {
  const {
    func: { name: func_name, impls } = {},
    impl: { name } = {},
    test_case: { name: testCaseName } = {},
  } = useSelections()

  if (!name || !impls || !func_name) return <i>No impls for this function</i>

  return (
    <div className="flex flex-row justify-between">
      <div className="flex flex-row gap-2">
        <RunImplButton
          funcName={func_name.value}
          testCaseName={testCaseName?.value ?? 'playground'}
          implName={name.value}
          data={data}
        />
        {impls.length > 1 && (
          <RunAllImplsButton
            funcName={func_name.value}
            testCaseName={testCaseName?.value ?? 'playground'}
            impls={impls.map((i) => i.name.value)}
            data={data}
          />
        )}
      </div>
      <div>
        <SaveButton funcName={func_name.value} testCaseName={testCaseName} data={data} />
      </div>
    </div>
  )
}

const SaveButton: React.FC<{
  funcName: string
  testCaseName: StringSpan | undefined
  data: () => TestRequest['functions'][0]['tests'][0]['params']
}> = ({ funcName, testCaseName, data }) => {
  const { root_path } = useContext(ASTContext)
  return (
    <VSCodeButton
      className="flex justify-end"
      onClick={() => {
        vscode.postMessage({
          command: 'saveTest',
          data: {
            root_path,
            funcName,
            testCaseName,
            params: data(),
          },
        })
      }}
    >
      Save
    </VSCodeButton>
  )
}

const RunAllImplsButton: React.FC<{
  funcName: string
  testCaseName: string
  impls: string[]
  data: () => TestRequest['functions'][0]['tests'][0]['params']
}> = ({ funcName, testCaseName, impls, data }) => {
  return (
    <VSCodeButton
      className="flex justify-end"
      onClick={() => {
        const runTestRequest: TestRequest = {
          functions: [
            {
              name: funcName,
              tests: [
                {
                  name: testCaseName,
                  params: data(),
                  impls,
                },
              ],
            },
          ],
        }
        vscode.postMessage({
          command: 'runTest',
          data: runTestRequest,
        })
      }}
    >
      Run all impls
    </VSCodeButton>
  )
}

const RunImplButton: React.FC<{
  funcName: string
  testCaseName: string
  implName: string
  data: () => TestRequest['functions'][0]['tests'][0]['params']
}> = ({ funcName, testCaseName, implName, data }) => {
  return (
    <VSCodeButton
      className="flex justify-end"
      onClick={() => {
        const runTestRequest: TestRequest = {
          functions: [
            {
              name: funcName,
              tests: [
                {
                  name: testCaseName,
                  params: data(),
                  impls: [implName],
                },
              ],
            },
          ],
        }
        vscode.postMessage({
          command: 'runTest',
          data: runTestRequest,
        })
      }}
    >
      Run impl: {implName}
    </VSCodeButton>
  )
}

export default TestCasePanel
