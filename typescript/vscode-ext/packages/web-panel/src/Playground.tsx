import { ParserDatabase, TestRequest, TestResult, TestStatus, getFullTestName } from '@baml/common'
import {
  VSCodeBadge,
  VSCodeButton,
  VSCodeCheckbox,
  VSCodeDivider,
  VSCodeDropdown,
  VSCodeLink,
  VSCodeOption,
  VSCodePanelTab,
  VSCodePanelView,
  VSCodePanels,
  VSCodeProgressRing,
  VSCodeTextArea,
} from '@vscode/webview-ui-toolkit/react'
import { useEffect, useMemo, useState } from 'react'
import { vscode } from './utils/vscode'
import { Separator } from './components/ui/separator'

interface NamedParams {
  [key: string]: string
}

const Playground: React.FC<{ project: ParserDatabase }> = ({ project: { functions } }) => {
  let [selectedId, setSelectedId] = useState<{
    functionName: string | undefined
    implName: string | undefined
  }>({ functionName: functions.at(0)?.name.value, implName: functions.at(0)?.impls.at(0)?.name.value })

  let { func, impl, prompt } = useMemo(() => {
    let func = functions.find((func) => func.name.value === selectedId.functionName)
    let impl = func?.impls.find((impl) => impl.name.value === selectedId.implName)
    console.log('memo func', func, 'impl', impl, 'selectedId', selectedId)

    let prompt = impl?.prompt ?? ''
    impl?.input_replacers.forEach(({ key, value }) => {
      prompt = prompt.replaceAll(key, `{${value}}`)
    })
    impl?.output_replacers.forEach(({ key, value }) => {
      prompt = prompt.replaceAll(key, value)
    })
    return { func, impl, prompt }
  }, [selectedId, functions])

  const [singleArgValue, setSingleArgValue] = useState<string>('')
  const [multiArgValues, setMultiArgValues] = useState<
    {
      name: string
      value: string
    }[]
  >([])

  useEffect(() => {
    // Check if there's a need to update the implName
    if (!impl && func) {
      // Determine the default implementation name
      // Here, we're using the first implementation of the selected function
      const defaultImplName = func.impls.at(0)?.name.value

      // Update the selectedId state only if the defaultImplName is different from the current one
      if (selectedId.implName !== defaultImplName) {
        setSelectedId((prev) => ({ ...prev, implName: defaultImplName }))
      }
    }
  }, [func, impl, selectedId.implName])

  // jump to definition when the function and/or impl changes
  useEffect(() => {
    if (impl) {
      vscode.postMessage({
        command: 'jumpToFile',
        data: impl.name,
      })
    } else if (func) {
      vscode.postMessage({
        command: 'jumpToFile',
        data: func.name,
      })
    }
  }, [func?.name.value, impl?.name.value])

  const [testResults, setTestResults] = useState<TestResult[]>([])

  useEffect(() => {
    if (!impl) {
      return
    }
    const fn = (event: any) => {
      const command = event.data.command
      const messageContent = event.data.content

      switch (command) {
        case 'test-results': {
          const testResults = messageContent as TestResult[]
          setTestResults(testResults)
        }
      }
    }

    // TODO: these listeners probably need to go in some seaprate provider, as we are likely losing msgs anytime this component rerenders.
    window.addEventListener('message', fn)

    return () => {
      window.removeEventListener('message', fn)
    }
  }, [])

  const selectedTestResult = testResults.find((testResult) => {
    const testName = getFullTestName('mytest', impl?.name.value ?? '', func?.name.value ?? '')
    return testName === testResult.fullTestName
  })

  return (
    <main className="w-full h-screen py-2">
      <div className="flex flex-row justify-between">
        <div className="justify-start">
          <VSCodeDropdown
            className="mr-1"
            value={selectedId.functionName ?? '<unset>'}
            onChange={(event) =>
              setSelectedId((prev) => ({
                ...prev,
                functionName: (event as React.FormEvent<HTMLSelectElement>).currentTarget.value,
              }))
            }
          >
            {functions.map((func, index) => (
              <VSCodeOption key={index} value={func.name.value}>
                {func.name.value}
              </VSCodeOption>
            ))}
          </VSCodeDropdown>
        </div>
        <VSCodeLink className="flex justify-end ml-auto h-7" href="https://docs.trygloo.com">
          Docs
        </VSCodeLink>
      </div>
      {func && (
        <div className="flex flex-col">
          <span className="font-bold">Test Case</span>
          {func.input.arg_type === 'positional' ? (
            <div className="flex flex-col gap-1">
              <span className="font-bold">
                arg: <span className="font-normal">{func.input.type}</span>
              </span>
              <VSCodeTextArea
                placeholder='Enter the input as json like { "hello": "world" } or a string'
                className="w-full"
                resize="vertical"
                value={singleArgValue}
                onInput={(e: any) => {
                  setSingleArgValue(e?.target?.value ?? undefined)
                }}
              />
            </div>
          ) : (
            <div className="flex flex-col gap-1">
              {func.input.values.map((argValue, index) => (
                <div className="flex flex-col gap-1">
                  <span className="font-bold">
                    {argValue.name.value}: <span className="font-normal">{argValue.type}</span>
                  </span>
                  <VSCodeTextArea
                    placeholder='Enter the input as json like { "hello": "world" } or a string'
                    className="w-full"
                    resize="vertical"
                    value={multiArgValues.find((arg) => arg.name === argValue.name.value)?.value || ''}
                    onInput={(e: any) => {
                      const updatedValue = e.target.value
                      setMultiArgValues((prevValues) => {
                        const index = prevValues.findIndex((arg) => arg.name === argValue.name.value)
                        if (index >= 0) {
                          // If the argument exists, update its value
                          return [
                            ...prevValues.slice(0, index),
                            { ...prevValues[index], value: updatedValue },
                            ...prevValues.slice(index + 1),
                          ]
                        } else {
                          // If the argument doesn't exist, add it to the array
                          return [...prevValues, { name: argValue.name.value, value: updatedValue }]
                        }
                      })
                    }}
                  />
                </div>
              ))}
            </div>
          )}
          <div className="w-fit">
            <RunButton func={func} impl={impl} singleArgValue={singleArgValue} multiArgValues={multiArgValues} />
          </div>
        </div>
      )}
      {/* variant tabs */}
      {func && (
        <VSCodePanels
          className="w-full"
          key={func.name.value + impl?.name.value}
          activeid={impl ? selectedId.implName : undefined}
          onChange={(e) => {
            const newImplId = (e.target as any)?.activetab?.id
            setSelectedId((prev) => ({ ...prev, implName: newImplId }))
          }}
        >
          {func.impls.map((impl, index) => {
            const testStatus = testResults.find((testResult) => {
              const testName = getFullTestName('mytest', impl.name.value, func?.name.value ?? '')
              return testName === testResult.fullTestName
            })?.status
            return (
              <>
                <VSCodePanelTab key={index} id={impl.name.value}>
                  {impl.name.value}{' '}
                  {testStatus && (
                    <VSCodeBadge>
                      <TestStatusIcon testStatus={testStatus} />
                    </VSCodeBadge>
                  )}
                </VSCodePanelTab>
                <VSCodePanelView id={impl.name.value} className="p-0"></VSCodePanelView>
              </>
            )
          })}
        </VSCodePanels>
      )}
      <div className="w-full pb-4 px-0.5">
        <Separator className="bg-vscode-textSeparator-foreground" />
      </div>
      <ImplView impl={impl} func={func} testResult={selectedTestResult} prompt={prompt} />
    </main>
  )
}

const ImplView = ({
  impl,
  func,
  prompt,
  testResult,
}: {
  impl?: ParserDatabase['functions'][0]['impls'][0]
  func?: ParserDatabase['functions'][0]
  prompt: string
  testResult?: TestResult
}) => {
  const [showPrompt, setShowPrompt] = useState<boolean>(false)
  if (!impl || !func) {
    return null
  }
  return (
    <div className="flex flex-col">
      <TestOutputBox key={func.name.value + impl.name.value} testResult={testResult} />

      <div className="flex flex-col gap-0 overflow-y-scroll h-[50%] pb-6">
        <div className="flex flex-row justify-between">
          <div>
            <div className="flex flex-row gap-1">
              <span className="font-bold">Client</span>{' '}
              <VSCodeLink onClick={() => vscode.postMessage({ command: 'jumpToFile', data: impl?.client })}>
                {impl.client.value}
              </VSCodeLink>
            </div>
            {showPrompt && (
              <div className="flex flex-row gap-x-2">
                <b>Prompt</b>
                <VSCodeLink
                  onClick={() => {
                    vscode.postMessage({ command: 'jumpToFile', data: impl?.name })
                  }}
                >
                  Edit
                </VSCodeLink>
              </div>
            )}
          </div>
          <div>
            <VSCodeCheckbox checked={showPrompt} onChange={(e) => setShowPrompt((e.currentTarget as any).checked)}>
              Show Prompt
            </VSCodeCheckbox>
          </div>
        </div>

        {showPrompt && (
          <pre className="w-full p-2 overflow-y-scroll whitespace-pre-wrap select-none bg-vscode-input-background">
            {prompt}
          </pre>
        )}
      </div>
    </div>
  )
}

const RunButton = ({
  func,
  impl,
  singleArgValue,
  multiArgValues,
}: {
  func: ParserDatabase['functions'][0]
  impl?: ParserDatabase['functions'][0]['impls'][0]
  singleArgValue: string
  multiArgValues: {
    name: string
    value: string
  }[]
}) => {
  if (!impl) {
    return null
  }
  return (
    <VSCodeButton
      className="flex justify-end h-7"
      onClick={() => {
        if (!func || !impl) {
          return
        }
        let runTestRequest: TestRequest
        if (func.input.arg_type === 'positional') {
          runTestRequest = {
            functions: [
              {
                name: func.name.value,
                tests: [
                  {
                    name: 'mytest',
                    impls: [impl.name.value],
                    params: {
                      type: 'positional',
                      value: singleArgValue,
                    },
                  },
                ],
              },
            ],
          }
        } else {
          runTestRequest = {
            functions: [
              {
                name: func.name.value,
                tests: [
                  {
                    name: 'mytest',
                    impls: [impl.name.value],
                    params: {
                      type: 'named',
                      value: multiArgValues,
                    },
                  },
                ],
              },
            ],
          }
        }
        vscode.postMessage({
          command: 'runTest',
          data: runTestRequest,
        })
      }}
    >
      Run
    </VSCodeButton>
  )
}

const TestStatusIcon = ({ testStatus }: { testStatus: TestStatus }) => {
  return (
    <div className="text-vscode-descriptionForeground">
      {
        {
          [TestStatus.Queued]: 'Queued',
          [TestStatus.Running]: <VSCodeProgressRing className="h-4" />,
          [TestStatus.Passed]: <div className="text-vscode-testing-iconPassed">Passed</div>,
          [TestStatus.Failed]: <div className="text-vscode-testing-iconFailed">Failed</div>,
        }[testStatus]
      }
    </div>
  )
}
export const TestOutputBox = ({ testResult }: { testResult?: TestResult }) => {
  if (!testResult) {
    return null
  }

  return (
    <div className="flex flex-col gap-1 h-[20%] pb-8">
      <div className="flex flex-row items-center gap-x-2">
        <div>
          <b>Output</b>
        </div>
        {testResult.status && <TestStatusIcon testStatus={testResult.status} />}
      </div>

      <div className="max-w-full">
        <pre className="w-full h-full min-h-[80px] p-1 overflow-y-scroll break-words whitespace-break-spaces bg-vscode-input-background">
          {testResult.output ? (
            testResult.output
          ) : (
            <div className="flex flex-col items-center justify-center h-full text-vscode-descriptionForeground">
              <div>Nothing here yet...</div>
            </div>
          )}
        </pre>
      </div>
    </div>
  )
}

const FunctionPlayground: React.FC<{ func: ParserDatabase['functions'][0] }> = ({ func }) => {
  return null
}

export default Playground
