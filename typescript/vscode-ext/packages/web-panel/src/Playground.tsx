import { ParserDatabase, TestResult, TestStatus } from '@baml/common'
import {
  VSCodeButton,
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
        <VSCodeButton className="flex justify-end ml-auto h-7" onClick={() => {}}>
          Docs
        </VSCodeButton>
        {/* {func && (
          <VSCodeButton
            className="flex justify-end h-7"
            onClick={() => {
              if (func) {
                vscode.postMessage({
                  command: 'jumpToFile',
                  data: func.name,
                })
              }
            }}
          >
            Go to definition
          </VSCodeButton>
        )} */}
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
                      console.log('updatedValue', updatedValue)
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
        </div>
      )}
      {/* variant tabs */}
      {func && (
        <VSCodePanels
          className="w-full"
          activeid={impl ? selectedId.implName : undefined}
          onChange={(e) => {
            const newImplId = (e.target as any)?.activetab?.id
            setSelectedId((prev) => ({ ...prev, implName: newImplId }))
          }}
        >
          {func.impls.map((impl, index) => (
            <>
              <VSCodePanelTab key={index} id={impl.name.value}>
                {impl.name.value}
              </VSCodePanelTab>
              <VSCodePanelView id={impl.name.value} className="p-0"></VSCodePanelView>
            </>
          ))}
        </VSCodePanels>
      )}
      <div className="w-full pb-4 px-0.5">
        <Separator className="bg-vscode-textSeparator-foreground" />
      </div>
      {func && impl && (
        <>
          <div className="flex flex-col gap-1 overflow-y-scroll h-[50%]">
            {/* <div className="flex flex-row gap-1">
              <VSCodeLink onClick={() => vscode.postMessage({ command: 'jumpToFile', data: impl.name })}>
              <span className="font-bold">File</span> {impl.name.source_file}
            </div> */}
            <div className="flex flex-row gap-1">
              <span className="font-bold">Client</span>{' '}
              <VSCodeLink onClick={() => vscode.postMessage({ command: 'jumpToFile', data: impl?.client })}>
                {impl.client.value}
              </VSCodeLink>
            </div>
            <div className="flex flex-row gap-x-2">
              <b>Prompt</b>
              <div>(view only)</div>
            </div>

            <pre className="w-full p-2 overflow-y-scroll whitespace-pre-wrap select-none bg-vscode-input-background">
              {prompt}
            </pre>
          </div>
          <div className="py-3 w-fit">
            <VSCodeButton
              className="flex justify-end h-7"
              onClick={() => {
                if (!func || !impl) {
                  return
                }
                let runTestRequest
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
                  // Construct params for named arguments
                  const namedParams = multiArgValues.reduce((acc, arg) => {
                    acc[arg.name] = arg.value
                    return acc
                  }, {} as NamedParams)

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
                              value: namedParams,
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
          </div>
          <TestOutputBox />
        </>
      )}
    </main>
  )
}

export const TestOutputBox = () => {
  const [testOutput, setTestOutput] = useState<string>('')
  const [status, setStatus] = useState<TestStatus | undefined>()
  useEffect(() => {
    const fn = (event: any) => {
      const command = event.data.command
      const messageContent = event.data.content

      switch (command) {
        case 'stdout': {
          setTestOutput((prev) => (prev ? `${prev}\n${messageContent}` : messageContent))
          break
        }
        case 'reset-stdout': {
          setTestOutput('')
          break
        }
        case 'test-results': {
          console.log('test-results', messageContent)
          const testResults = messageContent as TestResult[]
          const testResult = testResults[0]
          setTestOutput(testResult.output)
          setStatus(testResult.status)
        }
      }
    }

    // TODO: these listeners probably need to go in some seaprate provider, as we are likely losing msgs anytime this component rerenders.
    window.addEventListener('message', fn)

    return () => {
      window.removeEventListener('message', fn)
    }
  })

  return (
    <div className="flex flex-col gap-1 h-[20%] pb-8">
      <div className="flex flex-row items-center gap-x-2">
        <div>
          <b>Output</b>
        </div>
        {/* TODO: Use icons */}
        {status && (
          <div className="text-vscode-descriptionForeground">
            {
              {
                [TestStatus.Queued]: 'Queued',
                [TestStatus.Running]: <VSCodeProgressRing className="h-4" />,
                [TestStatus.Passed]: <div className="text-vscode-testing-iconPassed">Passed</div>,
                [TestStatus.Failed]: <div className="text-vscode-testing-iconFailed">Failed</div>,
              }[status]
            }
          </div>
        )}
      </div>

      <div className="max-w-full">
        {/* {testOutput ? (

        ) : (

        )} */}
        <pre className="w-full h-full min-h-[80px] p-1 overflow-y-scroll break-words whitespace-break-spaces bg-vscode-input-background">
          {testOutput ? (
            testOutput
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
