/// Content once a function has been selected.

import { ParserDatabase, StringSpan, TestRequest } from '@baml/common'
import { useSelections } from './hooks'
import TypeComponent from './TypeComponent'
import { VSCodeButton, VSCodeTextArea } from '@vscode/webview-ui-toolkit/react'
import { useContext, useEffect, useMemo, useState } from 'react'
import { vscode } from '@/utils/vscode'
import { ASTContext } from './ASTProvider'

type Func = ParserDatabase['functions'][0]

const TestCasePanel: React.FC<{ func: Func }> = ({ func }) => {
  const { test_case, input_json_schema } = useSelections()

  return (
    <div className="flex flex-col">
      <pre>{JSON.stringify(input_json_schema, undefined, 2)}</pre>
      {func.input.arg_type === 'positional' ? (
        <PositionalTestCase input={func.input.type} content={test_case?.content} />
      ) : (
        <NamedTestCase values={func.input.values} content={test_case?.content} />
      )}
    </div>
  )
}

const PositionalTestCase: React.FC<{ input: string; content: string | undefined }> = ({ input, content }) => {
  const [singleArgValue, setSingleArgValue] = useState(content ?? '')

  return (
    <div className="flex flex-col gap-2">
      <div className="flex flex-row gap-1 items-baseline">
        <span>input</span>
        <div className="italic text-xs">
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
                <div className="flex flex-row gap-1 items-baseline">
                  <span>{name.value}</span>
                  <div className="italic text-xs">
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
